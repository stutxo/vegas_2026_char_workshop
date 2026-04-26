# vegas_2026_char_workshop

This repo contains a template hello world app that you can use to tell your ai to make a real char app

## Prerequisites

Install the Rust toolchain, which includes `rustc` and `cargo`. This project uses
the Rust 2024 edition, so use Rust 1.85 or newer.

The easiest way to install Rust is with `rustup`:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup default stable
cargo --version
```

That is the only required install for the workshop template. The Char SDK and
Chomp code are included in this repo as local path dependencies.

You will need to edit the template with the char workshop rpc ip address and password (you will get these at the workshop)

To run the template:

```bash
cargo run
```

# Challenge 1

Make a char app using the code in this repo as a starting point using the workshop char node

simple hello world template in this repo at src/

see char/sdk/examples for help

## Challenge 2

Add chomp DA to your app and post data to bitcoin via the workshop char node

see chomp/examples for help

## Char transactions

Signet bond anchor transaction

https://mempool.space/signet/tx/13ef85b44db8b4b928ec9e80d1e4338f397f8fea87e805ad431e354010924a0d

Signet bond fund transaction

https://mempool.space/signet/tx/ade62b3bead373aaaec66a53421bd415cbaad82be703e73f761b4aad3895ac69

Chomp mutinynet DA tx

https://mutinynet.com/tx/3224fe7435833c1ff451d4713fb66df7e4e89d56ca2a1f9e1a2b42ad4db035c3


## Purify

Vefifiable deterministic nonces

https://github.com/judica-org/purify

## charca.de

aws enclave for bitcoin price attestations

https://charcad.de

## Oracle for charcade

https://github.com/stutxo/choracle
