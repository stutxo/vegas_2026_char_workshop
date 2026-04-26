# Char Rust SDK

Production-grade Rust SDK for integrating with the Char node (Bitcoin-anchored coordination layer for AVSes). This directory is the **SDK root** within the char-bitcoin repo.

## Contents

- **SDKSPEC.md** - Full draft specification: crate layout, transport/semantics/framing/utils, RPC types, progress model, sync/submit, precommit framing, adapters, testing, milestones.
- **Cargo.toml** - Workspace definition. Crates will be added under `crates/`, with `char-sdk` as the single entry point for integrators.
- **crates/** - Core library crates: `char-sdk`, `char-transport`, `char-semantics`, `char-framing`, `char-utils`.
- **char-mock/** - In-memory mock RPC/ZMQ for tests.
- **char-conformance/** - Conformance harness against a real Char node.
- **examples/** - e.g. `hello-char` generic demo.
- **adapters/** - Integration adapters (e.g. `char-adapter-alpen`); depend only on `char-sdk`.

## Build and test

From the SDK root (`contrib/char/sdk/`):

```bash
# Build everything
cargo build

# Run all unit tests (all crates)
cargo test

# Run only integration tests (char-sdk facade)
cargo test -p char-sdk --tests

# Build release
cargo build --release

# Run the hello-char example (requires a Char node)
#
# Set CHAR_RPC_URL (or BITCOIND_URL) and CHAR_COOKIE_PATH (or BITCOIND_COOKIE) to your
# node's RPC endpoint and cookie file. Optional: CHAR_DOMAIN_HEX, ZMQ addrs, poll timings — see
# examples/hello-char/README.md.
cargo run -p hello-char
```

Crates are implemented per the spec. The **hello-char** example talks to a real node via `BitcoindAsyncTransport` (JSON-RPC): domain schedule, leader/submit flow, and RPC or ZMQ runners. Use **`char-sdk` integration tests** or a dedicated mock crate for in-process mocks.

## Char node

The Char node (C++) lives in this same repo: `src/char/`, `src/rpc/char_rpc.cpp`, etc. Conformance tests run against the node built from this repo.
