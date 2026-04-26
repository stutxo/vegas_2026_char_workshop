use std::{env, fs};

use anyhow::{Context, Result, bail};
use bitcoin::Network;
use chomp::{
    BitcoinDa, BitcoinDaConfig, ChompPayload, DataAvailabilityExt, NamespaceId, OversizePolicy,
};

#[path = "../utils.rs"]
mod utils;

#[derive(Debug)]
struct CliArgs {
    chunked: bool,
    payload: Vec<u8>,
}

fn usage() -> &'static str {
    "Usage: cargo run --example bitcoin -- [--chunked] (--payload <string> | --payload-file <path>)\n\
     \n\
     This example always uses Bitcoin regtest and a wallet-scoped RPC endpoint."
}

fn parse_cli_args() -> Result<Option<CliArgs>> {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut chunked = false;
    let mut payload: Option<Vec<u8>> = None;
    let mut i = 0usize;

    while i < args.len() {
        let arg = &args[i];

        if arg == "--chunked" {
            chunked = true;
            i += 1;
            continue;
        }
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
                    .with_context(|| format!("Failed to read Bitcoin payload file '{}'", value))?,
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
                    .with_context(|| format!("Failed to read Bitcoin payload file '{}'", value))?,
            );
            i += 1;
            continue;
        }
        bail!("Unknown flag '{}'\n\n{}", arg, usage());
    }

    let payload = payload.with_context(|| format!("Missing Bitcoin payload.\n\n{}", usage()))?;
    Ok(Some(CliArgs { chunked, payload }))
}

#[tokio::main]
async fn main() -> Result<()> {
    let Some(cli) = parse_cli_args()? else {
        println!("{}", usage());
        return Ok(());
    };

    let (client, rpc_url) = utils::build_bitcoin_rpc_client_from_env()?;
    utils::ensure_wallet_rpc_endpoint(&client, "Bitcoin", &rpc_url, "BITCOIND_RPC_WALLET").await?;

    let da = BitcoinDa::new(BitcoinDaConfig {
        network: Network::Regtest,
        client,
        namespace_id: NamespaceId::new(b"btc")?,
        fee_policy: BitcoinDaConfig::default_fee_policy(),
        oversize_policy: if cli.chunked {
            OversizePolicy::chunked_default()
        } else {
            OversizePolicy::Reject
        },  
    })?;

    println!("bitcoin rpc: {rpc_url}");
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
