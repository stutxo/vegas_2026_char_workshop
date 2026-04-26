#!/usr/bin/env bash
# One-shot: from char-bitcoin repo root:
#   contrib/char/sdk/examples/hello-char/run-regtest.sh       # RPC mode
#   contrib/char/sdk/examples/hello-char/run-regtest.sh --zmq # ZMQ mode (leader + decisionroll)
# Builds node, starts regtest, creates wallet+bond+stake, schedules domain, seeds ballot, runs hello-char.
# For a clean run: rm -rf char_data (or set CHAR_DATA_DIR to an empty dir).
#
# After schedule, seed pending store with addreferendumvote (use init when node supports it).
set -e
REPO_ROOT="$(cd "$(dirname "$0")/../../../../.." && pwd)"
cd "$REPO_ROOT"
DATA_DIR="${CHAR_DATA_DIR:-$REPO_ROOT/char_data}"
# Avoid ~/.cargo permission issues when the sandbox or home dir is not writable.
export CARGO_HOME="${CARGO_HOME:-$DATA_DIR/cargo-cache}"
mkdir -p "$CARGO_HOME"
RPC_PORT="${RPC_PORT:-18443}"
ZMQ_PORT="${ZMQ_PORT:-28332}"
ZMQ_ADDR="tcp://127.0.0.1:${ZMQ_PORT}"
DOMAIN_HEX="68656c6c6f2d63686172"   # "hello-char"

USE_ZMQ=false
for arg in "$@"; do
  if [[ "$arg" == "--zmq" ]]; then USE_ZMQ=true; break; fi
done

echo "== Build =="
if [[ ! -x ./build/bin/bitcoind ]]; then
  cmake -B build -DWITH_ZMQ=ON
  cmake --build build
fi
cli() { ./build/bin/bitcoin-cli -regtest -datadir="$DATA_DIR" -rpcport="$RPC_PORT" "$@"; }

echo "== Stop any existing node =="
cli stop 2>/dev/null || true
sleep 2

echo "== Start node =="
mkdir -p "$DATA_DIR"
BITCOIND_ARGS=(-regtest -datadir="$DATA_DIR" -rpcport="$RPC_PORT" -charenable -fallbackfee=0.00001 -maxtxfee=1000 -server=1 -daemon)
# Enable ZMQ so hello-char can use --zmq (leader + decisionroll topics)
BITCOIND_ARGS+=(-zmqpubleader="$ZMQ_ADDR" -zmqpubdecisionroll="$ZMQ_ADDR")
./build/bin/bitcoind "${BITCOIND_ARGS[@]}"
sleep 3
echo -n "Waiting for RPC"
until cli getblockcount &>/dev/null; do echo -n "."; sleep 1; done
echo " ok"

echo "== Wallet + 101 blocks =="
cli createwallet char false false "" false true 2>/dev/null || cli loadwallet char 2>/dev/null || true
ADDR=$(cli -rpcwallet=char getnewaddress)
cli generatetoaddress 101 "$ADDR" >/dev/null

echo "== Create bond + fund stake =="
BOND_OUT=$(cli -rpcwallet=char walletcreatetaprootoutputforcharbond)
BOND_TXID=$(echo "$BOND_OUT" | sed -n 's/.*"txid"[[:space:]]*:[[:space:]]*"\([0-9a-fA-F]\{64\}\)".*/\1/p')
if [[ -z "$BOND_TXID" || ${#BOND_TXID} -ne 64 ]]; then
  echo "Failed to get bond txid from walletcreatetaprootoutputforcharbond" >&2
  echo "$BOND_OUT" >&2
  exit 1
fi
cli -rpcwallet=char fundcharstake "$BOND_TXID" 0.1 1008 >/dev/null
# Mine 7 blocks so our bond is selectable and gets a pending store
cli generatetoaddress 7 "$(cli -rpcwallet=char getnewaddress)" >/dev/null

echo "== Seed pending store (domain is scheduled by the app) =="
# hello-char uses wallet bonds from the node (getallcharbonds 0); no bond txid env. Node attests automatically every 1s.
# Payload must match HelloCharApp::expected_payload(0) (UTF-8 "hello 0") or reconcile fails after trust_script_seeded_ballot_zero().
HELLO0_HEX=68656c6c6f2030
cli -rpcwallet=char addreferendumvote "[{\"$DOMAIN_HEX\":\"$HELLO0_HEX\"}]" is_leader || true

echo "== Hello-char =="
export CHAR_RPC_URL="http://127.0.0.1:${RPC_PORT}"
export CHAR_COOKIE_PATH="$DATA_DIR/regtest/.cookie"
if [[ ! -f "$CHAR_COOKIE_PATH" ]]; then
  echo "Cookie file not found. Set CHAR_COOKIE_PATH to your node's .cookie (e.g. char_data/regtest/.cookie)." >&2
  exit 1
fi
export CHAR_DOMAIN_HEX="$DOMAIN_HEX"
export CHAR_ZMQ_LEADER_ADDR="$ZMQ_ADDR"
export CHAR_ZMQ_DECISIONROLL_ADDR="$ZMQ_ADDR"
cd contrib/char/sdk
if $USE_ZMQ; then
  echo "Mode: ZMQ (leader + decisionroll)"
  cargo run -p hello-char -- --zmq
else
  cargo run -p hello-char
fi
