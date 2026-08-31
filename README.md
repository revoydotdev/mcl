# MCL — Model Control Language

A Rust runtime and declarative language for building applications where an LLM narrates and a deterministic engine enforces the rules. You describe the world — rooms, entities, actions, knowledge boundaries — in `.mcl` files. The runtime wires the LLM to domain resolvers (combat, inventory, dialogue) through a Pregel-style node graph. The LLM generates prose; the engine decides what actually happens.

> **Status (2026-08):** Stage 1 is complete. Most of Stage 2 is implemented and
> unreleased; Stage 3 is extracting the registry and arbiter into a
> domain-independent action-validation layer. See [Project status](#project-status)
> for the exact boundary.

---

## Why this exists

LLMs hallucinate state. They forget rules. They contradict prior turns. The standard response — "just prompt harder" — scales poorly and fails silently.

MCL takes the opposite bet: **the LLM is a narrator, not a source of truth.** State lives in a typed engine. The LLM emits structured signals (`MCL::action(...)`, `MCL::mutate(...)`, etc.) inline with its prose. The runtime extracts them, validates them against the declared world, dispatches to resolvers, and feeds results back for the next turn. If the LLM hallucinates an action that wasn't declared, the registry arbiter rejects it and emits an `MCL::error` back to the origin. If it tries to mutate state it shouldn't know about, the knowledge boundary blocks it.

That makes three things tractable that are usually painful:

- **Determinism.** Replay a session under `MockLlmBackend` and reproduce it bit-for-bit.
- **Audit.** Every accepted signal is content-addressed (blake3) and appended to a per-campaign log.
- **Composition.** Domain logic (combat, inventory, dialogue) lives in separate nodes that speak through typed channels, not shared mutable state.

---

## Quick start

Requirements: Rust 1.75+, and either an [Ollama](https://ollama.com) instance (default) or an OpenAI/Anthropic API key.

```bash
git clone https://github.com/revoydotdev/mcl.git && cd mcl
cargo run --release -p mcl-cli -- play examples/crypt/ --backend ollama --model llama3.1
```

A crypt text adventure starts. Type `look around`, `attack skeleton`, `pick up flint`. The LLM narrates; the engine tracks HP, inventory, door state, and knowledge scope.

For graph mode (separate LLM, combat resolver, inventory, and game engine nodes on independent channels):

```bash
cargo run --release -p mcl-cli -- graph examples/crypt/crypt_adventure.graph.toml
```

For a session with cross-session registry-backed memory (Stage 2, currently requires building from source):

```bash
# Start (or resume) a campaign with persistent signal logging
cargo run --release -p mcl-cli -- play examples/crypt/ --campaign my_run --registry

# Later: resume the same campaign — all turns, state, and memory are restored
cargo run --release -p mcl-cli -- play examples/crypt/ --campaign my_run --registry

# Inspect the signal log for a campaign
cargo run --release -p mcl-cli -- registry inspect my_run

# Semantic query against the vector index
cargo run --release -p mcl-cli -- registry query --semantic "the skeleton" --campaign my_run

# Verify log integrity and index consistency
cargo run --release -p mcl-cli -- registry verify my_run
```

---

## The MCL language in 30 seconds

Rooms, entities, actions, and knowledge boundaries are declared in `.mcl` files:

```mcl
---
context: crypt_antechamber
type: room
domain: rpg
---

# State
lit: false
door_north: locked

# Entities
- skeleton_guard:
    type: enemy
    hp: 12
    ac: 12
    knowledge: [guards_tomb, knows_nothing_of_key]

# Actions
- attack:
    target: enum(entities.enemies)
    resolution: combat_system
- unlock_door:
    requires: [inventory.rusty_key]
    mutates: { door_north: unlocked }

# Transitions
- crypt_inner:
    when: door_north == unlocked
    direction: north

# Narrator
tone: dread
voice: second_person
forbidden: [epic, awesome, incredible]

# Knowledge
- guards_tomb
- !secret_passage     # declared to exist, but hidden from this context
- !treasure_room
```

The LLM emits signals inline:

```
The skeleton's jaw creaks open.
MCL::action(attack, target=skeleton_guard)
You grip your sword.
```

The runtime extracts the signal, routes it to the combat resolver, and feeds the result into the next prompt. Full syntax in [`docs/language-reference.md`](docs/language-reference.md).

---

## Architecture

```
┌──────────────────────────────────────────────────────────┐
│ Registry Node (Stage 2)                                   │
│   append-only signal log · vector index · causality graph │
│   arbiter chain: schema · dedup · hallucination · whitelist│
└──────────────────────────────────────────────────────────┘
                           ▲ ingest / query
┌──────────────────────────────────────────────────────────┐
│ Governing Nodes (Stage 2)                                 │
│   Summarizer · Scheduler · wildcard subscriptions         │
└──────────────────────────────────────────────────────────┘
                           ▲
┌──────────────────────────────────────────────────────────┐
│ Per-node memory (Stage 1)                                 │
│   NodeMemory { incoming, outgoing, local } · usearch HNSW │
└──────────────────────────────────────────────────────────┘
                           ▲
┌──────────────────────────────────────────────────────────┐
│ Pregel executor · typed channels · signal protocol        │
└──────────────────────────────────────────────────────────┘
```

16 crates and roughly 30,000 lines of Rust. Lower layers do not depend on higher
layers, and the workspace has no circular crate dependencies.

| Crate | Responsibility |
|---|---|
| `mcl-spec` | Core types; signal grammar; `MclSignal` enum. No deps. |
| `mcl-parse` / `mcl-lang` | `.mcl` parsers. `mcl-lang` is the current parser/validator/emitter; `mcl-parse` is the legacy parser retained for fixture migration. |
| `mcl-signal` | Extract `MCL::*` signals from LLM output; incremental streaming extractor. |
| `mcl-llm` | Ollama, OpenAI, Anthropic, Mock backends. Streaming supported on all four. |
| `mcl-memory` | Per-node three-store vector memory (`incoming`, `outgoing`, `local`) via [usearch](https://github.com/unum-cloud/usearch) HNSW + [fastembed](https://github.com/Anush008/fastembed-rs). |
| `mcl-graph` | Node trait, typed channels, Pregel-style super-step executor, lifecycle hooks, governing-node supertrait. |
| `mcl-registry` | *(Stage 2)* Executor-bound signal registry — append-only log, arbiter chain, vector index, causality graph, campaign-scoped persistence with crash recovery and schema migration. |
| `mcl-runtime` | `WorldState`, prompt builder, knowledge filter, session/snapshot, audit log. |
| `mcl-nodes` | Concrete `Node` impls: `Client`, `LlmNode`, `GameEngineNode`, `CombatResolver`, WS bridge. |
| `mcl-domains` | RPG domain modules (combat, inventory). |
| `mcl-cli` | `play`, `graph`, `validate`, `inspect`, `dot`, `registry {inspect,verify,query,replay}`. |
| `mcl-server` | Axum HTTP + WebSocket server; minimal SPA under `crates/mcl-server/static/`. |
| `mcl-mcp` | Model Context Protocol bridge (experimental). |
| `mcl-wasm` | WebAssembly bindings (sketch). |

See [`ARCHITECTURE.md`](ARCHITECTURE.md) for the dependency graph and channel topology.

---

## Project status

Nothing here is sugarcoated. The roadmap has 13 documented milestones across two stages.

### Stage 1 — foundation (complete)

- **M1–M4 (bulletproofing, language, graph, memory):** shipped. ~85% green at first audit; closed out to 100% in M2.0.
- **M5 signal reliability gate:** *partially* validated. A scripted playtest baseline (`playtest-stage2-baseline.jsonl`) and real-LLM transcripts exist for `examples/crypt`. The documented ≥95% accept-rate acceptance gate under Llama 3.1 8B is pending a Stage 2 registry-backed re-run — telemetry plumbing is in place but the CI run has not yet produced a signed artifact.
- **M6 streaming LLM backends:** Ollama (NDJSON), OpenAI (SSE), Anthropic (SSE), Mock. Incremental signal extraction over a stream.
- **M7 knowledge filtering (prompt-level):** shipped.
- **M8 scheduler & state-level knowledge filter:** the scheduler lives in Stage 2 M2.7; state-level filter v2 lives in M2.8.
- **M9 web frontend:** minimal SPA + WebSocket session, static + WS integration tests passing. No auth, no multi-user.
- **M10 publish readiness:** metadata landed across all workspace crates;
  `cargo publish --dry-run` remains part of the M2.12 gate.
- **M11 documentation:** architecture, design, language reference, tutorial, three phase-plan docs.

### Stage 2 — graph-structured memory (in progress, core features landed)

Plan of record: [`STAGE-2-TODOS.md`](STAGE-2-TODOS.md). Milestones M2.0–M2.12.

| Milestone | Status |
|---|---|
| M2.0 Stage 1 closeout (8 sub-phases) | done |
| M2.1 Tag taxonomy & memory lifecycle hooks | done |
| M2.2 Governing-node trait + wildcard subscriptions | done |
| M2.3 Registry MVP — append-only log + arbiter chain | done |
| M2.4 Vector index + causality graph + query API | done |
| M2.5 Cross-session persistence + crash recovery + schema migration | done |
| M2.6 Summarizer governing node | done |
| M2.7 Scheduler governing node (closes M8.3) | done |
| M2.8 State-level knowledge filter v2 (closes M8.2) | done |
| M2.9 Entity- and context-scoped memory pools | done |
| M2.10 Registry-backed signal reliability telemetry (resolves M5) | done |
| M2.11 Docs, examples, dogfooding | in progress |
| M2.12 Publish readiness + CHANGELOG | pending |

Roughly 56 of 65 tracked Stage 2 tasks are complete as of this commit.

### Known gaps and caveats

- **No release has been cut.** Versions are `0.1.0` across the workspace; nothing is on crates.io yet. The publish sweep lands in M2.12.
- **No multi-user or auth** in the web server. It's a single-player SPA wired to the runtime.
- **No clustered or remote registry.** Stage 2 registry is single-writer, local-FS, campaign-scoped. Multi-machine campaigns are explicitly deferred to Stage 3.
- **Embedder plurality** is not shipped — `mcl-memory` only wires fastembed. The trait is in place; other backends are deferred to Stage 3.
- **WASM and MCP crates are sketches.** They compile and type-check; neither has a full feature surface or an integration test suite.
- **Signal reliability under third-party LLMs** is telemetry-gated rather than proven. The architecture *assumes* LLMs can be prompted to emit the signal grammar reliably; the Stage 2 registry gives us the instrument to prove or disprove that claim at scale.

---

## Testing

```bash
cargo test --workspace --all-features
cargo clippy --workspace -- -D warnings
cargo fmt --check
```

30 integration test files across the workspace; 73 modules with inline `#[test]` fns. Property tests via `proptest` in `mcl-memory`. E2E via Playwright script in `scripts/stage2-playwright.sh`. Scripted playtest harness in `scripts/playtest.sh` drives ≥10 crypt sessions through `MockLlmBackend` and records JSONL transcripts.

CI runs on a self-hosted Arch Linux runner; macOS Sonoma and Windows 11 lanes are non-blocking.

---

## Contributing

This is currently a single-author research project on `master`. PRs welcome once the Stage 2 milestones land and `main` stabilizes. If you're reading this before then and want to discuss the signal protocol, registry arbiter design, or the graph-memory model, open an issue.

Commit style: conventional (`feat:`, `fix:`, `chore:`, `refactor:`, `ci:`, `docs:`, `test:`, `perf:`). Semver-respecting splits. No AI attribution in commit trailers.

---

## License

MIT. See [`LICENSE`](LICENSE).
