# `liquid`

This is the smallest Liquid regtest example: write one `ChompPayload`, read it
back, and verify the locator.

The example uses the connected Elements wallet directly. There is no separate
SDK funding key or external funding address. Liquid writes use inscriptions
only. Pass `--chunked` only if you want the example to allow oversized
inscription payloads to be split across multiple transactions.

## 1. Start an Elements regtest node

Use a fresh datadir if you want the `initialfreecoins` setup to apply from
chain initialization.

```bash
./src/elementsd \
  -datadir=/tmp/elements-da \
  -chain=elementsregtest \
  -server=1 \
  -daemon=1 \
  -rpcbind=127.0.0.1 \
  -rpcport=18884 \
  -txindex=1 \
  -validatepegin=0 \
  -fallbackfee=0.0002 \
  -minrelaytxfee=0 \
  -initialfreecoins=2100000000000000 \
  -anyonecanspendaremine=1 \
  -con_connect_genesis_outputs=1 \
  -con_blocksubsidy=0 \
  -evbparams=taproot:1:::
```

## 2. Create or load a wallet

```bash
./src/elements-cli \
  -datadir=/tmp/elements-da \
  -chain=elementsregtest \
  -rpcport=18884 \
  createwallet da_wallet false false "" false false
```

If the wallet already exists, load it instead:

```bash
./src/elements-cli \
  -datadir=/tmp/elements-da \
  -chain=elementsregtest \
  -rpcport=18884 \
  loadwallet da_wallet
```

## 3. Claim the initial regtest funds

These default-asset outputs require a claim transaction and 100-block maturity
before they become spendable.

```bash
CLI="./src/elements-cli -datadir=/tmp/elements-da -chain=elementsregtest -rpcport=18884 -rpcwallet=da_wallet"

CLAIM_ADDR=$($CLI getnewaddress)
$CLI sendtoaddress "$CLAIM_ADDR" 21000000 "" "" true
$CLI generatetoaddress 101 "$($CLI getnewaddress)"
$CLI getwalletinfo
```

After that, `getwalletinfo` should show a spendable `balance.bitcoin`.

## 4. Point the example at the wallet RPC

```bash
export LIQUID_RPC_URL=http://127.0.0.1:18884
export LIQUID_RPC_WALLET=da_wallet
export LIQUID_COOKIE_FILE=/tmp/elements-da/elementsregtest/.cookie
```

The example also accepts a directory for `LIQUID_COOKIE_FILE`, as long as it can
find a single `.cookie` file underneath it. If `LIQUID_RPC_URL` already includes
`/wallet/<name>`, you can omit `LIQUID_RPC_WALLET`.

## 5. Write and read a typed payload

```bash
cargo run --example liquid -- --payload "hello chomp liquid"
```

For oversized payloads, opt into chunking explicitly:

```bash
cargo run --example liquid -- --chunked --payload-file /path/to/blob.bin
```

When chunking is enabled, the Liquid backend uses a conservative fixed cutoff
of `396_000` raw payload bytes before it switches from one inscription to
chunked writes.

## Recovery note

Liquid inscription commits are funded by the wallet and the reveal is broadcast
immediately after the commit. If reveal broadcast fails after the commit is
accepted, the SDK surfaces the commit txid so the operator can recover or
inspect the outstanding commit output manually.

## Troubleshooting

- `Could not locate RPC credentials`: make sure the CLI and daemon both use `-chain=elementsregtest`, or set `LIQUID_COOKIE_FILE` to the exact cookie file.
- `balance.bitcoin = 0`: use a fresh datadir, start the node with `-initialfreecoins` and `-anyonecanspendaremine=1`, and use the claim flow above.
- Wallet RPC errors mentioning `getnewaddress`, `fundrawtransaction`, or `walletsignpsbt`: make sure `LIQUID_RPC_URL` points at a wallet endpoint such as `/wallet/da_wallet`.
