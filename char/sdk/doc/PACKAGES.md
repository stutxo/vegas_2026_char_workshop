# Char SDK: Role of the Four Packages (and the Facade)

The SDK is structured as **four internal crates** plus **one facade crate** that integrators depend on. This document explains each package's role and why the split exists.

**Dependency rule (from SDKSPEC):** Integrators depend **only** on `char-sdk`. They do not depend on `char-transport`, `char-semantics`, `char-framing`, or `char-utils` directly. The four internal crates are implementation details; `char-sdk` re-exports the public API.

**Layering (bottom -> top):**

```
  char-sdk          <-  Single entry point (re-exports; no logic)
       |
  char-semantics    <-  Protocol logic (domain, progress, sync, submit, retry, gaps)
       |
  char-transport    char-framing    char-utils
  (RPC + ZMQ)       (vote/precommit (domain hash, hex,
   wire only)        wire encode/decode)  helpers)
```

---

## 1. char-utils - Shared Primitives (No I/O, No Protocol)

**Role:** Pure helpers used by framing and semantics. No network, no Char-specific protocol rules.

**Why it exists:** Domain hash and hex handling must be **canonical** and **shared**. Char nodes define domain identity as `SHA256(CompactSize(len) || preimage)`; RPC and ZMQ use hex. Putting this in one crate avoids duplication and keeps a single place to align with the node.

**What's in it:**

| Area | Contents | Rationale |
|------|----------|-----------|
| **Domain hash** | `domain_hash(preimage)`, `domain_hash_from_hex`, `domain_hash_to_hex` | Matches Char node `DomainToDomainIdentifier` in `src/rpc/char_rpc.cpp`: `HashWriter << DomainToBytes(domain)` - Bitcoin Core encodes `vector<unsigned char>` as CompactSize(len) then bytes. SHA256 of that encoding is the 32-byte domain id. |
| **Hex** | `hex_to_bytes`, `bytes_to_hex`, `strip_0x_prefix` | RPC and ZMQ often use hex with optional `0x`; one place to strip and decode keeps behavior consistent. |
| **Errors** | `UtilsError` (e.g. empty preimage, invalid hex) | Typed errors at the library boundary; no `anyhow` in the SDK surface. |

**Dependencies:** `bitcoin` (for `Encodable` on `Vec<u8>`), `sha2`, `hex`. No transport, no framing, no semantics.

**Why a separate crate:** So that **framing** (vote encode/decode) and **semantics** (domain id, progress) can both use the same domain hash and hex logic without depending on each other. Utils is the leaf of the dependency tree (except for `bitcoin`).

---

## 2. char-framing - Vote and Precommit Wire Formats

**Role:** Canonical encode/decode for **referendum vote and precommit bytes** on the framing wire (referendum vote **`data`** vector layout per node `SERIALIZE_METHODS`, DA, etc.). Bytes in, bytes out; no I/O, no protocol decisions (no "when to submit", no leader check).

**Why it exists:** On the stream, `ReferendumVote` serializes **`data` only** (CompactSize + payload per `referendum_vote.h`); ballot is out-of-band. **`char-framing`** implements that binary layout for tests and adapters. **JSON-RPC is different:** `addreferendumvote` values and `getreferendumdecisionroll` → `decision_roll.data` are **hex of raw payload bytes only** (no CompactSize in the hex); ballot comes from the node's pending ballot / `ballot_number` on the roll entry (`src/rpc/char_rpc.cpp`). Semantics uses **`char_utils`** (`bytes_to_hex` / `hex_to_bytes`) for those RPC strings and **`char-framing`** when you need the same **vector serialization** as the node for the vote body.

**What's in it:**

| Area | Contents | Rationale |
|------|----------|-----------|
| **Referendum vote** | `encode_referendum_vote`, `encode_referendum_vote_hex`, `decode_referendum_vote`, `decode_referendum_vote_hex`, `REFERENDUM_VOTE_LEAF_TYPE` | Matches `referendum_vote.h` **`data`** serialization: **CompactSize(len) \|\| payload**. `REFERENDUM_VOTE_LEAF_TYPE` is the `LeafType` discriminant (0), not the first byte of this encoding when the payload is non-empty. **Not** the RPC vote hex string: use `char_utils::bytes_to_hex` / `hex_to_bytes` for `addreferendumvote` and `decision_roll.data`. |
| **Errors** | `FramingError` (decode/encode failures) | Decoding can fail (truncation, wrong leaf type); encoding is infallible for valid inputs. |

**Dependencies:** `char-utils` (hex, for vote hex APIs). No transport, no semantics.

**Why a separate crate:** So that **transport** stays "dumb" (JSON and raw ZMQ bytes), and any code that needs the **same binary layout as the node's `ReferendumVote::data` serialization** has one implementation. RPC vote strings are payload-only hex; semantics uses **utils** for those and **framing** for the vector-encoded vote body when needed.

---

## 3. char-transport - RPC and ZMQ Abstractions (Wire Only)

**Role:** Typed RPC client trait and ZMQ subscription primitives. **Wire-shaped types only** - the shapes of JSON-RPC requests/responses and ZMQ messages. No domain hashing, no vote encode/decode, no progress, no leader logic, no retry policy.

**Why it exists:** The SDK must talk to a Char node over HTTP (JSON-RPC) and optionally ZMQ. That I/O and connection/retry behavior should live in one place. By exposing a **trait** (`CharRpcTransport`) and concrete types (`DomainInfo`, `DecisionRollEntry`, etc.), semantics can be written against an abstraction and tested with a mock, and the actual implementation can be swapped (e.g. bitcoind-async-client with `raw_rpc`).

**What's in it:**

| Area | Contents | Rationale |
|------|----------|-----------|
| **RPC trait** | `CharRpcTransport`: `get_domain_info`, `get_referendum_decision_roll`, `add_referendum_vote`, `get_leader_for_slot_current_block`, `get_all_char_bonds`, `domain_registry_schedule` | One async trait that semantics calls; implementation is currently **only** `BitcoindAsyncTransport` (Alpen's bitcoind-async-client with `raw_rpc` for connection + retry). |
| **RPC types** | `DomainInfo`, `DecisionRollEntry`, `DecisionRollWire`, `KeyRange`, `LeaderSlotEntry`, `BondInfo`, `AddReferendumVoteMode`, etc. | Serde (de)serializable structs that match the node's JSON-RPC. Transport returns these; semantics consumes them. |
| **ZMQ** | `ZmqAddress`, `ZmqMessage` (topic, body, sequence), `ZmqSubscriber` | Raw subscription: connect, subscribe by topic, receive multipart messages. No parsing of body contents (that's semantics' job, using framing when needed). |
| **Errors** | `TransportError`: `Network`, `Deserialization`, `Rpc { code, message }`, `Timeout` | Failures at the wire; semantics maps these to retry/terminal via `classify_transport_error`. |

**Dependencies:** `bitcoind-async-client` (git, `raw_rpc`), `async-trait`, `serde`, `serde_json`. No framing, no utils, no semantics.

**Why a separate crate:** So that **semantics** does not depend on a specific HTTP client or ZMQ library. Semantics depends on `CharRpcTransport` and the RPC/ZMQ types; the real implementation (bitcoind-async-client) lives in transport. Tests use a mock transport. This keeps protocol logic independent of I/O and makes it easy to add another backend later if needed (without touching semantics).

---

## 4. char-semantics - Char Protocol Logic

**Role:** The **rules and state** that implement "how Char works" from the client's perspective. Uses transport (RPC/ZMQ), utils (domain hash, hex for RPC payloads), and framing (binary vote **`data`** layout when needed outside the RPC payload-only hex contract).

**Why it exists:** Integrators need a single place that knows: what a domain is, what "pending ballot" and "leader" mean, when to submit, when to reconcile after ZMQ, how to classify errors for retry, and what progress/verified/gaps mean. Putting that in one crate keeps protocol behavior consistent and testable (with mock transport and optional mock ZMQ).

**What's in it (high level):**

| Area | Contents | Rationale |
|------|----------|-----------|
| **Domain** | `DomainId`, `DomainError` | Domain identity (from preimage hex); uses utils' domain hash. |
| **Progress** | `Progress`, `ProgressError` | Verified vs observed; rollback/gaps as errors. |
| **Ballot** | `pending_ballot`, `PendingBallotInfo`, `PendingBallotError` | Next ballot and leader bond from transport's `get_domain_info`; caller `bond_id_hex` must match `next_leader_bond`. |
| **Leader** | `check_leader`, `LeaderCheck` | Is my bond the leader for this ballot? Uses transport's `get_leader_for_slot_current_block`. |
| **Sync / reconcile** | `reconcile`, `ReconcileRequest`, `ReconcileResult`, `VerifiedRoll`, `next_decision_roll_event`, `process_zmq_decision_roll_message`, `DecisionRollStreamEvent`, `GapReason` | When to trust ZMQ vs when to force RPC reconciliation; stream events and gap handling. Fetches rolls via RPC; stores **`decision_roll.data`** as decoded **raw payload bytes**; ballot is `VerifiedRoll.ballot` / `DecisionRollEntry.ballot_number`. |
| **Submit** | `submit_vote`, `SubmitRequest`, `SubmitResult`, `ReadAfterWriteConfig`, `RejectReason` | Hex-encodes **payload only** for `add_referendum_vote`; read-after-write compares RPC `data` to the submitted payload. Idempotency and retry classification live here. |
| **Retry / errors** | `classify_transport_error`, `classify_semantics_error`, `RetryClass`, `SemanticsConfig` (timeouts, backoff, retry budget) | Map transport and semantics errors to retry vs terminal; config for timeouts and concurrency. |

**Dependencies:** `char-transport`, `char-framing`, `char-utils`, `async-trait`, `tokio`, `thiserror`. Semantics is the only crate that ties transport, framing, and utils together.

**Why a separate crate:** So that **all protocol decisions** live in one place. Transport stays "dumb" (just RPC and ZMQ shapes); framing stays "dumb" (binary vote body / precommit bytes only); utils stays "dumb" (just hash and hex). Semantics ties them together: e.g. derive pending ballot from `get_domain_info`, `check_leader`, then **`bytes_to_hex(payload)`** for RPC submit and **`hex_to_bytes`** on `decision_roll.data` when verifying rolls.

---

## 5. char-sdk - The Single Entry Point (Facade)

**Role:** The **only** crate integrators depend on. It contains no new logic; it only **re-exports** the public API from the four internal crates.

**Why it exists:** SDKSPEC requires that integrators and adapters depend **only** on `char-sdk`. They get `char_sdk::DomainId`, `char_sdk::Progress`, `char_sdk::CharRpcTransport`, `char_sdk::bytes_to_hex`, `char_sdk::encode_referendum_vote` (binary `data` layout), etc., without knowing or caring which internal crate defined them. This keeps the public surface stable and allows the internal crates to be reorganized or split without breaking integrators.

**What's in it:** Re-exports from:

- **char-semantics:** domain, progress, leader, reconcile, submit, streaming events, retry classification, config, errors.
- **char-transport:** `CharRpcTransport`, RPC types (`DomainInfo`, `DecisionRollEntry`, ...), ZMQ types, `TransportError`.
- **char-framing:** vote encode/decode, `FramingError`, `REFERENDUM_VOTE_LEAF_TYPE`.
- **char-utils:** domain hash, hex helpers, `UtilsError`.

**Dependencies:** `char-transport`, `char-semantics`, `char-framing`, `char-utils`. No additional runtime deps.

**Why a separate crate:** So that the "four packages" remain **internal**. Integrators get one dependency and one namespace; the layering (utils -> framing/transport -> semantics -> sdk) is an implementation detail.

---

## Summary Table

| Crate | Role | Deps (internal) | Why separate |
|-------|------|-----------------|--------------|
| **char-utils** | Domain hash, hex; no I/O, no protocol | (none) | Single canonical place for hash/hex; used by framing and semantics. |
| **char-framing** | Vote (and later precommit) wire encode/decode | utils | Single place for wire layout; transport and semantics stay format-agnostic. |
| **char-transport** | RPC trait + types, ZMQ types; bitcoind-async-client only | (none) | I/O and wire shapes only; semantics and tests use trait + mock. |
| **char-semantics** | Protocol logic: domain, progress, sync, submit, retry, gaps | transport, framing, utils | All "how Char works" in one place; transport/framing/utils stay dumb. |
| **char-sdk** | Re-exports only; single entry for integrators | transport, semantics, framing, utils | Integrators depend on one crate; internal split is hidden. |

This matches **SDKSPEC Section 2.1-2.3**: transport = wire; framing = referendum vote **`data`** vector + precommit bytes; utils = shared helpers (including RPC payload hex); semantics = protocol logic that ties them together; char-sdk = the SDK.
