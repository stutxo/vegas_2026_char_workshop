# CHOMP (Char Object Management Protocol)

CHOMP is a Rust SDK and wire format for publishing Borsh-encoded payloads to
Bitcoin, Liquid, and council-style backends through one consistent interface.

```text
        ╭━━━━━━━━━━━╮
        ┃   ◕    ◕  ┃
        ┃           ┃
        ╰▼╶▼╶▼╶▼╶▼╶▼╯
        ╭▲╶▲╶▲╶▲╶▲╶▲╮
        ╰━━━━━━━━━━━╯
        c h o m p y
```

CHOMP is designed as a crate-root-first SDK:

- import normal application types from `chomp::{...}`
- use `DataAvailability` for blob operations on leaf backends
- use `DataAvailabilityExt` for typed payloads
- use `ChompPayload` for raw bytes
- keep the returned `BlobWriteReceipt` and `PolicyKey` for future reads and verification
- use `Locator` when you need a stable serialized provider-native key

## Install

```toml
[dependencies]
chomp = "0.1"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

## Choose This API

- `DataAvailability`: primary blob DA API for leaf backends
- `DataAvailabilityExt`: typed Borsh helper layer on top of `DataAvailability`
- `ChompPayload`: raw bytes when you do not want a custom payload struct

## Quick Start: Typed Payload

```rust,no_run
use borsh::{BorshDeserialize, BorshSerialize};
use chomp::{BorshPayload, CouncilDa, DaError, DataAvailabilityExt};

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct Vote {
    choice: String,
}

impl BorshPayload for Vote {}

#[tokio::main]
async fn main() -> Result<(), DaError> {
    let da = CouncilDa::new("default", "http://127.0.0.1:8080")?;
    let receipt = da
        .write(&Vote {
            choice: "accept".to_string(),
        })
        .await?;

    let decoded: Vote = da.read(receipt.key()).await?;
    let verify = da.verify(receipt.key()).await?;

    assert_eq!(decoded.choice, "accept");
    assert!(verify.is_read_guaranteed());
    Ok(())
}
```

## Quick Start: Raw Bytes

```rust,no_run
use chomp::{ChompPayload, CouncilDa, DaError, DataAvailabilityExt};

#[tokio::main]
async fn main() -> Result<(), DaError> {
    let da = CouncilDa::new("default", "http://127.0.0.1:8080")?;
    let payload = ChompPayload::from(b"accept".as_slice());
    let receipt = da.write(&payload).await?;
    let decoded: ChompPayload = da.read(receipt.key()).await?;

    assert_eq!(decoded, payload);
    Ok(())
}
```

## Backends

- `CouncilDa`: simplest backend shape, backed by HTTP push/pull endpoints
- `BitcoinDa`: wallet-scoped Bitcoin RPC backend for inscription-based writes
- `LiquidDa`: wallet-scoped Liquid RPC backend for inscription-based writes
- `MultiDa`: policy-aware composite backend over multiple members

Bitcoin and Liquid use explicit config structs:

```rust,no_run
use bitcoin::Network;
use chomp::{BitcoinDa, BitcoinDaConfig, DaError, NamespaceId, OversizePolicy};
use bitcoind_async_client::Client;

fn build_da(client: Client) -> Result<BitcoinDa, DaError> {
    let namespace_id =
        NamespaceId::new(b"btc").expect("hard-coded namespace id should validate");

    Ok(BitcoinDa::new(BitcoinDaConfig {
        network: Network::Regtest,
        client,
        namespace_id,
        fee_policy: BitcoinDaConfig::default_fee_policy(),
        oversize_policy: OversizePolicy::Reject,
    })?)
}
```

`MultiDa` composes member backends with convenience constructors like
`all_of` / `any_of`, or the general `from_spec` builder for nested policies,
and returns a `PolicyKey`:

```rust,no_run
use chomp::{CouncilDa, DaError, MultiDa};

fn build_multi() -> Result<MultiDa, DaError> {
    let council = CouncilDa::new("default", "http://127.0.0.1:8080")?;
    Ok(MultiDa::all_of(vec![council])?)
}
```

For nested policies without repeating `MemberId`s separately, build from
`PolicySpec`:

```rust,no_run
use chomp::{CouncilDa, DaError, MultiDa, policy_spec};

fn build_multi(council: CouncilDa) -> Result<MultiDa, DaError> {
    MultiDa::from_spec(policy_spec!(any(council)))
}
```

## Locators And Receipts

- `BlobWriteReceipt` tells you what `PolicyKey` was written and how many encoded bytes were stored
- `Locator` is the stable serialized provider-native locator for one backend
- `PolicyLeafKey` carries both the `MemberId` and the provider-native `Locator` for one leaf
- `PolicyKey` is the persisted key tree returned by policy-aware writes
- `Policy` describes composite composition as `Leaf` / `And` / `Or`

Leaf backend writes return `PolicyKey::Leaf(...)` containing both the target
member and its locator. Composite writes return `PolicyKey::And(...)` or
`PolicyKey::Or(...)` depending on policy semantics.

Bitcoin and Liquid locators can be:

- a single reveal transaction id
- a chunked locator containing ordered transaction ids for oversized payloads

Council locators are content hashes.

## Policies

- `Policy::Leaf(member)`: use one member
- `Policy::And(children)`: write all, read any, verify all
- `Policy::Or(children)`: write the first successful branch, then read and verify that chosen branch

## Policy Guide

Use `Policy` when you want a persisted/runtime policy tree, and `PolicySpec`
when you want to build that tree directly from live `DaMember` values.
For call-site construction, use `policy_spec!(all(...))` / `policy_spec!(any(...))`.
`PolicySpec` remains the underlying tree type that `MultiDa::from_spec(...)`
consumes.

The current conditions are:

- `Leaf`: write one backend, read that backend, verify that backend
- `And`: write every branch, read the first readable branch in policy order, verify every branch
- `Or`: try branches in policy order until one write succeeds, store that chosen branch, then read and verify only that branch

Retry and stop behavior:

- Retryable during composite read/verify: `RuntimeError` and `SemanticError::NotFound`
- Stops immediately: integrity failures, decode failures, limit/precondition failures, unsupported operations, and all `UsageError`s
- Aggregate exhaustion returns `SemanticError::UnavailableAcrossPolicy(summary)`
- Late write failure after earlier success returns `SemanticError::WriteIncomplete(...)` with the partial `PolicyKey`
- Read and verify reject malformed or mismatched `PolicyKey` trees before dispatch
- Leaf keys include `MemberId`, so `Or` remains unambiguous even when two branches use the same provider kind

Important `Or` limitation:

- `Or` is plain boolean OR, not a redundancy policy
- That means the returned `PolicyKey::Or(...)` stores the chosen successful branch, not a replicated subset
- Plain boolean `And` / `Or` still does not express stronger redundancy contracts like quorum, “at least one verified forever”, or “N independent surviving branches”

The current test suite exercises:

- `all_of`, `any_of`, and `from_spec` policy construction
- `And` write/read/verify behavior
- `Or` chosen-branch persistence
- typed payload round-trips through the blob-first layer
- partial-write reporting
- malformed policy validation and fallback/error classification cases

- `FeePolicy::next_block()`: target next-block pricing
- `FeePolicy::within_blocks(n)`: target inclusion within `n` blocks
- `FeePolicy::manual(rate)`: explicit fee rate in sat/vB
- `OversizePolicy::Reject`: fail when a payload is too large for one standard write
- `OversizePolicy::Chunked { ... }`: split oversized payloads across ordered writes

If a Bitcoin or Liquid payload does not fit in one standard inscription flow
and chunking is enabled, CHOMP falls back to chunked writes automatically.
For Bitcoin and Liquid, CHOMP uses a conservative fixed cutoff of `396_000`
raw payload bytes before it skips straight to chunking; exact transaction-size
checks still run later during transaction construction.

## TXID Targeting

Bitcoin and Liquid derive their txid grind target from the first 3 configured
bytes of `NamespaceId`.

- `NamespaceId::new(b"btc")` targets txids that start with hex `627463`
- `NamespaceId::new(b"lqd")` targets txids that start with hex `6c7164`
- only the first 3 bytes currently matter for txid targeting
- namespace ids shorter than 3 bytes fail validation up front

For Bitcoin and Liquid writes, the returned locator carries the reveal
transaction txid, not the commit txid.

## Advanced Backend Authoring

If you are implementing a custom backend, implement `chomp::da::DataAvailability`.
That trait is the object-safe blob layer underneath `DataAvailabilityExt`.

## Advanced Module Layout

The crate root is the recommended application entrypoint. These modules remain
available when you want the domain grouped explicitly:

- `chomp::bundle`: encoding-facing types and codec helpers
- `chomp::core`: shared errors and member identifiers
- `chomp::da`: locators, policies, backend traits, and backend implementations

## Current Boundaries

- Proof support is not implemented in this crate yet.
- Bitcoin and Liquid share the same SDK-facing config pattern, but this README
  does not document every chain-specific operational detail of the underlying
  RPC flows.

## Examples

- [examples/bitcoin/main.rs](./examples/bitcoin/main.rs)
- [examples/council/main.rs](./examples/council/main.rs)
- [examples/liquid/main.rs](./examples/liquid/main.rs)
- [examples/multi/main.rs](./examples/multi/main.rs)

## License

MIT. See [LICENSE](./LICENSE).
