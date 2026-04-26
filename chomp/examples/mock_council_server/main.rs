use std::{collections::HashMap, env, sync::Arc};

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::Mutex,
};
use tracing::{error, info, warn};

type BlobStore = Arc<Mutex<HashMap<String, Vec<u8>>>>;

#[derive(Debug)]
struct CliArgs {
    bind: String,
}

#[derive(Debug)]
struct HttpRequest {
    method: String,
    path: String,
    body: Vec<u8>,
}

fn usage() -> &'static str {
    "Usage: cargo run --example mock_council_server -- [--bind <host:port>]\n\
     \n\
     Defaults to COUNCIL_BIND_ADDR or 127.0.0.1:8080.\n\
     \n\
     Exposes:\n\
       POST /push/<sha256-hex>  to store serialized Borsh payload bytes\n\
       GET  /pull/<sha256-hex>  to fetch serialized Borsh payload bytes"
}

fn parse_cli_args() -> Result<Option<CliArgs>> {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut bind = env::var("COUNCIL_BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".to_string());
    let mut i = 0usize;

    while i < args.len() {
        let arg = &args[i];

        if arg == "--bind" || arg == "-b" {
            bind = args
                .get(i + 1)
                .context("Missing value for --bind flag")?
                .clone();
            i += 2;
            continue;
        }
        if arg == "--help" || arg == "-h" {
            return Ok(None);
        }
        if let Some(value) = arg.strip_prefix("--bind=") {
            bind = value.to_string();
            i += 1;
            continue;
        }
        if arg.starts_with('-') {
            bail!("Unknown flag '{}'\n\n{}", arg, usage());
        }

        bail!("Unexpected positional argument '{}'\n\n{}", arg, usage());
    }

    Ok(Some(CliArgs { bind }))
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

fn parse_content_length(headers: &str) -> Result<usize> {
    for line in headers.lines().skip(1) {
        if let Some((name, value)) = line.split_once(':')
            && name.trim().eq_ignore_ascii_case("content-length")
        {
            return value
                .trim()
                .parse::<usize>()
                .context("Invalid Content-Length header");
        }
    }

    Ok(0)
}

async fn read_http_request(stream: &mut TcpStream) -> Result<HttpRequest> {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 4096];
    let header_end = loop {
        let read = stream
            .read(&mut chunk)
            .await
            .context("Failed to read from council client")?;
        if read == 0 {
            bail!("Client closed the connection before sending a complete request");
        }
        buffer.extend_from_slice(&chunk[..read]);
        if let Some(position) = find_header_end(&buffer) {
            break position;
        }
        if buffer.len() > 1024 * 1024 {
            bail!("Request headers are too large");
        }
    };

    let headers =
        std::str::from_utf8(&buffer[..header_end]).context("Request was not valid UTF-8")?;
    let mut lines = headers.lines();
    let request_line = lines.next().context("Missing HTTP request line")?;
    let mut request_parts = request_line.split_whitespace();
    let method = request_parts
        .next()
        .context("Missing HTTP method")?
        .to_string();
    let path = request_parts
        .next()
        .context("Missing HTTP path")?
        .to_string();
    let _version = request_parts.next().context("Missing HTTP version")?;
    let content_length = parse_content_length(headers)?;

    let body_start = header_end + 4;
    let mut body = buffer[body_start..].to_vec();
    while body.len() < content_length {
        let read = stream
            .read(&mut chunk)
            .await
            .context("Failed while reading request body")?;
        if read == 0 {
            bail!("Client closed the connection before sending the full request body");
        }
        body.extend_from_slice(&chunk[..read]);
    }
    body.truncate(content_length);

    Ok(HttpRequest { method, path, body })
}

fn extract_hash(path: &str, prefix: &str) -> Option<String> {
    let value = path.strip_prefix(prefix)?;
    if value.len() != 64 || !value.as_bytes().iter().all(u8::is_ascii_hexdigit) {
        return None;
    }
    Some(value.to_ascii_lowercase())
}

fn compute_sha256_hex(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}

async fn write_response(
    stream: &mut TcpStream,
    status_line: &str,
    content_type: Option<&str>,
    body: &[u8],
) -> Result<()> {
    let content_type_header = content_type
        .map(|content_type| format!("Content-Type: {content_type}\r\n"))
        .unwrap_or_default();
    let headers = format!(
        "HTTP/1.1 {status_line}\r\n{content_type_header}Content-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream
        .write_all(headers.as_bytes())
        .await
        .context("Failed to write response headers")?;
    stream
        .write_all(body)
        .await
        .context("Failed to write response body")?;
    stream.flush().await.context("Failed to flush response")?;
    Ok(())
}

async fn handle_request(mut stream: TcpStream, blobs: BlobStore) -> Result<()> {
    let request = read_http_request(&mut stream).await?;

    match (request.method.as_str(), request.path.as_str()) {
        ("POST", path) => {
            let Some(hash) = extract_hash(path, "/push/") else {
                write_response(
                    &mut stream,
                    "400 Bad Request",
                    Some("text/plain; charset=utf-8"),
                    b"expected POST /push/<64-char sha256 hex>\n",
                )
                .await?;
                return Ok(());
            };

            let computed_hash = compute_sha256_hex(&request.body);
            if computed_hash != hash {
                let message = format!(
                    "path hash did not match request body hash: expected {hash}, got {computed_hash}\n"
                );
                write_response(
                    &mut stream,
                    "400 Bad Request",
                    Some("text/plain; charset=utf-8"),
                    message.as_bytes(),
                )
                .await?;
                return Ok(());
            }

            let blob_len = request.body.len();
            blobs.lock().await.insert(hash.clone(), request.body);
            info!("stored {} bytes under {}", blob_len, hash);
            write_response(&mut stream, "204 No Content", None, b"").await?;
        }
        ("GET", path) => {
            let Some(hash) = extract_hash(path, "/pull/") else {
                write_response(
                    &mut stream,
                    "400 Bad Request",
                    Some("text/plain; charset=utf-8"),
                    b"expected GET /pull/<64-char sha256 hex>\n",
                )
                .await?;
                return Ok(());
            };

            let maybe_blob = blobs.lock().await.get(&hash).cloned();
            match maybe_blob {
                Some(blob) => {
                    info!("served {} bytes from {}", blob.len(), hash);
                    write_response(
                        &mut stream,
                        "200 OK",
                        Some("application/octet-stream"),
                        &blob,
                    )
                    .await?;
                }
                None => {
                    warn!("blob not found for {}", hash);
                    write_response(
                        &mut stream,
                        "404 Not Found",
                        Some("text/plain; charset=utf-8"),
                        b"not found\n",
                    )
                    .await?;
                }
            }
        }
        _ => {
            write_response(
                &mut stream,
                "405 Method Not Allowed",
                Some("text/plain; charset=utf-8"),
                b"supported routes: POST /push/<sha256>, GET /pull/<sha256>\n",
            )
            .await?;
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .compact()
        .with_target(false)
        .without_time()
        .init();

    let Some(cli) = parse_cli_args()? else {
        println!("{}", usage());
        return Ok(());
    };

    let listener = TcpListener::bind(&cli.bind)
        .await
        .with_context(|| format!("Failed to bind mock council server to '{}'", cli.bind))?;
    let blobs: BlobStore = Arc::new(Mutex::new(HashMap::new()));

    info!("Mock council server listening on http://{}", cli.bind);
    info!("Ready for POST /push/<sha256> and GET /pull/<sha256>");

    loop {
        let (stream, peer_addr) = listener
            .accept()
            .await
            .context("Failed to accept council client connection")?;
        let blobs = blobs.clone();
        tokio::spawn(async move {
            if let Err(error) = handle_request(stream, blobs).await {
                error!("request from {} failed: {:#}", peer_addr, error);
            }
        });
    }
}
