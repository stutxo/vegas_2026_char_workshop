# `mock_council_server`

This example is a tiny in-memory HTTP server for testing [`CouncilDa`](../council/README.md).

It stores serialized Borsh payload bytes by SHA-256 hash and exposes:

- `POST /push/<sha256-hex>` to store serialized Borsh payload bytes and return `204 No Content`
- `GET /pull/<sha256-hex>` to fetch serialized Borsh payload bytes as `application/octet-stream`

## Usage

Start the mock server:

```bash
cargo run --example mock_council_server
```

By default it listens on `127.0.0.1:8080`. You can change that with:

```bash
export COUNCIL_BIND_ADDR=127.0.0.1:18080
```

or:

```bash
cargo run --example mock_council_server -- --bind 127.0.0.1:18080
```

Then point [`council`](../council/README.md) at the same URL and run it.
