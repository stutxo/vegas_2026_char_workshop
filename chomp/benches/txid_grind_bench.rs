use std::time::{Duration, Instant};

use bitcoin::blockdata::locktime::absolute::LockTime;
use bitcoin::blockdata::script::ScriptBuf;
use bitcoin::blockdata::transaction::{OutPoint, TxIn, TxOut, Version};
use bitcoin::{Sequence, Transaction, Witness, hashes::Hash};
use criterion::{Criterion, Throughput, black_box, criterion_group, criterion_main};
use elements::{
    LockTime as LiquidLockTime, OutPoint as LiquidOutPoint, Sequence as LiquidSequence,
    Transaction as LiquidTransaction, TxIn as LiquidTxIn, TxOut as LiquidTxOut,
    TxOutWitness as LiquidTxOutWitness,
    confidential::{Asset, Nonce, Value},
};

const BENCH_DOMAIN_ID: [u8; 4] = [0x93, 0x4a, 0xfe, 0x00];
const BENCH_ATTEMPTS: u32 = 1 << 20;
const ESTIMATE_SAMPLES: usize = 3;
const FULL_GRIND_SAMPLES: usize = 5;

#[path = "support/txid_grind.rs"]
mod id;

fn benchmark_tx() -> Transaction {
    Transaction {
        version: Version(2),
        lock_time: LockTime::ZERO,
        input: vec![TxIn {
            previous_output: OutPoint::null(),
            script_sig: ScriptBuf::new(),
            sequence: Sequence::MAX,
            witness: Witness::default(),
        }],
        output: vec![TxOut {
            value: bitcoin::Amount::from_sat(50_000),
            script_pubkey: ScriptBuf::new(),
        }],
    }
}

fn benchmark_tx_variant(sample: u32) -> Transaction {
    Transaction {
        version: Version(2),
        lock_time: LockTime::ZERO,
        input: vec![TxIn {
            previous_output: OutPoint::new(bitcoin::Txid::from_byte_array([sample as u8; 32]), 0),
            script_sig: ScriptBuf::new(),
            sequence: Sequence::MAX,
            witness: Witness::default(),
        }],
        output: vec![TxOut {
            value: bitcoin::Amount::from_sat(50_000 + sample as u64),
            script_pubkey: ScriptBuf::new(),
        }],
    }
}

fn benchmark_liquid_tx() -> LiquidTransaction {
    LiquidTransaction {
        version: 2,
        lock_time: LiquidLockTime::ZERO,
        input: vec![LiquidTxIn {
            previous_output: LiquidOutPoint::default(),
            sequence: LiquidSequence::MAX,
            ..Default::default()
        }],
        output: vec![LiquidTxOut {
            asset: Asset::Explicit(elements::AssetId::from_slice(&[7u8; 32]).unwrap()),
            value: Value::Explicit(50_000),
            nonce: Nonce::Null,
            script_pubkey: elements::Script::new(),
            witness: LiquidTxOutWitness::default(),
        }],
    }
}

fn benchmark_liquid_tx_variant(sample: u32) -> LiquidTransaction {
    LiquidTransaction {
        version: 2,
        lock_time: LiquidLockTime::ZERO,
        input: vec![LiquidTxIn {
            previous_output: LiquidOutPoint::new(
                elements::Txid::from_byte_array([sample as u8; 32]),
                0,
            ),
            sequence: LiquidSequence::MAX,
            ..Default::default()
        }],
        output: vec![LiquidTxOut {
            asset: Asset::Explicit(elements::AssetId::from_slice(&[7u8; 32]).unwrap()),
            value: Value::Explicit(50_000 + sample as u64),
            nonce: Nonce::Null,
            script_pubkey: elements::Script::new(),
            witness: LiquidTxOutWitness::default(),
        }],
    }
}

fn print_average_match_estimate(label: &str, attempts: u32, mut sample: impl FnMut() -> u64) {
    let mut rates = Vec::with_capacity(ESTIMATE_SAMPLES);
    let mut hits = Vec::with_capacity(ESTIMATE_SAMPLES);

    for _ in 0..ESTIMATE_SAMPLES {
        let start = Instant::now();
        hits.push(sample());
        rates.push(attempts as f64 / start.elapsed().as_secs_f64());
    }

    rates.sort_by(f64::total_cmp);
    let median_rate = rates[rates.len() / 2];
    let average_match_seconds = ((1u64 << (id::PREFIX_LEN * 8)) as f64) / median_rate;

    println!(
        "{label}: median {:.0} attempts/s over {attempts} attempts; estimated average 3-byte match time {:.2}s; incidental matches per sample {:?}",
        median_rate, average_match_seconds, hits
    );
}

fn print_completed_grind_samples<T>(
    label: &str,
    mut tx_factory: impl FnMut(u32) -> T,
    mut grind: impl FnMut(&T) -> u32,
) {
    let mut timings_ms = Vec::with_capacity(FULL_GRIND_SAMPLES);
    let mut nonces = Vec::with_capacity(FULL_GRIND_SAMPLES);

    for sample in 0..FULL_GRIND_SAMPLES {
        let tx = tx_factory(sample as u32 + 1);
        let start = Instant::now();
        let nonce = grind(&tx);
        timings_ms.push(start.elapsed().as_millis());
        nonces.push(nonce);
    }

    let total_ms: u128 = timings_ms.iter().copied().sum();
    let average_ms = total_ms as f64 / timings_ms.len() as f64;
    let mut sorted = timings_ms.clone();
    sorted.sort_unstable();
    let median_ms = sorted[sorted.len() / 2];

    println!(
        "{label}: completed {FULL_GRIND_SAMPLES} full grinds; avg {:.1}ms, median {}ms, samples {:?}, nonces {:?}",
        average_ms, median_ms, timings_ms, nonces
    );
}

fn benchmark_grinders(c: &mut Criterion) {
    let bitcoin_tx = benchmark_tx();
    let bitcoin_prefix = id::derive_prefix(&BENCH_DOMAIN_ID).expect("prefix should derive");
    let liquid_tx = benchmark_liquid_tx();
    let liquid_prefix = id::derive_prefix(&BENCH_DOMAIN_ID).expect("prefix should derive");

    print_average_match_estimate("bitcoin", BENCH_ATTEMPTS, || {
        id::benchmark_bitcoin_attempts(&bitcoin_tx, &bitcoin_prefix, BENCH_ATTEMPTS)
    });
    print_average_match_estimate("liquid", BENCH_ATTEMPTS, || {
        id::benchmark_liquid_attempts(&liquid_tx, &liquid_prefix, BENCH_ATTEMPTS)
    });
    print_completed_grind_samples("bitcoin_full_grind", benchmark_tx_variant, |tx| {
        id::grind_txid_prefix(tx, &bitcoin_prefix).expect("bitcoin full grind should succeed")
    });
    print_completed_grind_samples("liquid_full_grind", benchmark_liquid_tx_variant, |tx| {
        id::grind_liquid_txid_prefix(tx, &liquid_prefix).expect("liquid full grind should succeed")
    });

    let mut group = c.benchmark_group("grind_attempt_rate");
    group.throughput(Throughput::Elements(BENCH_ATTEMPTS as u64));
    group.bench_function("bitcoin_3_bytes", |b| {
        b.iter(|| {
            black_box(id::benchmark_bitcoin_attempts(
                black_box(&bitcoin_tx),
                black_box(&bitcoin_prefix),
                black_box(BENCH_ATTEMPTS),
            ))
        })
    });
    group.bench_function("liquid_3_bytes", |b| {
        b.iter(|| {
            black_box(id::benchmark_liquid_attempts(
                black_box(&liquid_tx),
                black_box(&liquid_prefix),
                black_box(BENCH_ATTEMPTS),
            ))
        })
    });
    group.finish();
}

fn criterion_config() -> Criterion {
    Criterion::default()
        .sample_size(10)
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(5))
}

criterion_group! {
    name = benches;
    config = criterion_config();
    targets = benchmark_grinders
}
criterion_main!(benches);
