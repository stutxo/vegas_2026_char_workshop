use std::{env, fs};

use anyhow::{Context, Result, bail};
use bitcoin::Network;
use chomp::{
    BitcoinDa, BitcoinDaConfig, ChompPayload, CouncilDa, LiquidDa, LiquidDaConfig, MultiDa,
    NamespaceId, OversizePolicy, policy_spec,
};

#[path = "../utils.rs"]
mod utils;

#[derive(Debug)]
struct CliArgs {
    council_url: String,
    payload: Vec<u8>,
}

fn usage() -> &'static str {
    "Usage: cargo run --example multi -- (--payload <string> | --payload-file <path>)\n\
     \n\
     This example always uses Bitcoin regtest, Liquid regtest, and a council server at COUNCIL_URL or http://127.0.0.1:8080."
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
                    .with_context(|| format!("Failed to read MultiDa payload file '{}'", value))?,
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
                    .with_context(|| format!("Failed to read MultiDa payload file '{}'", value))?,
            );
            i += 1;
            continue;
        }
        bail!("Unknown flag '{}'\n\n{}", arg, usage());
    }

    let payload = payload.with_context(|| format!("Missing MultiDa payload.\n\n{}", usage()))?;
    let council_url =
        env::var("COUNCIL_URL").unwrap_or_else(|_| "http://127.0.0.1:8080".to_string());

    Ok(Some(CliArgs {
        council_url,
        payload,
    }))
}

#[tokio::main]
async fn main() -> Result<()> {
    let Some(cli) = parse_cli_args()? else {
        println!("{}", usage());
        return Ok(());
    };

    let (bitcoin_client, bitcoin_rpc_url) = utils::build_bitcoin_rpc_client_from_env()?;
    let (liquid_client, liquid_rpc_url) = utils::build_liquid_rpc_client_from_env()?;
    utils::ensure_wallet_rpc_endpoint(
        &bitcoin_client,
        "Bitcoin",
        &bitcoin_rpc_url,
        "BITCOIND_RPC_WALLET",
    )
    .await?;
    utils::ensure_wallet_rpc_endpoint(
        &liquid_client,
        "Liquid",
        &liquid_rpc_url,
        "LIQUID_RPC_WALLET or ELEMENTSD_RPC_WALLET",
    )
    .await?;

    let bitcoin_backend = BitcoinDa::new(BitcoinDaConfig {
        network: Network::Regtest,
        client: bitcoin_client,
        namespace_id: NamespaceId::new(b"btc")?,
        fee_policy: BitcoinDaConfig::default_fee_policy(),
        oversize_policy: OversizePolicy::Reject,
    })?;
    let liquid_backend = LiquidDa::new(LiquidDaConfig {
        network: Network::Regtest,
        client: liquid_client,
        namespace_id: NamespaceId::new(b"lqd")?,
        fee_policy: LiquidDaConfig::default_fee_policy(),
        oversize_policy: OversizePolicy::Reject,
    })?;
    let council_backend = CouncilDa::new("default", &cli.council_url)?;

    // Require a council write, and also keep whichever chain backend succeeds first.
    let da = MultiDa::from_spec(policy_spec!(all(
        any(bitcoin_backend, liquid_backend),
        council_backend,
    )))?;

    println!("bitcoin rpc: {bitcoin_rpc_url}");
    println!("liquid rpc: {liquid_rpc_url}");
    println!("council url: {}", cli.council_url);
    println!("policy: AND([OR([bitcoin, liquid]), council:default])");
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
