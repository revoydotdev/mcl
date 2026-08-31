# MCL Architecture

## System Overview

MCL is a Rust workspace of 15 crates implementing a node-graph runtime for LLM-constrained applications. The system has four layers: language, engine, memory, and application.

```
┌─────────────────────────────────────────────┐
│  Language Layer                              │
│  mcl-lang (parser, validator, emitter)       │
│  mcl-parse (legacy parser)                   │
│  mcl-spec (core types)                       │
├─────────────────────────────────────────────┤
│  Engine Layer                                │
│  mcl-graph (node trait, channels, executor)   │
│  mcl-signal (LLM output extraction)           │
│  mcl-llm (Ollama, OpenAI, Anthropic)          │
├─────────────────────────────────────────────┤
│  Memory Layer                                │
│  mcl-memory (vector stores, chunkers)         │
│  mcl-registry (signal log, arbiter, index)    │
├─────────────────────────────────────────────┤
│  Application Layer                           │
│  mcl-nodes (Client, LLM, GameEngine, etc.)   │
│  mcl-runtime (WorldState, prompt builder)     │
│  mcl-domains (RPG combat, inventory)          │
│  mcl-cli (CLI entry point)                    │
│  mcl-server (HTTP + WebSocket)                │
│  mcl-mcp (Model Context Protocol bridge)      │
│  mcl-wasm (WebAssembly bindings)              │
└─────────────────────────────────────────────┘
```

## Dependency Graph

```
mcl-spec          (no deps — pure data types)
  ↑
mcl-parse         (legacy .mcl parser)
mcl-signal        (signal extraction)
mcl-llm           (LLM backends)
mcl-memory        (fastembed + usearch)
  ↑
mcl-graph         (node trait, executor, graph def)
mcl-registry      (signal log, arbiter chain, causality, persistence)
  ↑
mcl-lang          (new parser, validator, emitter)
  ↑
mcl-runtime       (WorldState, prompts, context mgr)
  ↑
mcl-nodes         (concrete node implementations)
mcl-domains       (RPG resolvers)
  ↑
mcl-cli           (entry point)
```

No circular dependencies. Lower layers have no knowledge of higher layers.

## File Structure

```
mcl/
├── crates/
│   ├── mcl-spec/        # Core types shared by everything
│   │   └── src/
│   │       ├── ast.rs       # AST nodes for the MCL language
│   │       ├── document.rs  # Parsed .mcl document types
│   │       ├── error.rs     # MclError enum, Display impl
│   │       ├── expression.rs # Condition expressions, FieldPath
│   │       ├── signal.rs    # Signal types (Action, Mutate, etc.)
│   │       ├── span.rs      # Source location spans
│   │       └── value.rs     # MclValue enum (Int, Float, Bool, etc.)
│   ├── mcl-lang/        # MCL language parser (graph, context, domain, template files)
│   │   └── src/
│   │       ├── lexer.rs     # Tokenizer
│   │       ├── parser.rs    # Recursive descent parser
│   │       ├── validator.rs # Semantic validation
│   │       ├── emitter.rs   # GraphDef emission
│   │       └── constraints.rs # Constraint tag parsing/compilation
│   ├── mcl-parse/       # Legacy .mcl context file parser
│   │   └── src/
│   │       ├── document.rs  # Top-level document parsing
│   │       ├── lexer.rs     # Line-type classification
│   │       └── sections.rs  # Section-specific parsers
│   ├── mcl-signal/      # Signal extraction from LLM output
│   │   └── src/
│   │       ├── extract.rs   # Main extraction pipeline
│   │       └── parse.rs     # Individual signal parsing
│   ├── mcl-llm/         # LLM backend abstraction
│   │   └── src/
│   │       ├── backend.rs   # LlmBackend trait
│   │       ├── ollama.rs    # Ollama backend
│   │       ├── openai.rs    # OpenAI backend
│   │       ├── anthropic.rs # Anthropic backend
│   │       └── mock.rs      # MockLlmBackend for testing
│   ├── mcl-memory/      # Vector memory system
│   │   └── src/
│   │       ├── store.rs     # MemoryStore (HNSW index)
│   │       ├── node_memory.rs # Per-node memory wrapper
│   │       ├── chunker.rs   # Text chunkers (paragraph, signal, whole)
│   │       └── embedder.rs  # fastembed integration
│   ├── mcl-graph/       # Node-graph execution engine
│   │   └── src/
│   │       ├── graph_def.rs # GraphDef, NodeDef, EdgeDef types
│   │       ├── node.rs      # Node trait, NodeCapability, GoverningNode
│   │       ├── channel.rs   # Typed channels between nodes
│   │       ├── executor.rs  # Pregel-style GraphExecutor
│   │       ├── factory.rs   # NodeFactory for node registration
│   │       ├── scheduler.rs # Scheduler trait, RoundRobinScheduler
│   │       └── toml_loader.rs # .graph.toml file parser
│   ├── mcl-registry/   # Signal registry and governing nodes
│   │   └── src/
│   │       ├── lib.rs        # Public API re-exports
│   │       ├── log.rs        # SignalLog (append-only JSONL)
│   │       ├── arbiter.rs    # ArbiterChain, SchemaArbiter, DedupArbiter, etc.
│   │       ├── index.rs      # RegistryIndex (vector search over signals)
│   │       ├── causality.rs  # CausalityGraph (signal ancestry)
│   │       ├── persist.rs    # RegistryPersistence (file-locked JSONL)
│   │       ├── migration.rs  # SchemaMigrator (v0 → v1)
│   │       ├── recovery.rs   # Crash recovery (repair, rebuild, verify)
│   │       ├── query.rs      # RegistryQuery API
│   │       ├── telemetry.rs  # TelemetryAggregator, RegistryStats
│   │       ├── pools.rs      # MemoryPoolRegistry, entity/context pools
│   │       ├── handle.rs     # RegistryHandle (thread-safe wrapper)
│   │       ├── types.rs      # SignalId, SignalLogEntry, Verdict
│   │       └── nodes/
│   │           ├── summarizer.rs # SummarizerNode with RollupPolicy
│   │           └── scheduler.rs  # SchedulerNode with circuit breakers
│   ├── mcl-nodes/       # Concrete node implementations
│   │   └── src/
│   │       ├── client.rs    # ClientNode (stdin/stdout)
│   │       ├── llm.rs       # LlmNode (LLM backend wrapper)
│   │       ├── game_engine.rs # GameEngineNode (state, actions, transitions)
│   │       ├── combat.rs    # CombatNode (d20 combat)
│   │       └── inventory.rs # InventoryNode (pickup, drop, use)
│   ├── mcl-runtime/     # Session management and state
│   │   └── src/
│   │       ├── state.rs     # WorldState (global, local, entities, player)
│   │       ├── session.rs   # Session (turn loop, signal routing)
│   │       ├── prompt.rs    # System prompt generation
│   │       ├── context.rs   # ContextManager (load, activate, transition)
│   │       └── resolver.rs  # ActionResolver trait
│   ├── mcl-domains/     # Domain-specific resolvers
│   │   └── src/
│   │       └── rpg/
│   │           ├── combat.rs     # d20 combat resolver
│   │           └── inventory.rs  # Inventory resolver
│   └── mcl-cli/         # Command-line interface
│       └── src/
│           └── main.rs       # CLI subcommands: play, graph, validate, inspect
├── examples/
│   └── crypt/           # Crypt adventure demo (3 rooms)
│       ├── crypt_antechamber.mcl
│       ├── crypt_inner.mcl
│       ├── crypt_treasury.mcl
│       └── crypt_adventure.graph.toml
└── docs/
    ├── language-reference.md
    ├── comparison.md
    └── tutorial.md
```

## Execution Model

### Pregel Super-Steps

The graph executor uses a Pregel-inspired synchronous execution model:

1. **Poll sources** — source nodes (ClientNode) produce input without upstream triggers
2. **Collect pending** — identify all channels with unprocessed packets
3. **Fire subscribers** — for each pending channel, call `process()` on all subscriber nodes
4. **Route outputs** — drain each node's outbox, push packets into destination channels
5. **Repeat** — if any channel has new data, execute another super-step
6. **Quiesce** — when no channels have pending data, wait for next source input

Deterministic ordering within a step. Natural parallelism across independent branches. Clean error recovery: discard a node's outbox on failure.

### Node Lifecycle

```
NodeFactory::create(def) → node.init(ctx) → [process loop] → node.shutdown(ctx)
```

Every node implements `async_trait Node`:
- `capability()` — declares input/output channels
- `init(ctx)` — one-time setup (load contexts, create LLM backend)
- `process(channel, packets, ctx)` — handle incoming data, emit output via `ctx.emit()`
- `shutdown(ctx)` — cleanup
- `is_source()` / `poll_source(ctx)` — for nodes that produce data from external I/O

### Packet Flow

Messages between nodes are `Packet` structs carrying:
- `channel` — which channel this travels on
- `origin` — source node ID
- `payload` — structured `MclValue` data
- `signals` — extracted MCL signals (for narrative packets)
- `status` — pending/complete/failed
- `meta` — freeform metadata

## Implemented

These features exist, have tests, and work today.

### Signal Protocol

LLMs emit structured signals within freeform text to trigger mechanical actions:

```
The skeleton's jaw creaks open.

MCL::dialogue(skeleton_guard, "Who dares?")

You grip your sword.

MCL::action(attack, target=skeleton_guard)
```

The signal parser (`mcl-signal`) extracts `MCL::` lines from raw LLM output. Everything else is narration. Malformed signals become warnings and fall through as narration.

Signal types: `action`, `mutate`, `transition`, `dialogue`, `narrate`, `query`, `error`, `noop`.

Signal reliability hardening:
- Few-shot examples in the system prompt (concrete examples using the current context's actual actions)
- Retry-on-missing-signal: if the player's input maps to a known action and the LLM response contains no signal, re-prompt once
- Structured JSON fallback mode: `signal_mode = "structured"` in graph config requests JSON output with separate `narration` and `signals` fields

### Memory System

Each node has three vector memory stores:

| Store | Purpose | Population |
|-------|---------|------------|
| `incoming` | Context from upstream | Auto-chunked from received packets |
| `outgoing` | History of outputs | Auto-recorded on emit |
| `local` | Persistent scratchpad | Node-managed |

Stores use fastembed (ONNX, all-MiniLM-L6-v2) for embedding and usearch (HNSW) for ANN search. Supports semantic search, hybrid search (semantic + tag overlap), and recency queries. Fixed capacity with oldest-first eviction.

16 memory tests are `#[ignore]` because fastembed downloads ~100MB of models on first run.

### Knowledge Boundaries

Per-context knowledge declarations constrain what the LLM narrator can reference:

```mcl
# Knowledge
- guards_tomb         # narrator knows this
- !secret_passage     # narrator must never mention this
- !treasure_room      # narrator must never mention this
```

The `!` prefix is a hard constraint included in the system prompt. The prompt builder compiles these into explicit instructions: facts the narrator knows, and facts the narrator must not reference or hint at.

Knowledge boundaries are per-context. When the player transitions rooms, the knowledge set changes.

### Multi-Backend LLM Support

Three backends implemented:
- **Ollama** — local inference, streaming support, default for development
- **OpenAI** — GPT-4o, GPT-4o-mini, etc. via REST API
- **Anthropic** — Claude models via REST API

All backends implement the `LlmBackend` trait in `mcl-llm`. A `MockLlmBackend` exists for testing.

### State Management

`WorldState` tracks all mutable game state:
- `global` — persists across context transitions
- `local` — reset on context transition
- `entities` — entity property maps keyed by ID
- `player` — player state (HP, inventory)
- `changelog` — state changes from the current turn

State is owned by `GameEngineNode`. Dot-path access: `player.hp`, `entities.skeleton_guard.hp`, `door_north`.

### Prompt Generation

`build_system_prompt(doc, state)` compiles an MCL context document and current state into an LLM system prompt including:
- Narrator directives (tone, voice, tense, forbidden words)
- Scene description
- Current state variables
- Entity list with properties
- Available actions (filtered by precondition satisfaction)
- Signal syntax reference with few-shot examples
- Available transitions
- Knowledge boundaries (known and unknown)
- Constraints (verbatim)

### Context Transitions

`MCL::transition(target)` signals cause the game to switch contexts. The runtime loads the new `.mcl` file, resets local state, preserves global and player state, and rebuilds the system prompt.

### Combat and Inventory

Domain resolvers in `mcl-domains`:
- **Combat resolver**: d20 attack rolls vs AC, damage, HP reduction, death at 0
- **Inventory resolver**: pickup, drop, item visibility conditions

### CLI Commands

```bash
mcl play <dir>          # Linear session from .mcl context files
mcl graph <file>        # Node-graph session from .graph.toml
mcl validate <dir>      # Validate .mcl documents
mcl inspect <file>      # Print parsed AST
```

### Save/Load

Session persistence via JSON serialization. Save on quit, load to resume. Includes WorldState, active context, conversation history, and changelog.

## Memory Graph

The memory system is organized in three layers, each built on top of the last. Layer 1 existed in Stage 1. Layers 2 and 3 are Stage 2 additions.

```
┌──────────────────────────────────────────────────────────┐
│ Layer 3: Registry Node  (single, executor-bound)         │
│   append-only SignalLog   (.mcl/registry/*/signals.jsonl)│
│   vector index over signals + summaries                  │
│   causality graph (signal → parent_signals)              │
│   arbitration policies (schema, dedup, hallucination)    │
│   query API: by path / turn range / semantic / ancestors │
└──────────────────────────────────────────────────────────┘
                           ▲ ingest / query
┌──────────────────────────────────────────────────────────┐
│ Layer 2: Governing Nodes                                 │
│   SummarizerNode   (turn→scene→session→campaign)         │
│   SchedulerNode    (turn-order, priority, timeouts)      │
│   Wildcard channel subscriptions; larger stores (10x)    │
│   Cross-session persistence by default                   │
└──────────────────────────────────────────────────────────┘
                           ▲ hooks into executor lifecycle
┌──────────────────────────────────────────────────────────┐
│ Layer 1: Per-node small stores                           │
│   NodeMemory { incoming, outgoing, local }               │
│   Standardized tag taxonomy:                             │
│       node_id, turn, signal_id, scope, entity, context   │
│   Executor-driven population (on_packet_in / _out)       │
│   Opt-in cross-session persistence                       │
└──────────────────────────────────────────────────────────┘
```

### Layer 1: Per-Node Memory

Every node gets three vector stores via `mcl-memory`: `incoming` (auto-populated from upstream packets), `outgoing` (auto-recorded on emit), and `local` (node-managed scratchpad). Entries carry a standardized tag taxonomy (`node_id`, `turn`, `signal_id`, `scope`, `entity`, `context`). The executor populates incoming/outgoing automatically via `on_packet_in` / `on_packet_out` hooks. Cross-session persistence is opt-in.

### Layer 2: Governing Nodes

Governing nodes oversee the graph rather than processing a single domain. They implement the `GoverningNode` supertrait in `mcl-graph` and declare their governance kind (`Summarizer`, `Scheduler`, or `Registry`).

**SummarizerNode** (`mcl-registry`) condenses graph activity into tiered summaries: turn, scene, session, campaign. Rollup policies control when summaries fire: `TurnWindow(n)` rolls up every n turns, `SceneBoundary(signal)` rolls up on a specific signal, and `TokenBudget(chars)` rolls up when the accumulated text exceeds a budget. SummarizerNodes hold wildcard channel subscriptions and 10x larger memory stores than regular nodes.

**SchedulerNode** (`mcl-registry`) replaces the default round-robin `Scheduler` with priority-based ordering. It includes starvation prevention (no node waits forever) and circuit breakers (nodes that fail `DEFAULT_MAX_CONSECUTIVE_FAILURES` times in a row get temporarily disabled). Timeouts are configurable via `DEFAULT_TIMEOUT_STEPS`.

### Layer 3: Registry

The registry is a single executor-bound node backed by `mcl-registry`. It provides:

- **SignalLog**: append-only JSONL file with content-addressed (blake3) entries. Each entry records the signal, its verdict, the originating node, turn number, and parent signal IDs.
- **ArbiterChain**: pipeline of arbiters that inspect every incoming signal. Four built-in arbiters: `SchemaArbiter` (structural validation), `DedupArbiter` (duplicate detection), `HallucinationArbiter` (rejects signals the LLM should not know about), and `ActionWhitelistArbiter` (rejects undeclared actions). Arbiters return `Accept`, `Reject(reason)`, or `Defer`.
- **RegistryIndex**: vector index over signals and summaries for semantic query.
- **CausalityGraph**: tracks signal-to-parent relationships, enabling ancestry queries.
- **RegistryPersistence**: file-locked (fd-lock) JSONL storage with schema migration (v0 to v1 so far) and crash recovery (repair corrupted JSONL, rebuild indexes, verify consistency).
- **RegistryQuery**: unified query API supporting path-based, turn-range, semantic (vector similarity), and ancestor lookups.
- **TelemetryAggregator**: accumulates per-signal verdict stats broken down by arbiter, origin node, and signal kind. Accept rate is the key metric for the signal reliability gate.

## Planned

These features are documented in [ROADMAP.md](ROADMAP.md) but not yet implemented. They should not be assumed to work.

- **Signal reliability validation against real LLMs** — telemetry plumbing is in place via `TelemetryAggregator`. The ≥95% accept-rate acceptance gate under Llama 3.1 8B is pending a Stage 2 registry-backed re-run.
- **LSP server** — no language server protocol support. Syntax highlighting exists as a TextMate grammar.
- **Template files** — the `template` file type is parsed and validated by `mcl-lang`, with `extends` inheritance wired through the validator and emitter. Runtime consumption of templates (entity instantiation from templates) is not yet implemented.
- **WebSocket server and web frontend** — axum + tokio-tungstenite dependencies are declared, but the web interface is not production-ready.

## Design Decisions

**Why a bespoke language instead of YAML/TOML?** MCL has unique requirements: mixing narrative prose with typed structured data, expressing LLM behavioral constraints, defining signal protocols, and declaring graph topology in documents authored by both game designers and engineers. No existing format serves all these needs. The language draws from HCL (graph blocks), CUE (constraint types), Ink (narrative-first prose), and DOT (arrow edges).

**Why Pregel instead of async message passing?** Deterministic super-step execution makes debugging possible. Same inputs produce identical state evolution. Async message passing introduces non-determinism in processing order, which is unacceptable for a game engine where mechanical outcomes must be reproducible.

**Why hand-rolled parsers?** The MCL format is line-oriented and regular enough that a grammar engine adds complexity without benefit. The parser is ~600 lines of straightforward recursive descent. Consistent with the project's philosophy of minimal dependencies.

**Why fastembed + usearch for memory?** fastembed runs ONNX models locally with zero infrastructure. usearch provides a production-quality HNSW index in a single crate. Together they give real semantic search without requiring a vector database service. The tradeoff is a ~100MB model download on first use.

**Why three memory stores per node?** The Letta/MemGPT research established that tiered memory produces better agent behavior than a single store. MCL's incoming/outgoing/local mapping provides: upstream context awareness (incoming), output history for self-consistency (outgoing), and persistent learned facts (local).
