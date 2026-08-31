# Stage 2 — MCL Graph Memory System (TODOs)

## Context

MCL (Model Control Language) is a Rust runtime for LLM-narrated interactive fiction. Stage 1 (Milestones 1–4) shipped the Pregel-style node graph, signal protocol, per-node three-store vector memory (`NodeMemory { incoming, outgoing, local }` in `crates/mcl-memory/src/node_memory.rs`), WorldState audit log (`crates/mcl-runtime/src/audit.rs`), and working examples. The Stage 1 audit files (`phase-1-audit.md`, `phase-2-audit.md`, `milestone-1-4-audit.md`, `final-audit.md`) record ~85% completion with documented gaps: no cross-session memory, no turn-order scheduler (Milestone 8.3 FAIL), only prompt-level knowledge filtering (Milestone 8.2 PARTIAL), unproven signal reliability against real LLMs (Milestone 5 BLOCKER), streaming trait without backends (6.2 FAIL), empty web frontend (9.3 FAIL), missing publish metadata (10.5 FAIL), residual panics / CLI test gap.

Stage 2 delivers a **graph-structured memory system**:
- Every node keeps a small local vector store (already present — harden and standardize).
- A class of **governing nodes** carries larger coordinating stores with privileged access.
- The **Registry Node** is the crown jewel: a single, authoritative log + arbiter for *every valid signal ever emitted* in an MCL graph. It is both write-ahead log and reference surface.

Approved design decisions:
1. **Executor-bound registry** — wired into `GraphExecutor`; `NodeContext::emit` routes all signal-bearing packets through `registry.ingest(...)` before dispatch. Impossible to bypass.
2. **Active arbiter** — `Accept | Reject(reason) | Quarantine(reason)`. Rejection short-circuits the packet and emits `MCL::error` back to origin.
3. **Cross-session + campaign-scoped persistence** — `.mcl/registry/<campaign_id>/` with append-only signal log, vector index, causality graph, schema version.
4. **New `mcl-registry` crate** — depends on `mcl-memory`, `mcl-graph`, `mcl-spec`.

This stage unlocks determinism/replay, hallucinated-signal detection, multi-LLM turn arbitration, hierarchical summarization, semantic retrieval during prompt construction, and a telemetry surface that itself resolves the Milestone 5 reliability question.

## Feasibility / Utility / Viability

- **Feasibility: HIGH.** `mcl-memory` (usearch HNSW + fastembed) is proven. The `Node` trait (`crates/mcl-graph/src/node.rs:38-71`) is extensible. Pregel super-steps give deterministic serialization for free. Audit log (`crates/mcl-runtime/src/audit.rs`) is the natural seed for the registry's persistence layer.
- **Utility: HIGH.** Closes four documented audit gaps (turn scheduler, state-level knowledge filter, cross-session memory, signal reliability telemetry) and unlocks replay, causality queries, hallucination quarantine, semantic prompt assembly.
- **Viability: HIGH** with two risks to mitigate in-plan: (a) signal schema evolution — enforce versioning + migration tests from M2.1; (b) single-writer discipline for the cross-session log — enforce with file lock + process-scoped `Arc<RwLock<Registry>>`.

## Macro Architecture

### Three layers

```
┌──────────────────────────────────────────────────────────┐
│ Layer 3: Registry Node  (single, executor-bound)         │
│   • append-only SignalLog   (.mcl/registry/*/signals.jsonl)
│   • vector index over signals + summaries                │
│   • causality graph (signal → parent_signals)            │
│   • arbitration policies (schema, dedup, hallucination)  │
│   • query API: by path / turn range / semantic / ancestors
└──────────────────────────────────────────────────────────┘
                           ▲ ingest / query
┌──────────────────────────────────────────────────────────┐
│ Layer 2: Governing Nodes                                 │
│   • SummarizerNode   (turn→scene→session→campaign)       │
│   • SchedulerNode    (turn-order, priority, timeouts)    │
│   • Wildcard channel subscriptions; larger stores (10×)  │
│   • Cross-session persistence by default                 │
└──────────────────────────────────────────────────────────┘
                           ▲ hooks into executor lifecycle
┌──────────────────────────────────────────────────────────┐
│ Layer 1: Per-node small stores (exists; harden)          │
│   • NodeMemory { incoming, outgoing, local }             │
│   • Standardized tag taxonomy:                           │
│       node_id, turn, signal_id, scope, entity, context   │
│   • Executor-driven population (on_packet_in / _out)     │
│   • Opt-in cross-session persistence                     │
└──────────────────────────────────────────────────────────┘
```

### Key types (new crate `mcl-registry`)

```rust
// Canonical log entry. Every accepted signal produces exactly one.
pub struct SignalLogEntry {
    pub id: SignalId,           // content-addressed (blake3 of canonical form)
    pub turn: u64,
    pub super_step: u32,
    pub origin_node: NodeId,
    pub channel: String,
    pub signal: MclSignal,      // from mcl-spec
    pub parents: Vec<SignalId>, // causal ancestry
    pub timestamp_ns: u64,
    pub verdict: Verdict,
}

pub enum Verdict { Accept, Quarantine(String) } // Reject entries are NOT persisted; emitted back as MCL::error

pub trait Arbiter: Send + Sync {
    fn review(&self, signal: &MclSignal, ctx: &IngestContext) -> Verdict;
}
// Built-in arbiters: SchemaArbiter, DedupArbiter, HallucinationArbiter, ActionWhitelistArbiter

pub struct RegistryNode { /* log, index, causality, arbiter chain */ }
pub trait GoverningNode: Node { fn governance_kind(&self) -> GovernanceKind; }
```

### Executor integration (in `mcl-graph`)

- `NodeContext::emit(packet)` → if packet carries signals, call `registry.ingest(&mut packet, origin, turn, super_step)`.
  - `Accept` → forward as today, but packet gains `registry_ids: Vec<SignalId>` in `meta`.
  - `Reject(reason)` → drop; synthesize `MCL::error(reason)` packet back to origin; emit telemetry event.
  - `Quarantine(reason)` → forward with `quarantined=true` meta flag; downstream nodes decide.
- `GraphExecutor::new(..)` accepts a `RegistryHandle` (opens or creates the campaign-scoped registry).
- Lifecycle hooks: `on_super_step_end` → summarizer rollup tick; `on_shutdown` → registry flush + fsync.

### Storage layout (cross-session, campaign-scoped)

```
.mcl/registry/<campaign_id>/
  ├── schema-version         (u32, text)
  ├── signals.jsonl          (append-only log)
  ├── signals.usearch        (HNSW vector index)
  ├── signals.meta.json      (id ↔ vector offset map)
  ├── causality.jsonl        (append-only edge log)
  ├── summaries/             (hierarchical summaries, one file per level)
  └── lock                   (flock; single-writer)
```

### Critical files to modify / create

- **New**: `crates/mcl-registry/{Cargo.toml, src/{lib.rs, log.rs, arbiter.rs, causality.rs, index.rs, handle.rs, node.rs, persist.rs, lock.rs}}`
- **New**: `crates/mcl-registry/tests/{log_append.rs, arbiter_chain.rs, causality_query.rs, crash_recovery.rs, cross_session.rs, e2e_with_graph.rs}`
- **Modify**: `crates/mcl-graph/src/context.rs` — add `registry: Option<RegistryHandle>`; route `emit()` through it.
- **Modify**: `crates/mcl-graph/src/executor.rs` — construct + own registry, wire lifecycle hooks.
- **Modify**: `crates/mcl-graph/src/node.rs` — add `GoverningNode` supertrait; allow wildcard `ChannelSpec`.
- **Modify**: `crates/mcl-memory/src/node_memory.rs` — standardize tag taxonomy; add `persist_path: Option<PathBuf>`; add `on_packet_in` / `on_packet_out` helpers callable from executor.
- **Modify**: `crates/mcl-nodes/src/{llm.rs, game_engine.rs, client.rs}` — query registry for context, adopt standardized tags.
- **Modify**: `crates/mcl-runtime/src/{prompt.rs, knowledge_filter.rs}` — optionally consult registry for semantic retrieval and state-level knowledge filtering.
- **Modify**: `crates/mcl-cli/src/main.rs` — new `registry` subcommand (`inspect`, `replay`, `verify`, `export`).

---

## Milestone Plan (TDD-first, exhaustive)

> Every milestone follows the same phase pattern:
> **P1 Spec & test-first** → write failing tests that encode the milestone's contract.
> **P2 Implementation** → smallest change to go green.
> **P3 Integration** → wire into `mcl-graph` / `mcl-runtime` / `mcl-nodes`; add integration tests.
> **P4 Property / fuzz** → add proptest coverage where state-spaces are non-trivial.
> **P5 Bench & telemetry** → criterion micro-bench + registry telemetry assertions.
> **P6 Docs & example** → update `ARCHITECTURE.md`, `DESIGN.md`, README snippets, `examples/crypt`.
> **P7 Regression gate** → full `cargo test --workspace`, `cargo clippy -- -D warnings`, `cargo fmt --check` must all pass before merge.
>
> Exit criteria for every milestone: all tests green, CI green, docs updated, no `cargo clippy` warnings, no `unwrap()` added outside `#[cfg(test)]`.

### M2.0 — Stage 1 closeout (the remaining ~15%)

**Goal**: every Stage 1 audit item currently **FAIL** or **PARTIAL** that is *not* already folded into M2.1–M2.12 ships green here, so Stage 2 builds on a fully-green Stage 1. Audit source: `phase-1-audit.md`, `phase-2-audit.md`, `milestone-1-4-audit.md`, `final-audit.md`.

**Phase 0.A — CI baseline (mechanical, must land first)**
- **P2.0.A.1** Fix `cargo fmt --check` (95 diffs currently). *(closes Milestone 1.3 PARTIAL)*
- **P2.0.A.2** Fix `mcl-llm/src/normalize.rs:1` clippy `needless_pass_by_value`. Drive `cargo clippy --workspace -- -D warnings` to zero. *(closes Milestone 1.4 FAIL)*
- **P2.0.A.3** CI job matrix green (fmt + clippy + test) on `arch-linux` self-hosted runner; add `macos-sonoma` and `win11-ci` as non-blocking lanes so platform regressions surface early.
- **P2.0.A.4** Add dev-deps repo-wide once: `proptest`, `criterion`, `tempfile`, `insta`, `assert_cmd`, `predicates`.

**Phase 0.B — Production panic cleanup** *(closes Milestone 1.1 residual)*
- **P2.0.B.1 (test-first)**: regression tests that exercise the two remaining unwraps — `crates/mcl-runtime/src/session.rs:151` and `crates/mcl-lang/src/constraints.rs:152` — with inputs that previously panicked; assert they now return `Result::Err` cleanly.
- **P2.0.B.2**: replace unwraps with `?` + typed errors via `MclError`.
- **P2.0.B.3**: repo-wide audit — every `unwrap()` / `expect()` outside `#[cfg(test)]` gets justified or fixed. Enforce going forward with `clippy::unwrap_used`.

**Phase 0.C — `mcl-cli` unit tests** *(closes Milestone 1.5 gap — mcl-cli has 0 unit tests)*
- **P2.0.C.1 (test-first)**: `crates/mcl-cli/tests/cli_subcommands.rs` using `assert_cmd` — cover `play`, `graph`, `validate`, `inspect`, `dot` happy path + one error path each.
- **P2.0.C.2**: extract pure logic from `main.rs` into testable modules; target ≥70% line coverage on extracted modules.

**Phase 0.D — Playtest evidence pack** *(closes Phase 2.6 FAIL, seeds M2.10 baseline)*
- **P2.0.D.1**: playtest harness `scripts/playtest.sh` drives `examples/crypt` through ≥10 scripted + ≥3 real-LLM sessions (Ollama local Llama 3.1 8B default); records JSONL transcripts + signal verdicts into `playtest-stage1-baseline.jsonl`.
- **P2.0.D.2 (test-first)**: regression test loads the baseline JSONL and asserts replay under `MockLlmBackend` reproduces the scripted runs bit-identically.
- **P2.0.D.3**: bug-fix round — every issue the playtest surfaces gets a failing test in the appropriate crate, then a fix.

**Phase 0.E — Streaming LLM backends** *(closes Milestone 6.2 FAIL)*
- **P2.0.E.1 (test-first)**: `crates/mcl-llm/tests/streaming.rs` — mock transport yields token chunks; `stream()` surfaces them in order; cancellation mid-stream drops the transport cleanly; errors mid-stream map to `MclError`.
- **P2.0.E.2**: implement `stream()` for Ollama (NDJSON), OpenAI (SSE), Anthropic (SSE), Mock backends.
- **P2.0.E.3 (integration)**: `mcl-signal/src/extract.rs` — incremental signal extraction over a stream; emit partial narration chunks + finalized signals at line boundaries.
- **P2.0.E.4**: `mcl-nodes/src/llm.rs` opt-in `streaming=true` config; existing non-streaming path remains default.
- **P2.0.E.5**: CLI `play` renders streamed narration progressively.

**Phase 0.F — Web frontend + server polish** *(closes Milestone 9.3 FAIL)*
- **P2.0.F.1 (test-first)**: `crates/mcl-server/tests/http_static.rs` — `/` returns index.html with 200; static assets served with correct MIME types.
- **P2.0.F.2 (test-first)**: `crates/mcl-server/tests/ws_roundtrip.rs` — client connects, submits input, receives narration + signals frame-by-frame; disconnect is clean.
- **P2.0.F.3**: build minimal SPA — plain TypeScript or Svelte (zero-runtime-framework preferred default): input box, streamed narration pane, signal sidebar, session controls. Dev setup under `crates/mcl-server/static-src/`.
- **P2.0.F.4**: Playwright smoke test — script a short crypt session and assert UI renders expected beats. Headless on CI.
- **P2.0.F.5**: accessibility baseline — semantic HTML, focus order, reduced-motion respect. Lighthouse a11y ≥ 90 recorded in `docs/stage-1-closeout.md`.

**Phase 0.G — Publish-adjacent prep**
- **P2.0.G.1**: per-crate `Cargo.toml` metadata (description, license, repository, keywords, categories). Full publish push is M2.12; this phase just makes `cargo publish --dry-run` pass in CI.
- **P2.0.G.2**: per-crate top-of-file doc comments (`//!`) summarizing purpose.

**Phase 0.H — Exit criteria gate**
- `cargo fmt --check`, `cargo clippy --workspace -- -D warnings`, `cargo test --workspace --all-features` all green.
- Playtest baseline committed.
- Web frontend renders a session end-to-end.
- Create `stage-1-closeout-audit.md` re-asserting every prior FAIL/PARTIAL is now PASS, or explicitly listing what moved to a later Stage 2 milestone with rationale.
- Only after this gate does M2.1 work begin.

### M2.1 — Tag taxonomy & memory lifecycle (foundation)

**Why first**: every downstream milestone relies on uniform tags and executor-driven memory population.

- **P2.1.1 (test-first)**: `crates/mcl-memory/tests/taxonomy.rs` asserting `MemoryTag::{NodeId, Turn, SignalId, Scope, Entity, Context}` round-trips through insert/search.
- **P2.1.2**: introduce `MemoryTag` enum + `tag!` macro; migrate existing `Vec<String>` call sites to typed tags with a `String`-backed wire format for backward compatibility.
- **P2.1.3 (test-first)**: integration test `crates/mcl-graph/tests/lifecycle_hooks.rs` — assert packets arriving at a node auto-populate `node.memory.incoming`, packets emitted auto-populate `outgoing`, without the node touching memory manually.
- **P2.1.4**: implement `NodeContext::on_packet_in` / `on_packet_out` lifecycle hooks; wire into `GraphExecutor`.
- **P2.1.5**: migrate `LlmNode` and `GameEngineNode` to executor-driven population; remove manual `node.memory.incoming.insert(...)`.
- **P2.1.6**: opt-in `persist_path: Option<PathBuf>` on `NodeMemory`; round-trip test with `tempfile`.
- **P2.1.7 (proptest)**: for any sequence of inserts within capacity, `recent(N)` returns last N in insertion order.
- **P2.1.8**: docs — "Memory Tag Taxonomy" section in `ARCHITECTURE.md`.

### M2.2 — Governing node trait & wildcard subscriptions

- **P2.2.1 (test-first)**: `crates/mcl-graph/tests/governing_node.rs` — mock `GoverningNode` with wildcard subscription receives every published packet.
- **P2.2.2**: add `GoverningNode: Node` supertrait; extend `ChannelSpec` with a `Wildcard` variant (or `subscribe_all: bool` on `NodeCapability`).
- **P2.2.3**: update `GraphExecutor` routing to fan-out wildcard subscribers.
- **P2.2.4 (test)**: determinism — wildcard ordering is deterministic across runs with seeded RNG.
- **P2.2.5**: `mcl-registry`, `SummarizerNode`, `SchedulerNode` all implement `GoverningNode`.
- **P2.2.6**: docs — "Governing Nodes" section in `DESIGN.md`.

### M2.3 — Registry Node MVP: append-only log + arbiter chain (no vector yet)

- **P2.3.1 (test-first)**: `crates/mcl-registry/tests/log_append.rs` — signals written in order; ids are deterministic (blake3 of canonical form); crash-recovery replays intact log (`tempfile` + kill-simulation with partial trailing line).
- **P2.3.2**: create `mcl-registry` crate; implement `SignalLog` (append-only JSONL with fsync policy: every N entries or every super-step end).
- **P2.3.3 (test-first)**: `arbiter_chain.rs` — `SchemaArbiter` rejects malformed signal; `DedupArbiter` quarantines exact repeat within window; `HallucinationArbiter` rejects action not declared in context; chain returns first non-Accept verdict.
- **P2.3.4**: implement built-in arbiters, each independently testable.
- **P2.3.5 (test-first)**: `crates/mcl-graph/tests/registry_bound_executor.rs` — emit a packet with 1 valid + 1 invalid signal; valid is logged with `SignalId`; invalid produces `MCL::error` packet back to origin; nothing leaks to downstream subscribers.
- **P2.3.6**: wire `RegistryHandle` into `NodeContext::emit`.
- **P2.3.7**: CLI — `mcl registry inspect <campaign>` prints last N entries; test via `assert_cmd`.
- **P2.3.8 (proptest)**: for any arbitrary sequence of (possibly malformed) signals, the log only contains Accepted/Quarantined, and for every Rejected input there is exactly one `MCL::error` emitted.

### M2.4 — Registry vector index + causality graph

- **P2.4.1 (test-first)**: `crates/mcl-registry/tests/semantic_query.rs` — insert N signals across 3 topics; semantic query returns the right topic cluster above irrelevant ones (mock embedder for determinism).
- **P2.4.2**: integrate `mcl-memory` `MemoryStore` into `RegistryNode` for the semantic index; tag entries by `SignalId`.
- **P2.4.3 (test-first)**: `causality_query.rs` — given chain `A → B → C`, `ancestors(C) == [A, B]`, `descendants(A) == [B, C]`.
- **P2.4.4**: implement causality capture — packets may declare `caused_by: Vec<SignalId>` in `meta`; registry links it. Default: packets auto-inherit the last N signals the node received as parents.
- **P2.4.5**: `registry.query(RegistryQuery)` API — by-path, by-turn-range, by-origin, by-semantic-similarity, by-ancestor/descendant.
- **P2.4.6 (bench)**: criterion — `ingest` throughput at 10k signals; `query` p99 under 5 ms at 100k signals.
- **P2.4.7**: CLI `mcl registry query --semantic "..."` + `--ancestors <signal_id>`.

### M2.5 — Cross-session persistence, campaign scoping, crash recovery, migrations

- **P2.5.1 (test-first)**: `cross_session.rs` — run session A, kill mid-super-step, open session B with same `campaign_id`; registry replays cleanly and new signals append after the recovered tail.
- **P2.5.2**: implement file locking (`fs2` or `fd-lock`) with single-writer discipline; readers use a snapshot.
- **P2.5.3 (test-first)**: `schema_migration.rs` — v0 fixture on disk, bump to v1, migration runs, log is re-readable.
- **P2.5.4**: `schema-version` file + migration framework; seed with identity migration for v1.
- **P2.5.5**: `SessionSnapshot` (`crates/mcl-runtime/src/snapshot.rs`) records `campaign_id` + last `SignalId`; resume continues from there.
- **P2.5.6 (proptest)**: for any interleaving of "fsync / kill / reopen", the log is a prefix of what was written — no torn entries after recovery.
- **P2.5.7**: CLI `mcl registry verify <campaign>` checks log ↔ index ↔ causality consistency; reports + optionally repairs.

### M2.6 — Summarizer governing node (hierarchical rollup)

- **P2.6.1 (test-first)**: `crates/mcl-registry/tests/summarizer_rollup.rs` — after N turns a `turn-summary` exists; after M scenes a `scene-summary` exists; summaries retrievable via `registry.query` and in the vector index.
- **P2.6.2**: `SummarizerNode` — subscribes wildcard, batches signals per turn, uses the configured LLM backend (reuse `mcl-llm`) to produce compressed summaries, writes to `.mcl/registry/<campaign>/summaries/`.
- **P2.6.3 (test)**: with `MockLlmBackend` the summary text is deterministic; assert reproducibility across runs.
- **P2.6.4**: configurable rollup policy: `TurnWindow(n)`, `SceneBoundary(transition_signal)`, `TokenBudget(t)`.
- **P2.6.5**: `mcl-runtime` `prompt.rs` consults summaries when context tokens exceed budget (new `SummaryRetrievalPolicy`).

### M2.7 — Scheduler governing node (closes Milestone 8.3)

- **P2.7.1 (test-first)**: `crates/mcl-registry/tests/scheduler.rs` — three LLM nodes with declared priorities → turn order matches priority; preemption on `MCL::transition` works; timeouts trigger fallback emission.
- **P2.7.2**: `SchedulerNode` — subscribes wildcard, maintains `turn_order: Vec<NodeId>`, arbitrates which LLM node gets the next super-step, enforces per-node timeouts + circuit breakers (closes TODOs.md line 51).
- **P2.7.3**: integrate with `GraphExecutor` via a `Scheduler` trait; default impl is round-robin when no `SchedulerNode` is present.
- **P2.7.4 (proptest)**: for any priority assignment + timeout config, no node starves longer than its max-wait; no two nodes execute in the same super-step when the scheduler is active.

### M2.8 — State-level knowledge filter v2 (closes Milestone 8.2 PARTIAL)

- **P2.8.1 (test-first)**: `crates/mcl-runtime/tests/knowledge_filter_v2.rs` — LLM queries state via registry; entries tagged with `scope != visible_to(node)` are filtered at registry-query time, not just at prompt time.
- **P2.8.2**: extend `knowledge_filter.rs` to take a `RegistryHandle` and filter registry query results.
- **P2.8.3**: tagged signals: `Scope::{Global, Context(id), Entity(id), Player}`; arbiter enforces scope on write; query filters on read.
- **P2.8.4**: regression — existing prompt-level filter tests still pass.

### M2.9 — Entity-scoped & context-scoped memory pools

- **P2.9.1 (test-first)**: `entity_memory.rs` — creating an NPC creates an `entity_pool` keyed by entity id; destroying it tombstones the pool; queries for that entity only return that pool's entries.
- **P2.9.2**: `MemoryPoolRegistry` inside `RegistryNode` — sparse map of pools with lazy allocation + eviction.
- **P2.9.3**: `mcl-nodes` `game_engine.rs` — on entity create/destroy signals, call pool lifecycle.
- **P2.9.4**: per-context pool — resets on `MCL::transition(new_context)` for non-persistent scopes; persists `Scope::Global`.

### M2.10 — Registry-backed signal reliability telemetry (resolves Milestone 5 BLOCKER)

- **P2.10.1 (test-first)**: `crates/mcl-registry/tests/telemetry.rs` — after a run, `registry.stats()` reports counts by verdict, by arbiter-that-rejected, by origin node, by signal kind.
- **P2.10.2**: telemetry aggregator + JSON export.
- **P2.10.3**: upgrade `mcl-signal/src/bench.rs` to drive real LLM backends (Ollama for local, gated on env) and emit registry-backed reliability scores.
- **P2.10.4**: add `playtest-registry.jsonl` artifact to `examples/crypt` dogfood run; CI optional job.
- **P2.10.5**: documented acceptance gate — ≥95% Accept rate on the crypt example with Llama 3.1 8B + structured fallback OFF, or documented gap + remediation path.

### M2.11 — Documentation, examples, dogfooding

- **P2.11.1**: update `ARCHITECTURE.md` — add "Memory Graph" chapter with the layered diagram above.
- **P2.11.2**: update `DESIGN.md` — governing nodes + registry semantics + tag taxonomy.
- **P2.11.3**: update `README.md` — new `mcl registry` subcommands, quickstart for cross-session resume.
- **P2.11.4**: `examples/crypt` — ship a `graph.toml` that opts into Summarizer + Scheduler + Registry; record a 30-turn playtest as `playtest-stage2.jsonl`.
- **P2.11.5**: write `docs/stage-2-walkthrough.md` — narrated tour from empty campaign to multi-session resume with semantic retrieval.

### M2.12 — Publish readiness (closes Milestone 10.5 FAIL)

- **P2.12.1**: per-crate `Cargo.toml` metadata (description, keywords, categories, license, repository) — including `mcl-registry`.
- **P2.12.2**: per-crate `README.md` snippet.
- **P2.12.3**: `cargo publish --dry-run` passes for each crate in dependency order.
- **P2.12.4**: CHANGELOG entry for Stage 2.

---

## Verification — end-to-end

After all milestones green:

1. `cargo test --workspace --all-features` — all tests pass; dev-deps (`proptest`, `criterion`, `tempfile`, `insta`) active.
2. `cargo clippy --workspace -- -D warnings` — clean.
3. `cargo fmt --check` — clean.
4. Run `cargo run -p mcl-cli -- play examples/crypt --campaign dogfood --registry` for 30 turns; quit; re-run with the same campaign; assert session resumes with semantic recall from the prior run (`scripts/verify-stage2.sh` wrapping `mcl registry verify dogfood`).
5. `cargo run -p mcl-cli -- registry query --semantic "the skeleton guard" --campaign dogfood` returns relevant prior turns.
6. `cargo run -p mcl-cli -- registry replay --from-turn 0 --campaign dogfood` reproduces the session deterministically under `MockLlmBackend`.
7. Criterion benches (`cargo bench -p mcl-registry`) show ingest ≥10k signals/s and query p99 < 5 ms at 100k signals on the dev machine.
8. Reliability telemetry — `jq '.accept_rate' .mcl/registry/dogfood/telemetry.json` ≥ 0.95 under Llama 3.1 8B, or an issue filed documenting the gap + mitigation.

## Out of scope (defer to Stage 3)

- Remote/clustered registry (multi-machine campaigns).
- Embedder plurality (OpenAI/Cohere/Voyage) — the `mcl-memory` embedder trait stays stable; only fastembed ships in Stage 2.
- Advanced web frontend features (auth, multi-user lobby, rich editor) — Stage 2 ships a functional single-player SPA via M2.0.F; anything richer is Stage 3.
- Streaming for non-inline signal modes beyond the current protocol.

(Milestones 6.2 Streaming and 9.3 Web Frontend, previously FAIL in Stage 1 audits, are now in-scope and closed by **M2.0 phases 0.E and 0.F**.)
