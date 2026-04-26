# Char Rust SDK - Draft Specification

**Status:** Draft for contractor engagement. Aligns with POCs (hello-char-demo, strata-client char_sync) and stated requirements. Protocol/RPC/ZMQ are pinned; SDK must not depend on protocol changes.

**References (paths from repo root):**
- Hello-char-demo: `contrib/char/examples/hello-char-demo` (CharRpc, vote encode/decode, ZMQ decode, catch-up, leader submit)
- Strata char_sync: `bin/strata-client/src/char_sync.rs` (SyncClient impl, get_blocks from decision rolls, submit path)
- Char node: `src/rpc/char_rpc.cpp`, `src/char/`, `doc/char/site/`

---

## 1. Objectives & Constraints

- **Deliverable:** Production-grade Rust SDK as the integration core for Char; competitive with OP Stack / Arbitrum / Astria / Espresso / EigenLayer AVS sequencing integration quality.
- **First integration target:** TXLS (Alpen Strata). Alpen-specific logic must live only in an adapter crate; the SDK is integration-agnostic.
- **Protocol scope:** No Char protocol changes. SDK targets pinned RPC/ZMQ interfaces; any node-side changes are Client's responsibility and do not reduce SDK delivery obligations.

### 1.1 Where the SDK lives

The SDK lives **in this repository** under **`contrib/char/sdk/`**. That directory is the SDK root: it contains this spec, the Cargo workspace (crates, adapters, examples, char-mock, char-conformance), and any SDK-specific docs. The Char node (C++) remains in `src/char/`, `src/rpc/char_rpc.cpp`, etc.; conformance runs against the node built from this same repo.

---

## 2. What "the SDK" Is and How It's Packaged

**The SDK is the entire deliverable** - one product that integrators use. There is no separate "internal" SDK; everything under the repo is the SDK, with a single public API surface.

**Packaging options (either is fine):**

- **Option A - Single crate:** One crate (e.g. `char-sdk`) that integrators depend on. Its *internal* structure is just modules (e.g. `transport`, `semantics`, `framing`, `utils`). Adapters and tests live in the same repo (e.g. `adapters/`, `examples/`, `tests/`).
- **Option B - Workspace with one entry point:** Multiple crates for separation of concerns, but **the only crate integrators depend on** is the main one (e.g. `char-sdk`). That crate re-exports the public API; the others (`char-transport`, `char-semantics`, `char-framing`, `char-utils`) are implementation details. Adapters depend on `char-sdk` only.

So: "char-sdk" in the layout below means **the one library crate that exposes the SDK**. It is not a second layer on top of "the real SDK" - it *is* the SDK. The other crates (if used) are internal structure only.

### 2.1 Suggested Layout (workspace variant)

```
contrib/char/sdk/           # SDK root (in char-bitcoin repo); "the SDK" = this tree
|---- Cargo.toml              # Workspace
|---- SDKSPEC.md              # This spec
|---- crates/
|   |---- char-sdk/            # THE SDK - only crate integrators depend on
|   |   `---- src/             # Re-exports from transport + semantics + framing + utils
|   |---- char-transport/      # Internal: RPC + ZMQ types and traits
|   |---- char-semantics/      # Internal: Char protocol logic (domain, progress, sync, submit, retry, gaps)
|   |---- char-framing/        # Internal: vote + precommit wire formats (encode/decode per SOW)
|   `---- char-utils/          # Internal: shared utilities (domain hash, hex, helpers)
|---- char-mock/               # Mock RPC/ZMQ (for tests; not published to integrators)
|---- char-conformance/        # Conformance harness vs pinned node
|---- examples/
|   `---- hello-char/          # Generic demo (non-Alpen)
`---- adapters/
    `---- char-adapter-alpen/  # Alpen adapter; depends only on char-sdk
```

**Dependency rule:** Integrators and adapters depend **only** on `char-sdk`. They never depend on `char-transport`, `char-semantics`, `char-framing`, or `char-utils` directly. Semantics may depend on transport, framing, and utils. Framing and utils do not depend on any integration (e.g. Alpen).

### 2.2 What is "semantics", "framing", "utils", and "transport"?

- **Transport (`char-transport`)**  
  Typed RPC client and ZMQ subscription primitives. Wire-shaped types only; no Char protocol logic.

- **Framing (`char-framing`)**  
  **Vote framing** and **precommit framing** per the SOW: canonical encode/decode for the two **byte-level** wire formats.
  1. **Referendum vote (binary `data` field):** `CompactSize(payload_len) || payload_bytes` — matches in-node `ReferendumVote::SERIALIZE_METHODS` (`READWRITE(data)` only). Ballot is out-of-band. **JSON-RPC uses a different contract:** `addreferendumvote` sends **hex of payload bytes only**; `getreferendumdecisionroll` exposes **`ballot_number` on each entry** and `decision_roll.data` as **hex of the winning payload only** (`AttestationPayloadHex` in `src/rpc/char_rpc.cpp`). Semantics uses **`char-utils`** hex helpers for those RPC strings and **`char-framing`** when the vector-encoded vote body is required.
  2. **Precommit:** versioned record for publishing to Bitcoin/DA (OP_RETURN, etc.).  
  Pure bytes in/out; no I/O, no protocol decisions.

- **Utils (`char-utils`)**  
  Shared **utilities** used by semantics and/or framing: e.g. domain hash (SHA256(CompactSize||preimage)), hex helpers, varint/compact-size if not folded into framing, other helpers. No Char protocol logic.

- **Semantics (`char-semantics`)**  
  **Char protocol logic** - the rules and state that implement "how Char works" from the client's perspective:
  - **Domain:** what a domain id is (uses utils for hash).
  - **Progress:** what "verified" vs "observed" means, and that rollback/gaps are errors.
  - **Ballot lifecycle:** what "pending ballot" is, when to submit, when a ballot is decided.
  - **Leader verification:** whether the local bond is leader before submitting.
  - **Sync/reconcile:** when to treat ZMQ as advisory, when to force RPC reconciliation, when to require reset after a gap.
  - **Submit result:** accepted vs verified vs rejected, idempotency, retry vs terminal errors.  
  Semantics uses **transport** (RPC/ZMQ), **utils** (domain hash, hex for RPC payloads), and **framing** (binary vote **`data`** / precommit bytes where the RPC contract does not apply).

So: **transport** = wire; **framing** = vote + precommit bytes; **utils** = shared helpers; **semantics** = protocol logic that ties them together.

### 2.3 Where do models, events, and errors go?

| Kind | Lives in | Re-exported by char-sdk? | Examples |
|------|-----------|---------------------------|----------|
| **Models (wire-shaped)** | `char-transport` | Yes (if integrator implements transport) | `DomainInfo`, `DecisionRollEntry`, `SlotSelection`, `BondInfo` - RPC request/response and ZMQ body shapes. |
| **Models (semantic)** | `char-semantics` | Yes | `DomainId`, `Progress`, `VerifiedRoll`, `ReconcileRequest`/`ReconcileResult`, `SubmitRequest`/`SubmitResult`, `LeaderCheck` - protocol concepts the SDK exposes. |
| **Models (framing)** | `char-framing` | Only if needed for precommit/DA | `Precommit`, `CommitmentRecord`, `PayloadRef` - structs for vote/precommit encoded format; semantics calls framing, most integrators don't touch these directly. |
| **Events** | `char-semantics` | Yes | `DecisionRollStreamEvent`, `DecisionRollEventKind`, `GapReason` - stream/gap events from sync; semantics owns the contract. |
| **Errors** | Per layer; semantics wraps | Yes (unified surface) | `TransportError` (transport), `UtilsError` (utils), `FramingError` (framing), `SemanticsError` (semantics; can wrap the others). Public API exposes `SemanticsError` and optionally `TransportError` for transport implementors. |

**Rule of thumb:** Wire-shaped -> **transport**. Protocol concepts and events -> **semantics**. Vote/precommit encode/decode and their structs -> **framing**. Shared helpers (domain hash, hex, etc.) -> **utils**. The **char-sdk** crate re-exports the types integrators need so they get `char_sdk::DomainId`, `char_sdk::Progress`, `char_sdk::SemanticsError`, `char_sdk::DecisionRollStreamEvent`, etc., without caring which internal crate defined them.

**Structured errors:** Avoid stringly-typed error payloads in public enums. Use finite variants (`HexParseError`, `FramingError`, etc.), and use `ShortDiag` only where a truncated human-readable diagnostic is needed at the boundary (transport deserialization/RPC message, `HandlerDenied`, etc.).

---

## 3. Transport Layer (`char-transport`)

**Responsibility:** Typed RPC client and ZMQ subscription primitives only. No leader logic, ballot lifecycle, idempotency, retry classification, or domain scheduling.

**What's in the package (breakdown):**

| Category | Contents |
|----------|----------|
| **RPC trait** | `CharRpcTransport` - async methods for each Char RPC used by the SDK. Implemented by the real HTTP client or by char-mock. |
| **RPC request/response types** | Wire-shaped structs for requests and responses: `DomainInfo`, `DecisionRollEntry`, `DecisionRollWire`, `SlotSelection`, `LeaderSlotResult`, `KeyRange`, `BondInfo`, `AddReferendumVoteMode`. All serde (de)serializable to match JSON-RPC. |
| **ZMQ types** | `ZmqAddress`, `ZmqMessage` (topic, body, sequence), trait `ZmqSubscriber`, and an implementation (e.g. `ZmqSubSocket`) that connects, subscribes to a topic, and delivers raw multipart messages. No parsing of body contents. |
| **Errors** | `TransportError`: `Network`, `Deserialization(ShortDiag)`, `Rpc { code, message: ShortDiag }`, `Timeout`, `ZmqMultipart(ZmqMultipartFormatError)` (wrong frame count or sequence length vs Core). `ShortDiag` is a fixed-capacity UTF-8 diagnostic for display/logging only. No `anyhow` at the library boundary. |
| **Not in transport** | No domain hashing, no vote encode/decode, no progress, no leader check, no retry logic, no idempotency - only "call this RPC / subscribe to this ZMQ topic and return the raw result." |

### 3.1 RPC Client

- **Base:** May be built on top of a fork or wrapper of `bitcoind-async-client` (or equivalent) to keep a single HTTP/JSON-RPC abstraction. The SDK must not hardcode raw method strings in the semantics layer; transport exposes **typed methods** with explicit request/response types.

**Required RPC methods (typed):**

| Method | Request | Response | Notes |
|--------|---------|----------|--------|
| `getdomaininfo` | `domain_preimage_hex: String` | `DomainInfo` | next_ballot, next_leader_bond, is_next_leader_mine, latest decided ballot + roll hashes (see types) |
| `getreferendumdecisionroll` | domain, start_ballot, end_ballot, verbosity | `Vec<DecisionRollEntry>` | verbosity 0/1/2; each entry has **`ballot_number`**. When present, **`decision_roll.data`** is **hex of the winning vote payload only** (not a self-contained leaf encoding ballot inside `data`). |
| `addreferendumvote` | `[(domain_preimage_hex, payload_hex)], mode?` | `HashMap<String, bool>` | Value per domain is **hex-encoded payload bytes**; node resolves **pending ballot** and stores `ReferendumVote{ballot, payload}`. per-domain success |
| `get_leader_for_slot_current_block` | key_ranges (domain + start_slot, end_slot) | `Vec<LeaderSlotEntry>` | one entry per key range: key, blockhash, selections (slot, bond) |
| `getallcharbonds` | verbosity | `Vec<BondInfo>` | txid, issuer, amount, closed, attestations? |
| `domain_registry` | subcommand, args | registry result | schedule/unschedule/list; used by node ops, not required for sync/submit core |

**Types (transport layer - wire-shaped):**

```rust
// char-transport/src/rpc/types.rs

/// Response of `getdomaininfo`: next ballot, leader bond for that ballot, and latest decided roll summary.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct DomainInfo {
    pub next_ballot: u64,
    pub next_leader_bond: bitcoin::Txid,
    pub is_next_leader_mine: bool,
    pub latest_decided_ballot: Option<u64>,
    pub latest_decision_roll_hash: String,
    pub latest_decision_data_hash: String,
    pub latest_decision_zeitgeist: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct DecisionRollEntry {
    pub domain_hash: String,
    pub ballot_number: u64,
    pub found: bool,
    #[serde(default)]
    pub decision_roll: Option<DecisionRollWire>,
}

/// When `found` is true, the node always sends roll_hash, data_hash, serialized.
/// attestation_hash and data appear at verbosity >= 1; proofs at verbosity >= 2.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct DecisionRollWire {
    pub roll_hash: Option<String>,
    pub data_hash: Option<String>,
    pub serialized: Option<String>,
    pub data: Option<String>,
    pub attestation_hash: Option<String>,
    pub proofs: Option<Vec<String>>,
}

/// One selection: bond (txid hex) for a given slot. Part of `LeaderSlotEntry::selections`.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct SlotSelection {
    pub slot: u64,
    pub bond: String,
}

/// One entry in the response of `get_leader_for_slot_current_block` (one per key range in the request).
#[derive(Debug, Clone, serde::Deserialize)]
pub struct LeaderSlotEntry {
    pub key: String,
    pub blockhash: String,
    pub selections: Vec<SlotSelection>,
}

/// Request item for `get_leader_for_slot_current_block`. Serialize for RPC.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct KeyRange {
    pub key: String,
    pub start_slot: u64,
    pub end_slot: u64,
}

/// One bond in the response of `getallcharbonds`. verbosity 0 = wallet bonds only; 1 = all seen.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct BondInfo {
    pub txid: String,
    /// Hex of bond key (issuer); may be empty string if anchor view unavailable.
    pub issuer: String,
    /// Bitcoin amount string (e.g. "0.10000000").
    pub amount: String,
    /// True if quickbreak output is spent (bond closed).
    pub closed: bool,
    /// Present when attestation chain stats are available for this bond.
    pub attestations: Option<BondAttestationsInfo>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct BondAttestationsInfo {
    pub height: u64,
    pub genesis_char_hash: String,
}
```

**Trait (transport only - no semantics):**

```rust
// char-transport/src/lib.rs

#[async_trait::async_trait]
pub trait CharRpcTransport: Send + Sync {
    async fn get_domain_info(&self, domain_preimage_hex: &str) -> Result<DomainInfo, TransportError>;
    async fn get_referendum_decision_roll(
        &self,
        domain_preimage_hex: &str,
        start_ballot: u64,
        end_ballot: u64,
        verbosity: u8,
    ) -> Result<Vec<DecisionRollEntry>, TransportError>;
    async fn add_referendum_vote(
        &self,
        votes: &[(String, String)],
        mode: Option<AddReferendumVoteMode>,
    ) -> Result<HashMap<String, bool>, TransportError>;
    async fn get_leader_for_slot_current_block(
        &self,
        key_ranges: &[KeyRange],
    ) -> Result<Vec<LeaderSlotEntry>, TransportError>;
    async fn get_all_char_bonds(&self, verbosity: u8) -> Result<Vec<BondInfo>, TransportError>;
}

#[derive(Debug, Clone, Copy, Default)]
pub enum AddReferendumVoteMode {
    #[default]
    IsLeader,
    Init,
    PlzFind,
}
```

**Transport errors:** Typed, no `anyhow` at library boundary.

```rust
// char-transport/src/error.rs (uses char_utils::ShortDiag)

#[derive(Debug)]
pub enum TransportError {
    Network(Box<dyn std::error::Error + Send + Sync>),
    /// Bounded diagnostic only (not an arbitrary `String` payload).
    Deserialization(ShortDiag),
    Rpc { code: i32, message: ShortDiag },
    Timeout,
    ZmqMultipart(ZmqMultipartFormatError),
}
```

### 3.2 ZMQ Subscription

- **Responsibility:** Connect, subscribe to topics, deliver raw multipart messages. No parsing of leader/decisionroll semantics; that lives in semantics layer.

```rust
// char-transport/src/zmq.rs

pub type ZmqAddress = String; // "tcp://127.0.0.1:28332" or "ipc://..."

#[derive(Debug, Clone)]
pub struct ZmqMessage {
    pub topic: Vec<u8>,
    pub body: Vec<u8>,
    pub sequence: [u8; 4], // Core: 4-byte LE uint32 after topic + body
}

#[async_trait::async_trait]
pub trait ZmqSubscriber: Send + Sync {
    fn topic(&self) -> &str;
    async fn recv(&mut self) -> Result<ZmqMessage, TransportError>;
}

pub struct ZmqSubSocket {
    // implementation wraps zeromq or zmq crate
}

impl ZmqSubscriber for ZmqSubSocket { ... }
```

- **Topics used by semantics (names only in transport):** `"leader"`, `"decisionroll"`. Transport does not interpret body.

---

## 4. Char Semantics Layer (`char-semantics`) - Core Deliverable

**Encapsulates:** Domain abstraction, progress model, decision roll streaming, reset/reconciliation, ballot lifecycle, leader verification, **RPC payload hex** via `char-utils` and **referendum vote `data`** encode/decode via `char-framing` when needed, idempotency, retry classification, fault detection. Must be integration-agnostic and contain no Alpen/Strata-specific logic.

**Integrator hooks:** `CharBallotHandlers` is an `#[async_trait]` trait (`async fn produce_payload`, `async fn on_roll_observed(ObservedRoll)`); `run_rpc` and `run_zmq` `.await` those calls so app logic can use async I/O without blocking Tokio. `char-sdk` re-exports `async_trait` for implementors.

### 4.1 Domain Abstraction

```rust
// char-semantics/src/domain.rs

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct DomainId(pub [u8; 32]);

impl DomainId {
    /// Canonical: SHA256(CompactSize(len(preimage)) || preimage) per Char node.
    pub fn from_preimage(preimage: &[u8]) -> Self;
    pub fn from_preimage_hex(hex: &str) -> Result<Self, DomainError>;
}

// `impl fmt::Display`: lowercase hex (64 chars). Use `id.to_string()` / `format!("{id}")`.

#[derive(Debug, thiserror::Error)]
pub enum DomainError {
    #[error("domain preimage empty")]
    EmptyPreimage,
    #[error("domain preimage hex: {0}")]
    InvalidHex(#[from] char_utils::HexParseError),
    #[error("domain preimage decoded to empty bytes")]
    PreimageBytesEmpty,
}
```

- All semantics APIs that need a "domain" accept `DomainId` or a type that can be derived from it (e.g. preimage hex for RPC). No hardcoded domain strings in semantics.

### 4.2 Progress Model (Explicit)

- **Observed progress:** Highest ballot number for which the SDK has **seen** a decision roll (e.g. via ZMQ or RPC), not yet necessarily verified or committed.
- **Verified progress:** Highest ballot number for which the SDK has **verified** the roll (PreVerify + optional ContextualVerify) and the consumer has **committed** (e.g. persisted).
- **Finality progress (optional):** If the node or SDK can express Bitcoin confirmation depth for the roll's Zeitgeist block, expose it; otherwise optional.

**Invariant:** Rollback or gap must never be silently tolerated. Any regression or gap must produce an explicit error and require reset/reconciliation before continuing.

```rust
// char-semantics/src/progress.rs

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    /// Last ballot for which we have verified and committed outcome.
    pub confirmed_ballot: Option<u64>,
    /// Optional: last ballot we've observed (e.g. from ZMQ) not yet verified.
    pub observed_ballot: Option<u64>,
    /// Optional: last ballot considered final (e.g. by L1 confirmations).
    pub finality_ballot: Option<u64>,
}

impl Progress {
    pub fn next_ballot_to_verify(&self) -> u64;
    pub fn advance_verified(&mut self, ballot: u64) -> Result<(), ProgressError>;
}

#[derive(Debug, thiserror::Error)]
pub enum ProgressError {
    #[error("rollback or gap: expected next ballot {expected}, got {got}")]
    RollbackOrGap { expected: u64, got: u64 },
    #[error("ballot overflow")]
    Overflow,
}
```

### 4.3 Decision Roll Streaming & Event Model

- ZMQ is **best-effort advisory**. The SDK must:
  - Detect loss or sequence discontinuity (e.g. missing ballot, reorder, duplicate).
  - Emit **explicit gap/error** events.
  - **Force RPC reconciliation** when a gap or error is detected.
  - **Require explicit reset** before continuing (no silent continuation after failure).

```rust
// char-semantics/src/streaming.rs

pub struct DecisionRollStreamEvent {
    pub domain: DomainId,
    pub ballot: u64,
    pub kind: DecisionRollEventKind,
}

pub enum DecisionRollEventKind {
    /// Raw roll data received (e.g. from ZMQ); not yet verified.
    Observed { serialized: Vec<u8>, payload: Vec<u8>, tag: u8 },
    /// Gap or invalid sequence detected; caller must reconcile and reset.
    Gap(GapReason),
}

pub enum GapReason {
    DuplicateBallot { ballot: u64 },
    MissingBallot { expected: u64 },
    SequenceDiscontinuity { expected: u64, got: u64 },
    ParseError(DecisionRollParseError),
}

pub enum DecisionRollParseError {
    BodyTooShort,
    DomainSlice,
    TagCompactSize,
}
```

- **Sync driver:** Consumes `CharRpcTransport` + optional `ZmqSubscriber`; on ZMQ message, parses and emits `DecisionRollStreamEvent`; on `Gap`, sets internal state to "requires_reset" and returns error; caller must call `reconcile_via_rpc` and then `reset` before processing further.

### 4.4 Reset and Reconciliation

```rust
// char-semantics/src/reconcile.rs

pub struct ReconcileRequest {
    pub domain: DomainId,
    pub from_ballot: u64,
    pub to_ballot: u64,
    pub max_fetch: u64,
}

pub struct ReconcileResult {
    pub rolls: Vec<VerifiedRoll>,
    pub next_ballot: u64,
    pub gap_detected: bool,
}

pub struct VerifiedRoll {
    pub ballot: u64,
    pub payload: Option<Vec<u8>>,
    pub serialized_roll: Vec<u8>,
    pub roll_hash: bitcoin::hashes::sha256::Hash, // re-exported as `RollHash` from `char-sdk`
    pub data_hash: Option<bitcoin::hashes::sha256::Hash>,
    pub block_hash: bitcoin::BlockHash, // placeholder until wired from RPC
}

/// After a gap or startup: fetch rolls via RPC, verify in order, return verified list.
/// Caller persists verified progress; SDK does not persist.
pub async fn reconcile(
    transport: &impl CharRpcTransport,
    domain_preimage_hex: &str,
    request: ReconcileRequest,
) -> Result<ReconcileResult, SemanticsError>;
```

### 4.5 Ballot Lifecycle (Semantic Model)

- **Pending ballot:** For (domain, bond), the next ballot number the leader may propose. From `getdomaininfo` (`next_ballot`); `bond_id` must match `next_leader_bond` (semantics `pending_ballot()` enforces this).
- **Proposal:** Leader submits a vote for that ballot via `addreferendumvote` with **payload hex** (`char-utils::bytes_to_hex`); the node attaches the **pending ballot**. Full-leaf encoding (`char-framing`) applies to **Bamboo** bytes, not this RPC string.
- **Decided:** Tabulation produces a DecisionRoll or ImpossibleRoll; node stores and may push via ZMQ. Consumer verifies and commits.
- **Next:** Progress advances; next pending ballot = next_ballot_to_verify (for sync) or next pending_ballot (for submit).

The semantics layer exposes:
- `pending_ballot(transport, domain, bond_id) -> Result<PendingBallotInfo, SemanticsError>` (uses `get_domain_info`; `bond_id` must match `next_leader_bond` or `PendingBallotError::BondNotNextLeader`)
- `next_ballot_to_verify(progress: &Progress) -> u64`
- No internal state machine for "current ballot" that isn't derived from explicit progress + RPC.

### 4.6 Leader Verification

- **Leader** for (block_hash, domain, ballot) is determined by the node (stake-weighted sampling). The SDK does not recompute leader; it **verifies** that the local bond is the leader before submitting (when leader verification is enabled).
- **Verification:** Call `get_leader_for_slot_current_block` or use `getdomaininfo.is_next_leader_mine` (via `pending_ballot` / `PendingBallotInfo`); semantics layer exposes:

```rust
// char-semantics/src/leader.rs

pub struct LeaderCheck {
    pub ballot: u64,
    /// `Some` when RPC returned a selection for `ballot`; bond is the leader's `Txid`.
    pub leader_bond_id: Option<bitcoin::Txid>,
    pub is_mine: bool,
}

pub async fn check_leader(
    transport: &impl CharRpcTransport,
    domain_preimage_hex: &str,
    ballot: u64,
    my_bond_id_hex: &str,
) -> Result<LeaderCheck, SemanticsError>;
```

- Submit path (see below) **enforces leader verification by default** (configurable off for plzfind or tests).

### 4.7 Referendum vote: RPC contract vs binary `data` layout (`char-utils` + `char-framing`)

- **RPC (what semantics uses for submit and roll verification):**
  - **`addreferendumvote`:** each domain maps to **hex of `payload` bytes only**; ballot is chosen by the node (`GetPendingBallotNumber` + insert). Use **`char_utils::bytes_to_hex`** (and `hex_to_bytes` / `strip_0x_prefix` when decoding RPC hex).
  - **`getreferendumdecisionroll`:** use **`entry.ballot_number`** for the ballot; **`decision_roll.data`** (when present) is **hex of the winning payload only**, same as node's `HexStr(attestation.GetReferendumVote().GetData())`.
- **Binary `ReferendumVote` `data` field (Bamboo inner body / tools / tests):** **`char-framing`** matches `referendum_vote.h` stream serialization: **CompactSize(payload_len) || payload_bytes** only. Ballot is **not** encoded; the `ballot` argument on encode is ignored (compat). A full Bamboo **`WithLeafType`** value is **CompactSize(`LeafType`)** then this body; the SDK does not assemble that wrapper in these helpers.

```rust
// char-framing/src/vote.rs — binary vote body / hex (NOT the addreferendumvote RPC value string)

pub fn encode_referendum_vote(payload: &[u8]) -> Vec<u8>;
pub fn encode_referendum_vote_hex(payload: &[u8]) -> String;
pub fn decode_referendum_vote(bytes: &[u8]) -> Result<Vec<u8>, FramingError>;
pub fn decode_referendum_vote_hex(hex: &str) -> Result<Vec<u8>, FramingError>;
```

`encode_referendum_vote_hex` is lowercase hex of the **CompactSize + payload** bytes. For RPC submit, integrators should use **`bytes_to_hex(&payload)`** from `char-utils` (re-exported as `char_sdk::bytes_to_hex`).

`FramingError` is a finite enum (`HexParse(HexParseError)`, `UnexpectedLeafType`, `PayloadLengthOverflow`, `Io(ErrorKind)`, `ConsensusDecode`) - no unbounded `String` diagnostic fields.

### 4.8 Submit Model

- **Distinguish:** "accepted" (RPC returned success for addreferendumvote) vs "verified/observed" (subsequent read-after-write confirms the vote is reflected).
- **Leader verification:** On by default; configurable.
- **Idempotency:** Deterministic idempotency key (e.g. domain + ballot + payload_hash or domain + ballot + client-supplied key). Duplicate submission must be safe (same key -> same outcome).
- **Retry:** Classify retryable vs terminal; bounded retry with backoff (configurable budget).
- **Read-after-write:** After submit, optionally poll getreferendumdecisionroll (or getdomaininfo) until the submitted ballot is decided or timeout.

**Submit result (explicit):**

```rust
// char-semantics/src/submit.rs

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubmitResult {
    /// RPC accepted the vote; not yet verified by read-after-write.
    Submitted,
    /// RPC accepted and we observed/verified the outcome (e.g. via read-after-write).
    VerifiedObserved,
    /// RPC rejected (e.g. not leader, invalid format).
    Rejected(RejectReason),
    /// Unknown (e.g. timeout before confirmation).
    Unknown,
}

pub enum RejectReason {
    NotLeader,
    InvalidVoteFormat,
    /// RPC returned success but `false` for this domain (vote not applied).
    VoteNotAccepted,
}
```

**Submit API:**

```rust
pub struct SubmitRequest {
    pub domain_preimage_hex: String,
    pub ballot: u64,
    pub payload: Vec<u8>,
    pub idempotency_key: Option<[u8; 32]>,
    pub leader_verification: bool,
    pub read_after_write: ReadAfterWriteConfig,
}

pub struct ReadAfterWriteConfig {
    pub enabled: bool,
    pub max_wait: Duration,
    pub poll_interval: Duration,
}

pub async fn submit_vote(
    transport: &impl CharRpcTransport,
    request: SubmitRequest,
    bond_id_hex: &str,
) -> Result<SubmitResult, SemanticsError>;
```

- Internal: **`char_utils::bytes_to_hex`** on `request.payload` for the RPC map value; call transport `add_referendum_vote`; if rejected (e.g. not leader), return `Rejected`; if accepted and read_after_write enabled, poll **`get_referendum_decision_roll`**, decode **`decision_roll.data`** with **`hex_to_bytes(strip_0x_prefix(...))`**, compare bytes to **`request.payload`** (ballot is already **`request.ballot`** / query context), until decided or timeout → `Submitted`, `VerifiedObserved`, or `Unknown`.

### 4.9 Idempotency Enforcement

- **Key:** Deterministic from (domain, ballot, payload_hash) or provided by caller. Stored only in adapter or application; SDK exposes the key derivation and accepts optional key in `SubmitRequest`.
- **Behavior:** If the same key is used again, treat as duplicate; submit path may short-circuit and return `VerifiedObserved` if already observed, or `Submitted` if we only know RPC accepted earlier (no double RPC call that could confuse the node).

### 4.10 Retry Classification

```rust
// char-semantics/src/retry.rs

#[derive(Debug, Clone, Copy)]
pub enum RetryClass {
    Retryable,
    Terminal,
}

pub fn classify_transport_error(e: &TransportError) -> RetryClass;
pub fn classify_semantics_error(e: &SemanticsError) -> RetryClass;
```

- **Retryable:** Network timeouts, transient RPC errors, rate limits.
- **Terminal:** Not leader, invalid format, rollback/gap (must reset), deserialization errors.

### 4.11 Fault Detection

- **Gap:** Missing ballot in ZMQ stream or RPC scan.
- **Rollback:** Verified progress would decrease.
- **Parse/verify failure:** Invalid ZMQ body or roll bytes.
- All surface as typed errors or `DecisionRollEventKind::Gap`; no silent drop. Logging alone is not sufficient; caller must receive an error or event.

### 4.12 Semantics Errors (Typed, No anyhow at Boundary)

```rust
// char-semantics/src/error.rs

pub enum LeaderCheckError {
    NoEntry,
    InvalidBondTxid,
}

pub enum ReconcileError {
    HexDecode(HexParseError),
    RollHashLength,
    MissingDecisionRollWire,
    MissingRollHash,
    MissingRollVoteHex,
    VoteBallotMismatch { entry: u64, framed: u64 },
    HandlerDenied(ShortDiag),
}
#[derive(Debug, thiserror::Error)]
pub enum SemanticsError {
    #[error("transport: {0}")]
    Transport(#[from] TransportError),
    #[error("domain: {0}")]
    Domain(#[from] DomainError),
    #[error("progress: {0}")]
    Progress(#[from] ProgressError),
    #[error("framing: {0}")]
    Framing(#[from] FramingError),
    #[error("utils: {0}")]
    Utils(#[from] UtilsError),
    #[error("gap: {0}")]
    Gap(GapReason),
    #[error("leader check: {0}")]
    LeaderCheck(#[from] LeaderCheckError),
    #[error("submit rejected: {0}")]
    SubmitRejected(RejectReason),
    #[error("reconcile: {0}")]
    Reconcile(#[from] ReconcileError),
}
```

---

## 5. Precommit framing & Data Publication (`char-framing`)

- **Purpose:** Formal, versioned framing for Char-generated precommits suitable for publication to Bitcoin OP_RETURN, optional inscription-like or annex-style paths (hooks only if not in scope), and external DA (e.g. blob DA).

### 5.1 Requirements (from SOW)

- Versioned, canonically encoded, domain separation, height and ballot context, parent commitment, payload commitment, Decision Roll data, deterministic hashing.
- Canonical precommit hash must include domain, height, ballot, parent commitment, protocol version (no reuse across domain/height/chain/network).
- **Minimal commitment record:** Explicit max field sizes, max encoded length, versioned, checksum/integrity, Bitcoin size/policy limits.
- **Two-part model:** (1) Small on-chain commitment record (OP_RETURN / Inscription), (2) optional off-chain/DA payload; commitment record links to payload (e.g. payload hash).
- **Diff/proof style:** Support for incremental/diff and proof-style commitments.

### 5.2 Structs (Draft)

```rust
// char-framing/src/precommit.rs

pub const PRECOMMIT_VERSION: u8 = 1;
pub const MAX_COMMITMENT_RECORD_LEN: usize = 80; // example; match Bitcoin policy

#[derive(Debug, Clone)]
pub struct Precommit {
    pub version: u8,
    pub domain_id: DomainId,
    pub height: u64,
    pub ballot: u64,
    pub parent_commitment: [u8; 32],
    pub payload_commitment: [u8; 32],
    pub decision_roll_data: DecisionRollCommitment,
    pub payload_ref: Option<PayloadRef>,
}

pub struct DecisionRollCommitment {
    pub roll_hash: [u8; 32],
    pub block_hash: [u8; 32],
    // optional: minimal proof info for diff/proof style
}

pub struct PayloadRef {
    pub hash: [u8; 32],
    pub size: u64,
    pub chunks: Option<ChunkSpec>,
}

pub struct ChunkSpec {
    pub chunk_size: u64,
    pub num_chunks: u32,
}

impl Precommit {
    pub fn canonical_encode(&self) -> Vec<u8>;
    pub fn canonical_hash(&self) -> [u8; 32];
    pub fn commitment_record(&self) -> CommitmentRecord;
}

#[derive(Debug, Clone)]
pub struct CommitmentRecord {
    pub version: u8,
    pub precommit_hash: [u8; 32],
    pub payload_hash: [u8; 32],
    pub parent_hash: [u8; 32],
    pub checksum: [u8; 4],
}

impl CommitmentRecord {
    pub fn encode(&self) -> Vec<u8>;
    pub fn max_encoded_len() -> usize;
}
```

- **DataPublication trait (generic DA):**

```rust
// char-framing/src/da.rs

pub trait DataCommitment {
    fn publish_commitment(&self, record: &CommitmentRecord) -> Result<(), FramingError>;
}

pub trait PayloadPublisher {
    fn publish_payload(&self, precommit: &Precommit, payload: &[u8]) -> Result<(), FramingError>;
}
```

- Nice-to-have: Inscription-like 2-phase witness, annex-compatible hooks (interfaces only); external DA with chunking and size bounds.

---

## 6. Integration Adapters

### 6.1 Rules

- Use **only** public SDK APIs.
- Perform **payload deserialization only** (e.g. L2BlockBundle, hello-count); **zero** Char protocol logic (no leader logic, no ballot lifecycle, no vote encoding in adapter).
- Alpen adapter: depends on `char-sdk`; implements Strata's `SyncClient` and a submit path by calling SDK sync + submit; decodes L2BlockBundle from roll payload via Borsh/serde.

### 6.2 Alpen Adapter Sketch

- **Sync:** Use SDK `reconcile` + progress; implement `get_sync_status` / `get_blocks_range` by calling SDK's RPC-based sync and mapping `VerifiedRoll.payload` to `L2BlockBundle` via BorshDeserialize. No hardcoded domain in SDK; adapter supplies domain (from config).
- **Submit:** Use SDK `submit_vote` with leader verification and read-after-write; encode payload as `borsh::to_vec(&bundle)`; idempotency key from (domain, ballot) or (domain, ballot, block_id).

### 6.3 Generic Demo Adapter (Skeleton)

- Second adapter: e.g. "hello-char" style app (counter or opaque payload). Same SDK surface: domain from config, payload = app-specific bytes, no Alpen types. Proves that the SDK is integration-neutral.

---

## 7. Configuration & Stability

### 7.1 Structured Configuration

```rust
// char-semantics/src/config.rs

#[derive(Debug, Clone)]
pub struct SemanticsConfig {
    pub retry_budget: RetryBudget,
    pub timeouts: Timeouts,
    pub concurrency_limits: ConcurrencyLimits,
    pub buffer_bounds: BufferBounds,
}

pub struct RetryBudget {
    pub max_retries: u32,
    pub backoff: BackoffConfig,
}

pub struct Timeouts {
    pub rpc_call: Duration,
    pub read_after_write: Duration,
    pub reconcile_batch: Duration,
}

pub struct BufferBounds {
    pub max_roll_batch: u64,
    pub max_payload_size: usize,
}
```

- **Defaults:** `BufferBounds::default().max_payload_size` is `char_utils::MAX_CHAR_BAMBOO_SIZE` (same identifier and numeric value as `Char::constants::MAX_CHAR_BAMBOO_SIZE` in `src/char/util/constants.h`) so drift from the node limit is easy to spot.
- **Defaults:** `BufferBounds::default().max_roll_batch` is `char_utils::GET_REFERENDUM_DECISION_ROLL_MAX_INCLUSIVE_SPAN` (`GET_REFERENDUM_DECISION_ROLL_MAX_RANGE - 1`), matching the `getreferendumdecisionroll` span limit (`MAX_RANGE` in `src/rpc/char_rpc.cpp`).
- Logging/tracing: feature-gated hooks (e.g. `tracing`), no forced log format in library.

### 7.2 Stability Policy

- **SemVer:** SDK follows semantic versioning.
- **Startup compatibility:** SDK performs a startup check against the connected Char node (version/capabilities if exposed by RPC). If the node is outside the supported range, SDK **fail-fasts** with a clear error, unless the caller explicitly opts into an "unsafe/override" mode.

```rust
// char-transport or char-semantics

pub async fn check_node_compatibility(
    transport: &impl CharRpcTransport,
    allowed_versions: &str,
    unsafe_override: bool,
) -> Result<(), CompatibilityError>;
```

### 7.3 Machine-Readable Interface

- Deliver **OpenRPC spec** for all SDK-used RPC methods, or an equivalent machine-readable schema (e.g. JSON Schema for request/response types). This can be generated from the same types the transport uses.

---

## 8. Testing & Conformance

**Conformance** here means: checking that the SDK **actually works against a real Char node** (the one built from a pinned commit/version), not just against mocks. The SDK must *conform* to the node's RPC and ZMQ behavior - correct request/response shapes, correct handling of gaps and reset, correct submit and read-after-write. So the "conformance harness" is the test suite that talks to a live (or testnet/regtest) Char node and runs a defined scenario; passing it is required for acceptance (mock-only tests are not enough).

**char-mock vs char-conformance**

| | **char-mock** | **char-conformance** |
|---|----------------|----------------------|
| **What it is** | In-memory fake implementation of `CharRpcTransport` (and optionally ZMQ). No real node. | Test harness that runs tests against a **real** Char node (pinned build). |
| **Purpose** | Run semantics, transport, and integration tests **without** starting a node. Fast, CI-friendly, no external process. | Prove the SDK **conforms** to a real node: correct RPC/ZMQ usage, gap handling, submit flow. Required for acceptance. |
| **When it runs** | Unit tests, property tests, gap simulation (mock injects missing ballot), any test that doesn't need the node. | Conformance scenario(s): e.g. sync range -> inject gap -> reset/reconcile -> submit with read-after-write. |
| **Dependency** | Tests and semantics layer (for tests) depend on char-mock. | Harness depends on char-sdk; it drives the SDK against a node (regtest or similar). |

So: **mock** = fake node for fast tests; **conformance** = real node for "does the SDK actually work with Char?".

### 8.1 Test Categories

- **Unit tests:** Domain hashing, vote encode/decode, progress advance, retry classification, error mapping.
- **Property tests:** e.g. referendum vote roundtrip, progress ordering invariants.
- **Mock RPC tests:** `char-mock` provides in-memory implementation of `CharRpcTransport` and mock ZMQ; semantics tests run without a node.
- **Gap simulation:** Mock injects missing ballot or wrong order; SDK must emit gap error and require reset.
- **Fuzz:** `cargo-fuzz` target (e.g. vote decode, precommit decode); regression corpus checked in.
- **Conformance harness:** Runs against a **pinned Char node** build (commit/version and setup instructions in repo). At least one full vertical test: sync over a range -> inject event gap -> forced reset/reconciliation -> submit with read-after-write verification. Mock-only validation is not sufficient for acceptance.

### 8.2 Conformance Scenario (Minimum)

1. Start from clean state; sync domain from ballot 0 to N via RPC (or ZMQ + RPC reconciliation).
2. Inject a gap (e.g. mock drops one ZMQ message or returns skip in one ballot).
3. Detect gap; force reset; run reconciliation over the range; verify verified progress matches expected.
4. Submit a vote (leader) with read-after-write; assert result is `VerifiedObserved` or `Submitted` and no silent failure.

---

## 9. Milestone Mapping to Spec

| Milestone | Deliverables | Spec sections |
|-----------|-------------|----------------|
| **M1 - Full Working Draft** | Transport, semantics, sync, submit, RPC payload hex (utils) + referendum vote `data` framing (char-framing), mock harness, initial Alpen wiring | Section 3, Section 4.1-4.8, Section 6.2 (partial), char-framing vote + utils hex |
| **M2 - Tests & Integration** | Retry/gap/fault tests, fuzz, regression corpus, final Alpen adapter, generic adapter skeleton | Section 4.9-4.11, Section 8, Section 6.2, Section 6.3 |
| **M3 - Docs & Interface** | README, sync/submit examples, stability policy, OpenRPC, integration guide, operator runbook | Section 7, examples |
| **M4 - Polish** | Performance, memory audit, API cleanup, CI | Section 7.1, config, bounds |

---

## 10. Summary of POC Alignment

- **Hello-char-demo:** CharRpc -> becomes typed `CharRpcTransport`; domain hash -> `char-utils` + `DomainId`; **RPC votes / roll `data`** -> **`char-utils` hex of payload**; binary vote-body tools -> `char-framing`; ZMQ decode -> semantics layer parsing + `DecisionRollStreamEvent`/gap; catch-up -> `reconcile`; leader submit -> `submit_vote` / runners with leader check and optional read-after-write; state DB (last_ballot, count, roll blob) -> consumer responsibility, SDK exposes progress and verified roll type.
- **Strata char_sync:** `CharSyncPeer` -> Alpen adapter using SDK sync (reconcile + progress) and SDK submit; `get_blocks` = SDK fetch rolls + adapter decodes `L2BlockBundle` from payload; no Char logic in adapter; domain from config, not hardcoded in SDK.

This spec is the single source of truth for the SDK design; implementation shall follow it so that both POCs are realizable on top of the same integration-agnostic core.

---

## Appendix A: RPC Method Signatures (for OpenRPC / Machine-Readable Artifact)

```yaml
# Minimal schema for SDK-used methods (derive from char_rpc.cpp)

getdomaininfo:
  params: [ { name: domain_preimage, type: string (hex) } ]
  result:
    next_ballot: number
    next_leader_bond: string (txid hex)
    is_next_leader_mine: boolean
    latest_decided_ballot: number | null
    latest_decision_roll_hash: string (hex)
    latest_decision_data_hash: string (hex)
    latest_decision_zeitgeist: string (block hash hex)

getreferendumdecisionroll:
  params: [ domain_preimage (hex), start_ballot (number), end_ballot (number), verbosity (0|1|2) ]
  result: array of { domain_hash, ballot_number, found, decision_roll?: { roll_hash, data_hash, serialized, data (payload hex only, verbosity>=1), attestation_hash, proofs[] } }

addreferendumvote:
  params: [ referendumvote: array of { [domain_preimage_hex]: payload_hex }, mode?: "is_leader"|"init"|"plzfind" ]
  result: object with domain_preimage keys and boolean values

get_leader_for_slot_current_block:
  params: [ key_ranges: array of { key: domain_preimage_hex, start_slot: number, end_slot: number } ]
  result: array of { selections: array of { bond: txid, slot: number } }

getallcharbonds:
  params: [ verbosity?: number ]
  result: array of { txid: string, ... }
```

---

## Appendix B: ZMQ Message Layout (Reference)

- **Topic:** `"leader"` or `"decisionroll"` (first frame).
- **Body (leader):** `[varint ballot][32-byte domain hash]` (hello-char-demo and char_sync decode).
- **Body (decisionroll):** `[32-byte domain][1-byte leaf_type][serialized roll bytes]` (tag 1 = DecisionRoll).
- **Sequence:** 4 bytes LE (third frame). Transport delivers raw; semantics layer parses for stream events and gap detection.
