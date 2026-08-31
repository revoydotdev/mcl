# mcl-registry

Executor-bound signal registry for MCL's graph-structured memory system. Provides an append-only JSONL signal log, an arbiter chain for validation, a vector index for semantic retrieval, and a causality graph for provenance tracking.

## Key API Surface

- **`Registry`** — central type holding signal log, vector index, causality graph, and query engine
- **`SignalLog`** — append-only JSONL with configurable fsync policy
- **`ArbiterChain`** — ordered pipeline of `Arbiter` implementations (Schema, Dedup, Hallucination, ActionWhitelist)
- **`RegistryQuery`** — enum with 6 variants: `ByPath`, `ByTurnRange`, `ByOrigin`, `BySemantic`, `ByAncestors`, `ByDescendants`
- **`SummarizerNode`** — governing node producing hierarchical summaries (turn → scene → session)
- **`SchedulerNode`** — governing node with priority ordering, circuit breakers, starvation prevention
- **`MemoryPoolRegistry`** — entity-scoped memory pools with LRU eviction
- **`TelemetryAggregator`** — accumulates verdict stats and exports JSON snapshots
- **`RegistryPersistence`** — cross-session persistence with file locking (fd-lock)
- **`CrashRecovery`** — partial-write truncation and index rebuild from log

## Architecture

See [ARCHITECTURE.md](../../ARCHITECTURE.md) for the full dependency graph and layer diagram.

## Running Tests

```bash
MCL_MOCK_EMBEDDING=1 cargo test -p mcl-registry
```

## Benchmarks

```bash
MCL_MOCK_EMBEDDING=1 cargo bench -p mcl-registry
```
