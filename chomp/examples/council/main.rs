use std::{env, fs};

use anyhow::{Context, Result, bail};
use chomp::{ChompPayload, CouncilDa, MultiDa, policy_spec};

#[path = "../utils.rs"]
mod utils;

#[derive(Debug)]
struct CliArgs {
    default_url: String,
    payload: Vec<u8>,
}

fn usage() -> &'static str {
    "Usage: cargo run --example council -- (--payload <string> | --payload-file <path>)\n\
     \n\
     This example uses COUNCIL_URL as the default for four logical council members,\n\
     or per-member overrides via COUNCIL_SEATTLE_URL / COUNCIL_LONDON_URL /\n\
     COUNCIL_TOKYO_URL / COUNCIL_SYDNEY_URL."
}

fn parse_cli_args() -> Result<Option<CliArgs>> {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut payload: Option<Vec<u8>> = None;
    let mut i = 0usize;

    while i < args.len() {
        let arg = &args[i];

        if arg == "--payload" {
            let value = args
                .get(i + 1)
                .context("Missing value for --payload flag")?;
            payload = Some(value.as_bytes().to_vec());
            i += 2;
            continue;
        }
        if arg == "--payload-file" {
            let value = args
                .get(i + 1)
                .context("Missing value for --payload-file flag")?;
            payload = Some(
                fs::read(value)
                    .with_context(|| format!("Failed to read council payload file '{}'", value))?,
            );
            i += 2;
            continue;
        }
        if arg == "--help" || arg == "-h" {
            return Ok(None);
        }
        if let Some(value) = arg.strip_prefix("--payload=") {
            payload = Some(value.as_bytes().to_vec());
            i += 1;
            continue;
        }
        if let Some(value) = arg.strip_prefix("--payload-file=") {
            payload = Some(
                fs::read(value)
                    .with_context(|| format!("Failed to read council payload file '{}'", value))?,
            );
            i += 1;
            continue;
        }
        bail!("Unknown flag '{}'\n\n{}", arg, usage());
    }

    let payload = payload.with_context(|| format!("Missing council payload.\n\n{}", usage()))?;
    let default_url =
        env::var("COUNCIL_URL").unwrap_or_else(|_| "http://127.0.0.1:8080".to_string());

    Ok(Some(CliArgs {
        default_url,
        payload,
    }))
}

fn council_url(env_key: &str, default_url: &str) -> String {
    env::var(env_key).unwrap_or_else(|_| default_url.to_string())
}

#[tokio::main]
async fn main() -> Result<()> {
    let Some(cli) = parse_cli_args()? else {
        println!("{}", usage());
        return Ok(());
    };

    let seattle_url = council_url("COUNCIL_SEATTLE_URL", &cli.default_url);
    let london_url = council_url("COUNCIL_LONDON_URL", &cli.default_url);
    let tokyo_url = council_url("COUNCIL_TOKYO_URL", &cli.default_url);
    let sydney_url = council_url("COUNCIL_SYDNEY_URL", &cli.default_url);

    // Require one council from the Seattle/London pair and one from the Tokyo/Sydney pair.
    let da = MultiDa::from_spec(policy_spec!(all(
        any(
            CouncilDa::new("seattle", &seattle_url)?,
            CouncilDa::new("london", &london_url)?
        ),
        any(
            CouncilDa::new("tokyo", &tokyo_url)?,
            CouncilDa::new("sydney", &sydney_url)?
        ),
    )))?;

    println!("seattle council url: {seattle_url}");
    println!("london council url: {london_url}");
    println!("tokyo council url: {tokyo_url}");
    println!("sydney council url: {sydney_url}");
    println!(
        "policy: AND([OR([council:seattle, council:london]), OR([council:tokyo, council:sydney])])"
    );
    println!("writing {} bytes", cli.payload.len());

    let payload = ChompPayload::new(cli.payload);
    let receipt = da.write(&payload).await?;
    println!("key: {:?}", receipt.key());

    let decoded: ChompPayload = da.read(receipt.key()).await?;
    println!("read back {} bytes", decoded.len());
    println!("{}", utils::format_blob(decoded.as_slice()));

    let verify = da.verify(receipt.key()).await?;
    println!("verify: {:?}", verify);

    Ok(())
}
