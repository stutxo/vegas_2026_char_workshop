# Hello Char

Example app that uses **only** the Char SDK (`char-sdk`). No direct use of transport/semantics/framing crates - everything goes through the facade.

## What it does

- **Domain**: Builds `DomainId` from config (preimage hex).
- **Vote**: The example implements [`CharBallotHandlers`](../../crates/char-semantics/src/ballot_handlers.rs) (`produce_payload` / `on_roll_observed`). The SDK runners submit **raw payload bytes as hex** over RPC (`addreferendumvote`); the node binds the pending ballot. `char-framing` encode/decode is for the **binary `ReferendumVote::data`** layout (CompactSize + payload), not the RPC vote hex string.
- **Semantics**: Uses `char_sdk::run_rpc` / `run_zmq`, which call `getdomaininfo`, leader checks, `add_referendum_vote`, and `get_referendum_decision_roll` as needed; you can also use `pending_ballot`, `check_leader`, `reconcile`, `submit_vote`, and `Progress` from the same crate for custom flows.
- **Loop**: Runs continuously (one round every few seconds); stop with **Ctrl+C**.

## Run

Requires a Char node (e.g. regtest; see `doc/regtest.md`).

### One-shot vs multi-terminal

- **One-shot script** (below): one command starts the node in the background (daemon), bootstraps, then runs the app in the same shell. The node and app are separate processes; the script just runs them in sequence. This can be flaky (e.g. pending store or bond selection) depending on timing and existing `char_data`.
- **Multi-terminal** (recommended for development): run the node in one terminal (e.g. `bitcoind -regtest -char_node=1 -charbondindex=1 ...`), then in another terminal bootstrap and run the app (`cargo run -p hello-char`), and in a third verify with `cli getreferendumdecisionroll "68656c6c6f2d63686172" <start> <end>`. Same process model as the one-shot script, but you control node vs app separately and can watch node logs. For the full flow (ZMQ, `char.env`, bootstrap script), see **contrib/char/examples/hello-char-demo**.

**Multi-terminal quick start:** Terminal A: start node (`bitcoind -regtest -datadir=$CHAR_DATA_DIR -rpcport=18443 -char_node=1 -charbondindex=1 -server=1`). Terminal B: from repo root, run the bootstrap steps (createwallet, 101 blocks, bond, fund stake, 7+6 blocks, schedule, attestbonds, addreferendumvote) then `cd contrib/char/sdk && cargo run -p hello-char` with `CHAR_RPC_URL`, `CHAR_COOKIE_PATH`, `CHAR_DOMAIN_HEX` set. The node wallet must load the bond you care about: **`run_rpc` / `run_zmq` do not take a bond parameter**; they call `getdomaininfo`, `get_leader_for_slot_current_block`, and `getallcharbonds 0` (wallet bonds) each cycle to decide if it is your turn to submit. Terminal C: `cli getreferendumdecisionroll "68656c6c6f2d63686172" <start> <end>` to inspect decision rolls.

### One-shot (build + regtest + hello-char)

From **char-bitcoin repo root**:

```bash
contrib/char/sdk/examples/hello-char/run-regtest.sh        # RPC polling (default)
contrib/char/sdk/examples/hello-char/run-regtest.sh --zmq  # ZMQ (leader + decisionroll)
```

The script starts the node with ZMQ publishers enabled (`-zmqpubleader`, `-zmqpubdecisionroll`) so either mode works. Uses `./char_data` (or `CHAR_DATA_DIR`). For a clean run: `rm -rf char_data` first.

### If you get "No pending ballot available", bond mismatch, or HTTP 500 on `getdomaininfo`

The example uses **`getdomaininfo`** plus **`get_leader_for_slot_current_block`** and **`getallcharbonds 0`** (wallet bonds only) on each poll—no integrator-supplied bond txid. If the node cannot select a leader (**"No active bonds in the network..."**), `getdomaininfo` fails. If you are never the leader, confirm the loaded wallet owns a bond that appears in `getallcharbonds 0` and matches **`next_leader_bond`** when it is your slot.

**1. Build the node** (from char-bitcoin repo root, once):

```bash
cmake -B build
cmake --build build
```

**2a. If using the one-shot script and it still returns 500:** stop the node, start it again, load the wallet, mine one block, then run hello-char (so the bond signer rescans from disk and the worker fills the pending store on that block).

**2b. Schedule domain and mine a block** (from char-bitcoin repo root; node must be running, wallet with bond loaded):

```bash
export CHAR_DATA_DIR="$HOME/.char-demo/hello-char-node"

./build/bin/bitcoin-cli -regtest -datadir="$CHAR_DATA_DIR" -rpcport=18443 domain_registry schedule "68656c6c6f2d63686172" "hello-char"
./build/bin/bitcoin-cli -regtest -datadir="$CHAR_DATA_DIR" -rpcport=18443 generatetoaddress 1 $(./build/bin/bitcoin-cli -regtest -datadir="$CHAR_DATA_DIR" -rpcport=18443 getnewaddress)
```

Then run hello-char again (from SDK root):

```bash
CHAR_RPC_URL="http://127.0.0.1:18443" \
CHAR_COOKIE_PATH="$CHAR_DATA_DIR/regtest/.cookie" \
CHAR_DOMAIN_HEX="68656c6c6f2d63686172" \
cargo run -p hello-char
```

### From SDK root (`contrib/char/sdk/`)

```bash
# Minimal: RPC URL and cookie (default port 18443)
CHAR_RPC_URL=http://127.0.0.1:18443/ cargo run -p hello-char
```

Or with all optional env (e.g. after the hello-char bootstrap). **Set `RPC_PORT`** (e.g. `18443`) so `CHAR_RPC_URL` has a port:

```bash
export CHAR_DATA_DIR="$HOME/.char-demo/hello-char-node"
export RPC_PORT=18443

CHAR_RPC_URL="http://127.0.0.1:${RPC_PORT}" \
CHAR_COOKIE_PATH="$CHAR_DATA_DIR/regtest/.cookie" \
CHAR_DOMAIN_HEX="68656c6c6f2d63686172" \
cargo run -p hello-char
```

Optional env:

- **CHAR_COOKIE_PATH** - path to node `.cookie` (default: `char_data/regtest/.cookie` or `../../char_data/regtest/.cookie` from SDK root).
- **CHAR_DOMAIN_HEX** - domain preimage hex (default: `636861722e6e6574776f726b2f68656c6c6f` = "char.network/hello"). Use the same domain as your bootstrap (e.g. for tag `hello-char`: `68656c6c6f2d63686172`).
- **CHAR_USE_ZMQ** - set to `1` (or pass `--zmq`) to use the ZMQ path instead of RPC polling: one reconcile, then listen on `leader` and `decisionroll` ZMQ topics; on leader notification we submit only when the notification matches **`getdomaininfo.next_ballot`** and the wallet-bond leader check passes (same RPC chain as the poll path). Optional ZMQ env: **CHAR_ZMQ_LEADER_ADDR** (default `tcp://127.0.0.1:28332`), **CHAR_ZMQ_DECISIONROLL_ADDR** (default same as leader).

If `CHAR_RPC_URL` is missing, the program exits with an error.

**ZMQ vs RPC:** The node sends leader ZMQ notifications for **every** (domain, bond) when it processes pending stores - so you see notifications for many ballot numbers (e.g. 128 for another bond). Both paths refresh **`getdomaininfo`** on each decision and require the wallet-bond RPC chain (`get_leader_for_slot_current_block` + **`getallcharbonds 0`**) before submitting, and ZMQ only submits when the notification ballot matches **`next_ballot`**. Otherwise you'd submit for other bonds' ballots and get "rejected".

### From char-bitcoin repo root

Use `--manifest-path` and set `RPC_PORT` (e.g. `18443`) so `CHAR_RPC_URL` is correct:

```bash
export CHAR_DATA_DIR="$HOME/.char-demo/hello-char-node"
export RPC_PORT=18443

CHAR_RPC_URL="http://127.0.0.1:${RPC_PORT}" \
CHAR_COOKIE_PATH="$CHAR_DATA_DIR/regtest/.cookie" \
CHAR_DOMAIN_HEX="68656c6c6f2d63686172" \
cargo run -p hello-char --manifest-path contrib/char/sdk/Cargo.toml
```

If you didn't save `BOND_TXID`, run `getallcharbonds 0` and use the bond's `txid` that has `amount` > 0.

**If submit returns "add_referendum_vote returned false":** The node inserts the vote into the pending store for **every** loaded bond. If any bond's store already has that ballot (e.g. duplicate submit) or is ahead, the RPC returns false. Mine one block to advance the attestation worker, then run the app again so it submits for the new pending ballot:

```bash
cli generatetoaddress 1 "$ADDR"
# then run the cargo command again
```

### Debugging `Transport(Rpc { code: 500, message: "Internal Server Error" })`

The node often returns **HTTP 500** for Char RPC errors; the **real error is in the response body**. To see it, call the same RPC with curl (replace `USER:PASS` with the first line of your `.cookie` file):

```bash
curl -s -u USER:PASS -X POST -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"getdomaininfo","params":["68656c6c6f2d63686172"],"id":1}' \
  http://127.0.0.1:18443/
```

Check the JSON `error.message`. Inspect **`next_leader_bond`** and run **`getallcharbonds 0`** to see wallet bonds; the example submits only when that leader bond is yours per the RPC chain above. Common causes: **"Char bond index not enabled"** -> use `-charbondindex=1`; **"No active bonds in the network..."** -> bond index or chain not ready; domain not scheduled or node not caught up.

## Layout

- `src/main.rs` - Transport, domain, bond; [`HelloCharApp`](./src/main.rs) implements [`CharBallotHandlers`](../../crates/char-semantics/src/ballot_handlers.rs) with `#[async_trait]` from `char-sdk` (async `produce_payload` / `on_roll_observed` so apps can `.await` I/O); payload per ballot is `hello {ballot}`; dispatch RPC vs ZMQ (`--zmq` / `CHAR_USE_ZMQ`).
- `src/rpc_example.rs` / `src/zmq_example.rs` - thin wrappers around `char_sdk::run_rpc` / `run_zmq` with the example app state.
- The SDK runners own the loop; the example wires **app hooks** + transport.

Environment aliases (for CI / functional tests): **`BITCOIND_URL`** / **`BITCOIND_COOKIE`** for `CHAR_RPC_URL` / `CHAR_COOKIE_PATH`; **`DOMAIN_PREIMAGE_HEX`** for `CHAR_DOMAIN_HEX`.
