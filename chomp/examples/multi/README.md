# `multi`

This example uses a nested `PolicySpec` tree to show both `And` and `Or` in one
composition. It writes the same typed Borsh payload to Council and to the first
successful chain backend out of Bitcoin or Liquid, then reads it back and
verifies the returned `PolicyKey`.

## Requires all three backends

- a wallet-scoped Bitcoin node RPC configured through `BITCOIND_*`
- a wallet-scoped Liquid/Elements node RPC configured through `LIQUID_*` or `ELEMENTSD_*`
- a council HTTP service configured through `COUNCIL_URL` or the default `http://127.0.0.1:8080`

For a local council service, start:

```bash
cargo run --example mock_council_server
```

For the individual Bitcoin and Liquid setup flows, see:

- [`../bitcoin/README.md`](../bitcoin/README.md)
- [`../liquid/README.md`](../liquid/README.md)

## Usage

Local regtest flow:

```bash
export COUNCIL_URL=http://127.0.0.1:8080

cargo run --example multi -- --payload "hello chomp multi"
```

Make sure the Bitcoin and Liquid wallets behind those RPC endpoints already have confirmed
spendable funds before you run the example.

## Notes

- The policy is `And([Or([bitcoin, liquid]), council:default])`.
- The write order is `bitcoin -> liquid` inside the `Or` branch, then `council`.
- `MultiDa` writes sequentially. There is no rollback if a later member fails after an earlier
  member already accepted the blob.
- The `Or` branch picks the first successful chain backend and persists only that chosen branch.
- The read path tries the chosen chain branch first, then falls back to council if the chain branch is temporarily unreadable.
- Verification requires the chosen chain branch and council to verify.

## Policy Behavior

- `Leaf`: one member, one locator, one verification target
- `And`: write all branches, read any readable branch, verify all branches
- `Or`: try branches in policy order until one write succeeds, then read/verify that chosen branch
- Returned leaf keys carry the chosen `MemberId` alongside the provider locator
- Composite read/verify only falls through on runtime failures and `NotFound`
- Integrity, decode, and usage failures stop immediately
