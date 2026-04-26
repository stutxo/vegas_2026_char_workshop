# `council`

This example shows a council-only `MultiDa` policy with four logical council
members:

- `seattle`
- `london`
- `tokyo`
- `sydney`

The policy is:

- `And([Or([seattle, london]), Or([tokyo, sydney])])`

That means each write must succeed on one council from the Seattle/London pair
and one council from the Tokyo/Sydney pair. The returned `PolicyKey` records
which branch was chosen in each `Or`.

## Server contract

The server must expose:

- `POST /push/<sha256-hex>` with the serialized Borsh payload request body
- `POST /push/<sha256-hex>` returning `204 No Content` on success
- `GET /pull/<sha256-hex>` returning the serialized Borsh payload as `application/octet-stream` or `404`

[`mock_council_server`](../mock_council_server/README.md) implements that contract for local
testing.

## Usage

Point the example at the council server:

```bash
export COUNCIL_URL=http://127.0.0.1:8080
```

You can also override any member individually:

```bash
export COUNCIL_SEATTLE_URL=http://127.0.0.1:8080
export COUNCIL_LONDON_URL=http://127.0.0.1:8080
export COUNCIL_TOKYO_URL=http://127.0.0.1:8080
export COUNCIL_SYDNEY_URL=http://127.0.0.1:8080
```

If the per-member URLs are unset, the example falls back to `COUNCIL_URL` for
all four logical councils. That keeps local testing simple with one
`mock_council_server`, while still demonstrating policy behavior with distinct
member ids.

Then write and read a payload:

```bash
cargo run --example council -- --payload "hello council"
```
