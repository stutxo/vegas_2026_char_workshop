use bitcoin::hashes::{Hash as _, sha256};
use char_sdk::{
    BitcoindAsyncTransport, CharBallotHandlers, CharReconcileCursor, CharRpcTransport,
    ObservedRoll, async_trait, bytes_to_hex,
};
use serde_json::{Value, json};

use std::error::Error;

type AnyError = Box<dyn Error + Send + Sync>;

const NODE_IP: &str = "";
const RPC_USERNAME: &str = "char";
const RPC_PASSWORD: &str = "";
// Change this to choose your own workshop domain.
const DOMAIN: &str = "";

// const CHOMP_RPC_WALLET: &str = "char";
// use chomp::{
//     BitcoinDa, BitcoinDaConfig, DaError, DataAvailability, MemberId, NamespaceId, OversizePolicy,
//     PolicyKey,
// };
// use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), AnyError> {
    let rpc_url = format!("http://{NODE_IP}:18443");
    let zmq_addr = format!("tcp://{NODE_IP}:28332");
    let domain_id = get_domain_id(DOMAIN);
    let domain_hex = domain_id.to_string();
    println!("Using domain `{DOMAIN}` with hex ID {domain_hex}");
    let transport = BitcoindAsyncTransport::from_user_pass(&rpc_url, RPC_USERNAME, RPC_PASSWORD)?;
    let _ = transport
        .domain_registry_schedule(&domain_hex, DOMAIN)
        .await?;
    let start_ballot = transport.get_domain_info(&domain_hex).await?.next_ballot;

    // let chomp = Arc::new(build_chomp_da(domain_id.as_byte_array())?);

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

// fn build_chomp_da(domain_id: &[u8]) -> Result<BitcoinDa, AnyError> {
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
//         namespace_id: NamespaceId::new(domain_id)?,
//         fee_policy: BitcoinDaConfig::default_fee_policy(),
//         oversize_policy: OversizePolicy::Reject,
//     })?)
// }

// #[derive(Debug)]
// struct ChompRollBytes {
//     bytes: Vec<u8>,
// }

// fn chomp_roll_bytes(roll: &ObservedRoll) -> Result<ChompRollBytes, AnyError> {
//     let bytes = roll
//         .serialized_roll
//         .clone()
//         .filter(|bytes| !bytes.is_empty())
//         .ok_or_else(|| {
//             std::io::Error::new(
//                 std::io::ErrorKind::InvalidData,
//                 format!(
//                     "observed ballot {} missing serialized decision roll",
//                     roll.ballot
//                 ),
//             )
//         })?;

//     Ok(ChompRollBytes { bytes })
// }

// async fn chomp_write(chomp: Arc<BitcoinDa>, ballot: u64, roll_bytes: ChompRollBytes) {
//     match chomp.write_blob(&roll_bytes.bytes).await {
//         Ok(receipt) => match bitcoin_txid_hex(receipt.key()) {
//             Ok(txid_hex) => println!(
//                 "chomp posted ballot {ballot}: {} decision roll bytes written, txid {txid_hex}",
//                 receipt.size(),
//             ),
//             Err(err) => println!(
//                 "chomp posted ballot {ballot}: {} decision roll bytes written, key {:?} (failed to decode txid: {err})",
//                 receipt.size(),
//                 receipt.key()
//             ),
//         },
//         Err(err) => eprintln!("chomp post failed for ballot {ballot}: {err}"),
//     }
// }

// fn bitcoin_txid_hex(key: &PolicyKey) -> Result<String, DaError> {
//     let locator = key.as_leaf_for_member(&MemberId::Bitcoin)?;
//     Ok(BitcoinDa::txid_from_locator(locator)?.to_string())
// }

fn observed_roll_json(roll: &ObservedRoll) -> Value {
    let serialized_roll = roll.serialized_roll.as_ref();
    let payload_utf8 = std::str::from_utf8(&roll.payload).ok();

    json!({
        "ballot": roll.ballot,
        "payload": {
            "len": roll.payload.len(),
            "hex": bytes_to_hex(&roll.payload),
            "utf8": payload_utf8,
        },
        "decision_roll": {
            "serialized_len": serialized_roll.map(Vec::len).unwrap_or_default(),
            "serialized_hex": serialized_roll.map(|bytes| bytes_to_hex(bytes)),
            "roll_hash": roll.roll_hash.map(|hash| hash.to_string()),
            "data_hash": roll.data_hash.map(|hash| hash.to_string()),
            "tag": roll.tag,
        }
    })
}

#[async_trait]
impl CharBallotHandlers for App {
    async fn produce_payload(&mut self, ballot: u64) -> Vec<u8> {
        format!("hello {ballot}").into_bytes()
    }

    async fn on_roll_observed(&mut self, roll: ObservedRoll) -> Result<(), AnyError> {
        // let roll_bytes = chomp_roll_bytes(&roll)?;
        let pretty_roll = serde_json::to_string_pretty(&observed_roll_json(&roll))?;
        println!("char observed decision roll:\n{pretty_roll}");

        // let chomp = Arc::clone(&self.chomp);
        // tokio::spawn(chomp_write(chomp, roll.ballot, roll_bytes));

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

fn get_domain_id(word: &str) -> sha256::Hash {
    sha256::Hash::hash(word.as_bytes())
}
