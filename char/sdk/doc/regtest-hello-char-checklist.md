# Regtest + Hello-Char: What We Learned

Quick checklist for bringing up a Char regtest node and running the hello-char example from scratch.

## 1. Start the node (one command)

Create datadir first, then start with **all** flags you'll need (avoids restarts):

```bash
mkdir -p /path/to/char-bitcoin/char_data
./build/bin/bitcoind -regtest -datadir=/path/to/char-bitcoin/char_data -rpcport=18443 -char_node=1 -fallbackfee=0.00001 -maxtxfee=1000
```

Use **absolute path** for `-datadir` so `bitcoin-cli` finds the same cookie.

## 2. Wallet + bond + stake

- Create wallet, mine 101 blocks to it so it has coins.
- `walletcreatetaprootoutputforcharbond` (needs wallet loaded, fallbackfee).
- Mine **1** block to confirm the bond tx.
- **fundcharstake** to give the bond stake (amount > 0).
- Mine **7** blocks to activate bond and stake (activation delays). Only then does the bond show `amount` > 0 in `getallcharbonds` and become "selectable."

## 3. Domain + pending ballot

- **Schedule the domain** (required for Char referendum / `getdomaininfo` to be meaningful):
  - `domain_registry schedule "636861722e6e6574776f726b2f68656c6c6f" "hello"`
- **Load the wallet that owns the bond** (so the node includes it in "participating" bonds).
- Progress to a pending ballot: either **add a referendum vote** (or use `addreferendumvote` to seed), or **wait ~20s** at ballot 0 and the node will null-fill and advance. In regtest you can instead **mine 1 block** to trigger processing immediately.
- Optional: wait a few seconds for the attestation worker to run.
- `getdomaininfo` for your domain should then return `next_ballot`, `next_leader_bond`, etc.; your bond txid should match `next_leader_bond` when you are the next leader.

## 4. Hello-char example

- **Cookie:** Set `CHAR_COOKIE_PATH` to the **absolute** path of `char_data/regtest/.cookie` (relative paths often fail).
- **Bond / leader:** The example does **not** take a bond txid from code or from `CHAR_BOND_TXID` (that env var is not read by hello-char). The SDK uses `getdomaininfo` + `get_leader_for_slot_current_block` + `getallcharbonds` to decide if your loaded wallet owns the next-leader bond. Use a bond with **stake** (amount > 0 in `getallcharbonds`) and load the wallet that owns it.
- **Submit:** In `is_leader` mode, only the **leader** for that (domain, ballot) can submit. The wallet that owns the **leader** bond must be loaded. The runner follows the node's `next_ballot` (ZMQ) or RPC poll loop; if your pending ballot has moved past 0, the app targets the **current** pending ballot (or you get "add_referendum_vote returned false").

## 5. Gotchas

- **Which wallet owns which bond:** `getallcharbonds 0` shows only bonds owned by loaded wallets. Leader for a ballot must be in that set for submit to succeed.
- **Stake activation:** 6 blocks after the block that confirmed `fundcharstake` before stake counts. Bond activation: 6 blocks after bond tx confirmation.
- **Pending store / leadership:** The node progresses when it processes a block (or advances after timeout) **and** the domain is scheduled (`domain_registry schedule`) **and** bonds are visible to the bond index. Load the wallet that owns your bond, mine or wait for progress, then **`getdomaininfo`** should report the next ballot and leader bond.

## 6. Clean slate

To start over: use a **new** datadir (e.g. `char_data2`) or remove the old one. Same datadir = same chain and old state.
