use bitcoin::hashes::{Hash as _, sha256};
use char_sdk::{
    BitcoindAsyncTransport, CharBallotHandlers, CharReconcileCursor, CharRpcTransport,
    ObservedRoll, async_trait,
};

use std::error::Error;

type AnyError = Box<dyn Error + Send + Sync>;

const NODE_IP: &str = "";
const RPC_USERNAME: &str = "";
const RPC_PASSWORD: &str = "";
// Change this to choose your own workshop domain.
const DOMAIN: &str = "";

// const CHOMP_RPC_WALLET: &str = "char";
// use chomp::{BitcoinDa, BitcoinDaConfig, DataAvailability, NamespaceId, OversizePolicy};
// use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), AnyError> {
    let rpc_url = format!("http://{NODE_IP}:18443");
    let zmq_addr = format!("tcp://{NODE_IP}:28332");
    let domain_hex = get_domain_hex(DOMAIN);
    let transport = BitcoindAsyncTransport::from_user_pass(&rpc_url, RPC_USERNAME, RPC_PASSWORD)?;
    let _ = transport
        .domain_registry_schedule(&domain_hex, DOMAIN)
        .await?;
    let start_ballot = transport.get_domain_info(&domain_hex).await?.next_ballot;

    // let chomp = Arc::new(build_chomp_da(&domain_hex)?);

    let mut app = App {
        next_ballot: start_ballot,
        // chomp,
    };

    char_sdk::run_zmq_with_address(&transport, &domain_hex, &mut app, zmq_addr).await?;

    Ok(())
}

struct App {
    next_ballot: u64,
    // chomp: Arc<BitcoinDa>,
}

// fn build_chomp_da(domain_hex: &str) -> Result<BitcoinDa, AnyError> {
//     let wallet_rpc_url = format!("http://{NODE_IP}:18443/wallet/{CHOMP_RPC_WALLET}");
//     let client = bitcoind_async_client::Client::new(
//         wallet_rpc_url,
//         bitcoind_async_client::Auth::UserPass(RPC_USERNAME.to_string(), RPC_PASSWORD.to_string()),
//         None,
//         None,
//         None,
//     )?;

//     Ok(BitcoinDa::new(BitcoinDaConfig {
//         network: bitcoin::Network::Regtest,
//         client,
//         namespace_id: NamespaceId::new(domain_hex.as_bytes())?,
//         fee_policy: BitcoinDaConfig::default_fee_policy(),
//         oversize_policy: OversizePolicy::Reject,
//     })?)
// }

// async fn chomp_write(chomp: Arc<BitcoinDa>, ballot: u64, bytes: Vec<u8>) {
//     match chomp.write_blob(&bytes).await {
//         Ok(receipt) => println!(
//             "chomp posted ballot {ballot}: {} bytes written, key {:?}",
//             receipt.size(),
//             receipt.key()
//         ),
//         Err(err) => eprintln!("chomp post failed for ballot {ballot}: {err}"),
//     }
// }

#[async_trait]
impl CharBallotHandlers for App {
    async fn produce_payload(&mut self, ballot: u64) -> Vec<u8> {
        format!("hello {ballot}").into_bytes()
    }

    async fn on_roll_observed(&mut self, roll: ObservedRoll) -> Result<(), AnyError> {
        let payload_utf8 = String::from_utf8_lossy(&roll.payload);
        println!("char observed ballot {}: `{payload_utf8}`", roll.ballot);

        // let chomp = Arc::clone(&self.chomp);
        // tokio::spawn(chomp_write(chomp, roll.ballot, roll.payload));

        Ok(())
    }
}

#[async_trait]
impl CharReconcileCursor for App {
    async fn next_ballot(&mut self) -> Result<u64, AnyError> {
        Ok(self.next_ballot)
    }

    async fn advance_cursor(&mut self, next_ballot: u64) -> Result<(), AnyError> {
        self.next_ballot = next_ballot;
        Ok(())
    }
}

fn get_domain_hex(word: &str) -> String {
    sha256::Hash::hash(word.as_bytes()).to_string()
}
