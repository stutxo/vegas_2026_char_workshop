# Running Char (bitcoind) in regtest

Commands to run the node in regtest so you can point the SDK (or hello-char with a real transport) at it. Assumes the char-bitcoin repo is built. Run everything from the **repo root**.

## 1. Build the node (from repo root)

```bash
cmake -B build
cmake --build build
```

Binaries are in `build/bin/` (e.g. `build/bin/bitcoind`, `build/bin/bitcoin-cli`).

## 2. Start bitcoind in regtest (with Char enabled)

**Using `char_data` in the repo (gitignored):**

```bash
./build/bin/bitcoind -regtest -datadir=./char_data -rpcport=18443 -char_node=1
```

- **`-char_node=1`** - Enables Char (p2p relay, bond index, Char RPCs like `getdomaininfo`, `addreferendumvote`). Without this, Char RPCs won't be available.
- Data dir: `./char_data` (regtest chain under `char_data/regtest/`).
- RPC: **18443**.
- Auth: cookie at `./char_data/regtest/.cookie` after first run.

**Or with an absolute datadir:**

```bash
./build/bin/bitcoind -regtest -datadir=/Users/you/path/to/char-bitcoin/char_data -rpcport=18443 -char_node=1
```

## 3. Talk to the node (bitcoin-cli)

**The node must be running first.** If you see `Could not connect to the server 127.0.0.1:18443`, start `bitcoind` (step 2) in another terminal and wait until it's up.

Use the **same** `-regtest`, `-datadir`, and `-rpcport` as the running node:

```bash
./build/bin/bitcoin-cli -regtest -datadir=./char_data -rpcport=18443 getblockchaininfo
./build/bin/bitcoin-cli -regtest -datadir=./char_data -rpcport=18443 getblockcount
```

Char RPCs (e.g. `getdomaininfo`, `addreferendumvote`) use the same endpoint and auth.

**Absolute path example:**

```bash
./build/bin/bitcoin-cli -regtest -datadir=/Users/setzeus/Documents/github/char/feb-23-overview/char-bitcoin/char_data -rpcport=18443 getblockchaininfo
```

## 4. Connect your app

- **RPC URL:** `http://127.0.0.1:18443/`
- **Auth:** Cookie file at `char_data/regtest/.cookie` (one line `USER:PASS`). Use for HTTP Basic auth in your RPC client.

The SDK exposes only the `CharRpcTransport` trait; it does not ship an HTTP implementation. The **hello-char** example includes a small HTTP transport (in the example only) so you can run it against regtest:

```bash
# From contrib/char/sdk/
CHAR_RPC_URL=http://127.0.0.1:18443/ cargo run -p hello-char
```

Optional: `CHAR_COOKIE_PATH` (default: `char_data/regtest/.cookie`). To talk to a node from your own app, implement `CharRpcTransport` (e.g. HTTP JSON-RPC with reqwest and cookie auth) and pass it into the semantics APIs.

## 5. Stop

```bash
./build/bin/bitcoin-cli -regtest -datadir=./char_data -rpcport=18443 stop
```
