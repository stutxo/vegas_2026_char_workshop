# `bitcoin`

This is the smallest Bitcoin regtest example: write one `ChompPayload`, read it
back, and verify the locator.

Bitcoin writes use inscriptions only. Pass `--chunked` only if you want the
example to allow oversized inscription payloads to be split across multiple
transactions.

## Environment

Point the example at a wallet-scoped Bitcoin RPC endpoint:

```bash
export BITCOIND_RPC_URL=http://127.0.0.1:18443
export BITCOIND_RPC_WALLET=da_wallet
export BITCOIND_COOKIE_FILE=/path/to/.cookie
```

Or use RPC username and password:

```bash
export BITCOIND_RPC_URL=http://127.0.0.1:18443
export BITCOIND_RPC_WALLET=da_wallet
export BITCOIND_RPC_USER=<rpc-user>
export BITCOIND_RPC_PASSWORD=<rpc-password>
```

If `BITCOIND_RPC_URL` already includes `/wallet/<name>`, you can omit `BITCOIND_RPC_WALLET`.

## Usage

Run it with a string payload:

```bash
cargo run --example bitcoin -- --payload "hello chomp"
```

Or load the payload from a file:

```bash
cargo run --example bitcoin -- --payload-file /path/to/blob.bin
```

Allow chunking for oversized payloads:

```bash
cargo run --example bitcoin -- --chunked --payload-file /path/to/blob.bin
```

When chunking is enabled, the Bitcoin backend uses a conservative fixed cutoff
of `396_000` raw payload bytes before it switches from one inscription to
chunked writes.

## Funding

The example uses the connected Bitcoin wallet to fund the write through wallet RPCs. Make sure the
wallet behind `BITCOIND_RPC_URL` already has confirmed spendable funds before you run it. Bitcoin
inscription writes use ordinary fee-paying commit/reveal transactions, with the commit funded by
the wallet and the reveal returning value to a fresh wallet-owned address. When `--chunked` is
enabled, each chunk is still funded from confirmed wallet inputs only.
