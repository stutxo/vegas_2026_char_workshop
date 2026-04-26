# Examples

Each example now lives in its own folder with a colocated `README.md`.

- [`bitcoin`](./bitcoin/README.md): write, read, and verify a typed Borsh payload against a Bitcoin regtest wallet RPC
- [`council`](./council/README.md): demonstrate a council-only `MultiDa` policy across four logical council members
- [`mock_council_server`](./mock_council_server/README.md): tiny in-memory HTTP server for testing `CouncilDa` serialized payload storage
- [`multi`](./multi/README.md): write the same typed Borsh payload across Bitcoin regtest, Liquid regtest, and Council via `MultiDa` with an explicit `Policy::And`
- [`liquid`](./liquid/README.md): write, read, and verify a typed Borsh payload against a Liquid regtest wallet RPC
