use super::{
    BorshBundle, BorshBundleVersion, BorshPayload, ChompPayload, CodecError, decode_borsh,
    decode_borsh_bundle, encode_borsh,
};
use borsh::{BorshDeserialize, BorshSerialize};

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct ExamplePayload {
    ballot: u64,
    payload: Vec<u8>,
}

impl BorshPayload for ExamplePayload {}

#[derive(BorshSerialize)]
struct RawBundle<T> {
    version: BorshBundleVersion,
    items: Vec<T>,
}

fn sample_payload(ballot: u64, payload: &[u8]) -> ExamplePayload {
    ExamplePayload {
        ballot,
        payload: payload.to_vec(),
    }
}

#[test]
fn direct_borsh_payload_round_trips() {
    let original = sample_payload(7, b"decision-roll");

    let encoded = encode_borsh(&original).expect("payload should encode");
    let decoded =
        decode_borsh::<ExamplePayload>(encoded.as_slice()).expect("payload should decode");

    assert_eq!(decoded, original);
}

#[test]
fn borsh_bundle_round_trips_with_one_item() {
    let bundle = BorshBundle::new(vec![sample_payload(1, b"one")]).expect("bundle should build");

    let decoded = BorshBundle::<ExamplePayload>::decode(
        bundle.encode().expect("bundle should encode").as_slice(),
    )
    .expect("bundle should decode");

    assert_eq!(decoded, bundle);
}

#[test]
fn borsh_bundle_round_trips_with_multiple_items() {
    let bundle = BorshBundle::new(vec![
        sample_payload(1, b"one"),
        sample_payload(2, b"two"),
        sample_payload(3, b"three"),
    ])
    .expect("bundle should build");

    let decoded = decode_borsh_bundle::<ExamplePayload>(
        bundle.encode().expect("bundle should encode").as_slice(),
    )
    .expect("bundle should decode");

    assert_eq!(decoded, bundle);
    assert_eq!(decoded.items()[0].ballot, 1);
    assert_eq!(decoded.items()[1].ballot, 2);
    assert_eq!(decoded.items()[2].ballot, 3);
}

#[test]
fn decode_borsh_rejects_truncated_payloads() {
    let payload = sample_payload(11, b"abc");
    let mut encoded = encode_borsh(&payload).expect("payload should encode");
    encoded.pop();

    let err = decode_borsh::<ExamplePayload>(&encoded).expect_err("truncated payload must fail");
    assert!(matches!(err, CodecError::Deserialize(_)));
}

#[test]
fn decode_borsh_rejects_trailing_bytes() {
    let mut encoded = encode_borsh(&sample_payload(12, b"abc")).expect("payload should encode");
    encoded.extend_from_slice(&[0xff, 0x00]);

    let err = decode_borsh::<ExamplePayload>(&encoded).expect_err("trailing bytes must fail");
    assert!(matches!(err, CodecError::Deserialize(_)));
}

#[test]
fn empty_bundle_is_rejected() {
    let err =
        BorshBundle::<ExamplePayload>::new(Vec::new()).expect_err("empty bundle must be rejected");
    assert_eq!(err, CodecError::EmptyBundle);
}

#[test]
fn bytes_payload_round_trips() {
    let original = ChompPayload::from(b"decision-roll".as_slice());

    let encoded = encode_borsh(&original).expect("payload should encode");
    let decoded = decode_borsh::<ChompPayload>(encoded).expect("payload should decode");

    assert_eq!(decoded, original);
}

#[test]
fn decode_borsh_bundle_rejects_empty_bundle_bytes() {
    let empty_bundle_bytes = borsh::to_vec(&RawBundle::<ExamplePayload> {
        version: BorshBundleVersion::V1,
        items: Vec::new(),
    })
    .expect("test bundle should serialize");

    let err = decode_borsh_bundle::<ExamplePayload>(&empty_bundle_bytes)
        .expect_err("empty bundle bytes must be rejected");
    assert_eq!(err, CodecError::EmptyBundle);
}
