# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - 2026-04-21

### Added

#### Stage 2: Graph-Structured Memory System

- **mcl-registry crate**: new executor-bound signal registry with append-only JSONL log, arbiter chain, vector index, causality graph, and cross-session persistence
- **M2.0 — Stage 1 closeout**: CI cleanup, repo-wide `unwrap()` audit with `clippy::unwrap_used` lint, streaming backends for all 4 LLM providers (Ollama, OpenAI, Anthropic, Mock), enhanced web frontend with streaming narration
- **M2.1 — Tag taxonomy**: `MemoryTag` enum with 7 variants (NodeId, Turn, SignalId, Scope, Entity, Context, Custom), `tag!` macro, executor-driven memory lifecycle hooks
- **M2.2 — Governing nodes**: `GoverningNode` trait with `GovernanceKind` enum, wildcard channel subscriptions, deterministic fan-out routing
- **M2.3 — Registry MVP**: `SignalLog` (append-only JSONL with fsync), `SignalId` (blake3 content-addressing), `ArbiterChain` (Schema, Dedup, Hallucination, ActionWhitelist arbiters), executor `drain_outbox` interception
- **M2.4 — Vector index + causality**: `RegistryIndex` integrating `MemoryStore` for semantic retrieval, `CausalityGraph` with ancestor/descendant queries, `RegistryQuery` API (6 variants)
- **M2.5 — Persistence**: `RegistryPersistence` with fd-lock file locking, `SchemaMigration` framework (v0→v1 identity), `CrashRecovery` with partial-write truncation and index rebuild
- **M2.6 — SummarizerNode**: hierarchical rollup with `RollupPolicy` (TurnWindow, SceneBoundary, TokenBudget), summary hierarchy (turn → scene → session)
- **M2.7 — SchedulerNode**: priority-based ordering with circuit breakers, starvation prevention, `Scheduler` trait in mcl-graph
- **M2.8 — Knowledge filter v2**: `Scope` enum (Global, Context, Entity, Player), `filter_with_registry()` for registry-backed scope enforcement
- **M2.9 — Entity-scoped memory pools**: `MemoryPoolRegistry` with lazy allocation, LRU eviction, tombstoning, context transitions
- **M2.10 — Telemetry**: `TelemetryAggregator` with verdict stats, per-arbiter/per-origin/per-signal-kind breakdowns, JSON export
- **M2.11 — Documentation**: ARCHITECTURE.md Memory Graph chapter, DESIGN.md governing nodes/registry/tag taxonomy sections, README.md registry quickstart

### Changed
- CombatNode now accepts injected `RngCore` for deterministic replay
- CLI extracted into lib.rs + command modules for testability
- Memory tests un-ignored with `MCL_MOCK_EMBEDDING=1` env var
- Web frontend enhanced with streaming narration display

### Fixed
- Inventory resolver now checks item existence before pickup
- Inventory resolver evaluates `visible_when` conditions
- README license corrected from "Proprietary" to "MIT"
- Repo-wide production `unwrap()` eliminated (zero outside `#[cfg(test)]`)

## [0.1.0] - 2025-12-15

### Added
- Initial release: MCL language parser, signal protocol, graph executor, LLM backends, memory system, CLI, web server
- Stage 1 milestones M1–M11: language foundation, node graph, per-node memory, signal extraction, knowledge filtering

[0.2.0]: https://github.com/revelri/mcl/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/revelri/mcl/releases/tag/v0.1.0
