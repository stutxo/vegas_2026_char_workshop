// Copyright (c) 2026 Judica, Inc.
// Distributed under the MIT software license, see the accompanying
// file COPYING or https://opensource.org/license/mit/.

//! Wire-shaped RPC request/response types. Serde (de)serializable to match JSON-RPC.

use bitcoin::hashes::sha256::Hash as Sha256Hash;
use bitcoin::{BlockHash, Txid};
use serde::de::Visitor;
use serde::{Deserialize, Deserializer, Serialize};
use std::fmt;

/// Response of `getdomaininfo` (Char RPC): next ballot, leader bond, latest decided roll summary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DomainInfo {
    pub next_ballot: u64,
    pub next_leader_bond: Txid,
    pub is_next_leader_mine: bool,
    /// JSON `null` when no ballot has been decided yet (`next_ballot == 0` on the node).
    #[serde(default)]
    pub latest_decided_ballot: Option<u64>,
    /// Hex root hash of the latest decided decision roll; empty when none.
    #[serde(default)]
    pub latest_decision_roll_hash: String,
    /// Hex data hash; empty when none.
    #[serde(default)]
    pub latest_decision_data_hash: String,
    /// Hex block hash anchor; empty when none.
    #[serde(default)]
    pub latest_decision_zeitgeist: String,
}

/// One entry from `getreferendumdecisionroll` (one per ballot in range).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecisionRollEntry {
    pub domain_hash: Sha256Hash,
    pub ballot_number: u64,
    pub found: bool,
    #[serde(default)]
    pub decision_roll: Option<DecisionRollWire>,
}

/// Verbosity for `getreferendumdecisionroll`. 0 = minimal; 1 = with attestation/data; 2 = with proofs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DecisionRollVerbosity {
    /// Minimal (0): ballot_number, found, basic decision_roll fields.
    Minimal = 0,
    /// Standard (1): plus attestation_hash and data.
    #[default]
    Standard = 1,
    /// Full (2): plus proofs.
    Full = 2,
}

impl From<DecisionRollVerbosity> for u8 {
    fn from(v: DecisionRollVerbosity) -> u8 {
        v as u8
    }
}

/// When `found` is true, the node sends roll_hash, data_hash, serialized; optional fields at higher verbosity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecisionRollWire {
    pub roll_hash: Option<Sha256Hash>,
    pub data_hash: Option<Sha256Hash>,
    pub serialized: Option<String>,
    pub data: Option<String>,
    pub attestation_hash: Option<Sha256Hash>,
    pub proofs: Option<Vec<Sha256Hash>>,
}

/// One selection: bond (txid hex) for a given slot. Part of `LeaderSlotEntry::selections`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlotSelection {
    /// Node JSON field is `ballot` (`get_leader_for_ballot_current_block`).
    #[serde(rename = "ballot")]
    pub slot: u64,
    pub bond: Txid,
}

/// One entry in the response of `get_leader_for_slot_current_block` (one per key range).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeaderSlotEntry {
    pub key: String,
    pub blockhash: BlockHash,
    pub selections: Vec<SlotSelection>,
}

/// Request item for `get_leader_for_slot_current_block`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyRange {
    pub key: String,
    pub start_slot: u64,
    pub end_slot: u64,
}

/// `getallcharbonds` sends `amount` via `ValueFromAmount` (JSON number). Also accept a string (tests/docs).
fn deserialize_bond_amount<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    struct AmountStr;

    impl Visitor<'_> for AmountStr {
        type Value = String;

        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("a JSON string or number for bond amount")
        }

        fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<String, E> {
            Ok(v.to_owned())
        }

        fn visit_string<E: serde::de::Error>(self, v: String) -> Result<String, E> {
            Ok(v)
        }

        fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<String, E> {
            Ok(format!("{v}"))
        }

        fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<String, E> {
            Ok(v.to_string())
        }

        fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<String, E> {
            Ok(v.to_string())
        }
    }

    deserializer.deserialize_any(AmountStr)
}

/// One bond in the response of `getallcharbonds`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BondInfo {
    pub txid: Txid,
    /// Hex of bond key (issuer); may be empty if anchor view unavailable.
    pub issuer: String,
    /// JSON number from the node (`ValueFromAmount`) or string.
    #[serde(deserialize_with = "deserialize_bond_amount")]
    pub amount: String,
    /// True if quickbreak output is spent (bond closed).
    pub closed: bool,
    /// Present when attestation chain stats are available.
    pub attestations: Option<BondAttestationsInfo>,
}

/// Attestation summary on a bond from `getallcharbonds` (when stats are available).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BondAttestationsInfo {
    pub ballot_number: u64,
    pub chain_id: Sha256Hash,
    pub genesis_char_hash: Sha256Hash,
}

/// One domain entry in `getattestationforbondatballot` `entries`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttestationEntryWire {
    pub key: String,
    pub value: String,
}

/// Response of `getattestationforbondatballot`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttestationForBondBallot {
    pub ballot_number: u64,
    pub chain_id: Sha256Hash,
    pub char_hash: String,
    pub block_hash: String,
    pub entries: Vec<AttestationEntryWire>,
}

/// Response of `domain_registry schedule`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DomainRegistryScheduleResult {
    pub success: bool,
}

/// Mode for `addreferendumvote`. Serializes to node strings "is_leader" / "plzfind".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddReferendumVoteMode {
    #[default]
    IsLeader,
    /// Initialize domain at ballot 0 (`addreferendumvote` mode `init`).
    Init,
    #[serde(rename = "plzfind")]
    PlzFind,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn hex64(ch: char) -> String {
        std::iter::repeat_n(ch, 64).collect()
    }

    #[test]
    fn domain_info_serde_roundtrip() {
        let json = format!(
            r#"{{"next_ballot":1,"next_leader_bond":"{}","is_next_leader_mine":true,"latest_decided_ballot":0,"latest_decision_roll_hash":"{}","latest_decision_data_hash":"{}","latest_decision_zeitgeist":"{}"}}"#,
            hex64('a'),
            hex64('b'),
            hex64('c'),
            hex64('d')
        );
        let v: DomainInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(v.next_ballot, 1);
        assert_eq!(v.next_leader_bond, Txid::from_str(&hex64('a')).unwrap());
        assert!(v.is_next_leader_mine);
        assert_eq!(v.latest_decided_ballot, Some(0));
        assert_eq!(v.latest_decision_roll_hash, hex64('b'));
        let back = serde_json::to_string(&v).unwrap();
        let v2: DomainInfo = serde_json::from_str(&back).unwrap();
        assert_eq!(v, v2);
    }

    #[test]
    fn domain_info_null_latest_decided() {
        let json = format!(
            r#"{{"next_ballot":0,"next_leader_bond":"{}","is_next_leader_mine":false,"latest_decided_ballot":null,"latest_decision_roll_hash":"","latest_decision_data_hash":"","latest_decision_zeitgeist":""}}"#,
            hex64('e')
        );
        let v: DomainInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(v.next_ballot, 0);
        assert!(v.latest_decided_ballot.is_none());
        assert!(!v.is_next_leader_mine);
    }

    #[test]
    fn decision_roll_entry_found_false() {
        let json = format!(
            r#"{{"domain_hash":"{}","ballot_number":1,"found":false}}"#,
            hex64('d')
        );
        let v: DecisionRollEntry = serde_json::from_str(&json).unwrap();
        assert!(!v.found);
        assert!(v.decision_roll.is_none());
    }

    #[test]
    fn decision_roll_entry_found_true() {
        let json = format!(
            r#"{{"domain_hash":"{}","ballot_number":2,"found":true,"decision_roll":{{"roll_hash":"{}","data_hash":"{}","serialized":"0x00"}}}}"#,
            hex64('d'),
            hex64('e'),
            hex64('f')
        );
        let v: DecisionRollEntry = serde_json::from_str(&json).unwrap();
        assert!(v.found);
        let dr = v.decision_roll.as_ref().unwrap();
        assert_eq!(dr.roll_hash, Some(Sha256Hash::from_str(&hex64('e')).unwrap()));
        assert_eq!(dr.serialized.as_deref(), Some("0x00"));
    }

    #[test]
    fn decision_roll_wire_optional_fields() {
        let json = format!(r#"{{"proofs":["{}","{}"]}}"#, hex64('1'), hex64('2'));
        let v: DecisionRollWire = serde_json::from_str(&json).unwrap();
        assert_eq!(
            v.proofs,
            Some(vec![
                Sha256Hash::from_str(&hex64('1')).unwrap(),
                Sha256Hash::from_str(&hex64('2')).unwrap()
            ])
        );
    }

    #[test]
    fn slot_selection_serde_roundtrip() {
        let json = format!(r#"{{"ballot":10,"bond":"{}"}}"#, hex64('a'));
        let v: SlotSelection = serde_json::from_str(&json).unwrap();
        assert_eq!(v.slot, 10);
        assert_eq!(v.bond, Txid::from_str(&hex64('a')).unwrap());
    }

    #[test]
    fn leader_slot_entry_serde_roundtrip() {
        let json = format!(
            r#"{{"key":"domainhex","blockhash":"{}","selections":[{{"ballot":0,"bond":"{}"}},{{"ballot":1,"bond":"{}"}}]}}"#,
            hex64('a'),
            hex64('b'),
            hex64('c')
        );
        let v: LeaderSlotEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(v.selections.len(), 2);
        assert_eq!(v.selections[0].slot, 0);
        assert_eq!(v.selections[1].bond, Txid::from_str(&hex64('c')).unwrap());
    }

    #[test]
    fn key_range_serde_roundtrip() {
        let k = KeyRange { key: "k".to_string(), start_slot: 0, end_slot: 99 };
        let json = serde_json::to_string(&k).unwrap();
        let k2: KeyRange = serde_json::from_str(&json).unwrap();
        assert_eq!(k, k2);
    }

    #[test]
    fn bond_info_without_attestations() {
        let json = format!(
            r#"{{"txid":"{}","issuer":"","amount":"0.10000000","closed":false}}"#,
            hex64('a')
        );
        let v: BondInfo = serde_json::from_str(&json).unwrap();
        assert!(v.attestations.is_none());
    }

    #[test]
    fn bond_info_amount_json_number_like_bitcoind() {
        let json = format!(
            r#"{{"txid":"{}","issuer":"","amount":0.1,"closed":false}}"#,
            hex64('a')
        );
        let v: BondInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(v.amount, "0.1");
    }

    #[test]
    fn bond_info_with_attestations() {
        let json = format!(
            r#"{{"txid":"{}","issuer":"iss","amount":"1.0","closed":false,"attestations":{{"ballot_number":3,"chain_id":"{}","genesis_char_hash":"{}"}}}}"#,
            hex64('a'),
            hex64('c'),
            hex64('b')
        );
        let v: BondInfo = serde_json::from_str(&json).unwrap();
        let att = v.attestations.as_ref().unwrap();
        assert_eq!(att.ballot_number, 3);
        assert_eq!(att.chain_id, Sha256Hash::from_str(&hex64('c')).unwrap());
        assert_eq!(att.genesis_char_hash, Sha256Hash::from_str(&hex64('b')).unwrap());
    }

    #[test]
    fn add_referendum_vote_mode_serialize() {
        let s = serde_json::to_string(&AddReferendumVoteMode::IsLeader).unwrap();
        assert_eq!(s, r#""is_leader""#);
        let s_init = serde_json::to_string(&AddReferendumVoteMode::Init).unwrap();
        assert_eq!(s_init, r#""init""#);
        let s2 = serde_json::to_string(&AddReferendumVoteMode::PlzFind).unwrap();
        assert_eq!(s2, r#""plzfind""#);
    }

    #[test]
    fn add_referendum_vote_mode_deserialize() {
        let m: AddReferendumVoteMode = serde_json::from_str(r#""is_leader""#).unwrap();
        assert_eq!(m, AddReferendumVoteMode::IsLeader);
        let m_init: AddReferendumVoteMode = serde_json::from_str(r#""init""#).unwrap();
        assert_eq!(m_init, AddReferendumVoteMode::Init);
        let m2: AddReferendumVoteMode = serde_json::from_str(r#""plzfind""#).unwrap();
        assert_eq!(m2, AddReferendumVoteMode::PlzFind);
    }

    #[test]
    fn add_referendum_vote_mode_default() {
        assert_eq!(AddReferendumVoteMode::default(), AddReferendumVoteMode::IsLeader);
    }
}
