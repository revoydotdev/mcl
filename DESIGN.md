# MCL Language Specification

Version 0.1. This document defines the syntax and semantics of the MCL language.

## Overview

MCL is a declarative language for defining LLM-constrained applications. It handles four concerns in one coherent syntax:

1. **Graph topology** — nodes, channels, edges, memory configuration
2. **Game contexts** — rooms, entities, actions, transitions, narrator constraints
3. **Domain modules** — resolver declarations, entity type hierarchies, state schemas
4. **Templates** — reusable entity/property definitions with inheritance

## File Type Detection

The parser determines file type from the first significant token (ignoring whitespace and comments):

| First token | File type |
|-------------|-----------|
| `graph` | Graph definition |
| `domain` | Domain module |
| `template` | Template library |
| `---` | Context file |

All files use the `.mcl` extension.

## Lexical Structure

### Comments

```mcl
// Line comment
```

Block comments are not supported. `#` is reserved for section headers in context files.

### Identifiers

Bare alphanumeric + underscore sequences. Start with a letter or underscore.

```
my_node
skeleton_guard
combat_system
```

### Strings

Double-quoted. Required when the value contains spaces or special characters. Optional for simple values.

```mcl
backend = "ollama"           // quoted (contains no specials, but valid)
tone: dread                  // bare identifier, equivalent to "dread"
context: "A dark crypt."     // quoted (contains spaces)
```

### Numbers

```mcl
42          // integer
-7          // negative integer
3.14        // float
```

### Booleans and Null

```mcl
true
false
null
```

## Graph Files

### Structure

```mcl
graph "name" {
    domain <ident>

    node <name>: <type> { <config> }
    channel <name> { payload: { <schema> } }
    <from> -> <to> via <channel>
    memory <node> { <config> }
}
```

### Node Declarations

```mcl
node client: client {}
node narrator: llm {
    backend = "ollama"
    model = "llama3"
}
```

Format: `node <name>: <type> { key = value ... }`

Config uses `=` assignment (not `:`). This distinguishes config assignment from type annotation.

### Channel Declarations

```mcl
channel user_input { payload: { text: string } }
channel resolution { payload: { outcome: "success" | "failure", hint: string } }
```

Optional. When declared, enables payload schema validation on edges.

### Edges

```mcl
client -> narrator via user_input
narrator -> engine via narrative
```

Format: `<from> -> <to> via <channel>`

### Memory Configuration

```mcl
memory narrator {
    capacity = 500
    chunker = paragraph
    embedding = "nomic-embed-text-v1.5"
}
```

Per-node vector memory settings.

## Context Files

### Frontmatter

```mcl
---
context: crypt_antechamber
type: room
domain: rpg
extends: base_room
---
```

Required fields: `context`, `type`. Types: `room`, `scene`, `step`, `dialogue`, `encounter`.

### Sections

Sections are delimited by `#` headers. Order is flexible.

#### State

```mcl
# State
lit: bool = false
door_north: "locked" | "unlocked" = locked
hp: int & >=0 & <=100 = 100
```

Format: `name: [type] [= default]`

Type constraints follow CUE semantics:
- `int`, `float`, `bool`, `string` — primitive types
- `int & >=0` — range constraint
- `"a" | "b"` — enum (union of string literals)
- `[T]` — list of T
- `T?` — optional

#### Entities

```mcl
# Entities
entity skeleton_guard: enemy {
    disposition: hostile
    hp: 12
    ac: 12
    knows: [guards_tomb, knows_nothing_of_key]
}
```

Format: `entity <name>[: <type>] [extends <base>] { <properties> }`

#### Items

```mcl
# Items
item rusty_key { on: floor, visible_when: lit }
item torch_sconce { on: wall, interactive: true }
```

Format: `item <name> { <properties> }`

#### Actions

```mcl
# Actions
action attack {
    target: enum(entities where type == enemy)
    resolution: combat_system
    "Attack an enemy"
}

action light_torch {
    requires: [inventory.flint]
    mutates: { lit: true }
    "Light the torch"
}
```

Format: `action <name> { <fields> }`

Fields:
- `requires: [<condition>, ...]` — preconditions (all must hold)
- `mutates: { <key>: <value>, ... }` — direct state mutations
- `resolution: <system>` — delegate to a resolver node (mutually exclusive with `mutates`)
- `target: <target_expr>` — valid targets
- `mode: <ident>` — resolution mode hint
- `"description"` — bare string inside block is the description

Target expressions:
- `enum(entities)` — all entities
- `enum(entities where type == enemy)` — filtered
- `door_north` — fixed target

#### Transitions

```mcl
# Transitions
transition crypt_inner {
    when: door_north == unlocked
    direction: north
}
```

#### Narrator

```mcl
# Narrator
narrator {
    tone: dread
    voice: second_person
    tense: present
    forbidden: [epic, awesome, incredible]
    context: "Ancient crypt, smell of dust and decay."
    style: "Atmospheric and tense."
}
```

LLM behavioral constraints. Compiles into the system prompt at runtime.

### Inline Constraint Tags

XML-style tags annotate prose with tonal, affective, practical, and voice constraints. Tags are parsed from narrator context blocks and entity dialogue_style properties, compiled into system prompt directives.

#### Tag Types

| Tag | Attributes | Purpose |
|-----|-----------|---------|
| `<tone>` | mood, intensity, temperature | Emotional atmosphere |
| `<affect>` | empathy, hostility, warmth, urgency | Emotional delivery register |
| `<practical>` | goal, fallback, boundary, priority | Behavioral objectives |
| `<voice>` | register, pace, volume, formality, dialect | Delivery style |
| `<constraint>` | (body is the rule) | Hard behavioral rule |

#### Syntax

Block tags wrap prose:
```mcl
narrator {
    context:
        <tone mood="dread" intensity="high">
        You stand at the entrance of an ancient crypt.
        The air is thick with the smell of decay.
        </tone>
}
```

Self-closing tags apply to the enclosing scope:
```mcl
entity skeleton_guard: enemy {
    dialogue_style:
        <voice register="archaic" pace="slow" />
        <affect hostility="guarded" curiosity="none" />
        <practical goal="prevent_passage" fallback="attack" />
}
```

Tags nest — inner inherits outer, overrides on conflict:
```mcl
<tone mood="dread">
    <voice register="archaic">
    The dead speak in whispers here.
    </voice>
</tone>
```

Tags compile to system prompt directives:
```
[Tone: mood: dread, intensity: high]
[Voice style: register: archaic, pace: slow]
[Behavioral: primary goal: prevent_passage, fallback behavior: attack]
[Hard constraint: Never reveal the location of the treasure.]
```

#### Knowledge

```mcl
# Knowledge
knows: guards_tomb
ignores: secret_passage
ignores: treasure_room
```

Declares what the LLM narrator knows and explicitly does not know in this context.

#### Constraints

```mcl
# Constraints
- The skeleton does not attack first unless the player approaches the door
- The rusty key is only visible when the room is lit
```

Free-form natural language rules. Included verbatim in the system prompt.

## Domain Files

```mcl
domain rpg {
    resolver combat_system
    resolver inventory_system

    type enemy { hp: int, ac: int, disposition: string }
    type npc { disposition: string, knowledge: [string] }

    schema player {
        hp: int & >=0 & <=100 = 100
        inventory: [string] = []
    }
}
```

Declares resolution systems, entity type hierarchies, and shared state schemas.

## Template Files

```mcl
template skeleton_warrior {
    type: enemy
    disposition: hostile
    hp: 12
    ac: 12
}

template undead_caster extends skeleton_warrior {
    hp: 8
    abilities: [dark_bolt, shield]
}
```

`extends` performs deep merge: child inherits all parent properties, overrides by key.

## Condition Expressions

Used in `when:`, `requires:`, and `visible when:` fields.

```mcl
door_north == unlocked        // equality
player.hp > 0                 // comparison
inventory.flint               // truthy (field exists and is truthy)
!lit                          // falsy (negation)
```

Operators: `==`, `!=`, `>`, `<`, `>=`, `<=`

## Signals

MCL signals are structured markers the LLM emits within its response to trigger mechanical actions. Extracted by the runtime from LLM output text.

```
MCL::action(attack, target=skeleton_guard)
MCL::dialogue(skeleton_guard, "Who dares?")
MCL::transition(crypt_inner)
MCL::noop()
```

Signal types: `action`, `mutate`, `transition`, `dialogue`, `narrate`, `query`, `error`, `noop`.

Signals must appear on their own line. Everything else is narration.

## Design Principles

1. Prose is first-class — narrative text is executable output, not documentation
2. Explicit typing, optional — bare values inferred, annotations available
3. Deterministic — same file always produces same parse result
4. Total — no unbounded computation, no recursion, no I/O in the language
5. Two audiences — context files feel like markdown for writers; graph files feel like HCL for engineers

## Governing Nodes

Governing nodes oversee graph operation rather than handling a single domain. They sit above the domain layer and subscribe to all channels via wildcard edges. The runtime identifies them through the `GoverningNode` supertrait, which extends the base `Node` trait.

### GoverningNode Trait

```rust
#[async_trait]
pub trait GoverningNode: Node {
    fn governance_kind(&self) -> GovernanceKind;
}

pub enum GovernanceKind {
    Summarizer,
    Scheduler,
    Registry,
}
```

A governing node is a regular node with an additional method that declares its oversight role. The executor treats governing nodes the same as domain nodes for scheduling and lifecycle; the distinction is semantic, not structural.

### Wildcard Subscriptions

Governing nodes observe every packet in the graph. In `graph.toml`, this is expressed with a wildcard edge:

```toml
[[edges]]
from = "summarizer"
to = "summarizer"
channel = "*"
```

The `channel = "*"` directive means the node receives packets published on any channel. Domain nodes continue using named channels.

### Summarizer Node

The summarizer condenses graph activity into shorter outputs. It tracks conversation turns and, based on a policy, summarizes older content to keep the prompt within budget.

```toml
[nodes.summarizer]
kind = "summarizer"
policy = "TurnWindow(10)"
```

Policies:

| Policy | Syntax | Behavior |
|--------|--------|----------|
| TurnWindow | `"TurnWindow(n)"` | Keep the last n turns verbatim. Older turns are summarized into a condensed form. |
| TokenBudget | `"TokenBudget(n)"` | Keep as many recent turns as fit within n characters. Older turns are summarized or dropped. |

Summary hierarchy: **turn** summaries are produced when the policy threshold is reached. **Scene** summaries are produced when a scene boundary signal fires. A **session** summary is produced on shutdown. Each level persists to disk as JSON under a configurable `summaries_dir`.

### Scheduler Node

The scheduler reorders or delays work across nodes. It assigns priority to packets and enforces circuit breakers to prevent runaway nodes from starving the rest of the graph.

```toml
[nodes.scheduler]
kind = "scheduler"
max_wait = 30
priority_order = ["combat", "narrator", "client"]
```

Fields:

| Field | Default | Purpose |
|-------|---------|---------|
| `max_wait` | 60 | Maximum steps a node can wait before forced dispatch. |
| `priority_order` | `[]` | Node names in dispatch priority. Earlier names get processed first. |

Behavior:

- **Priority ordering.** Nodes listed in `priority_order` are dispatched first, in listed order. Unlisted nodes follow in round-robin order.
- **Circuit breakers.** A node that fails `max_consecutive_failures` times in a row (default 3) is suspended. The scheduler skips it until the failure count resets.
- **Starvation prevention.** A node waiting longer than `max_wait` steps receives `u32::MAX` effective priority, guaranteeing it runs next.

### graph.toml Configuration

Governing nodes are declared as regular `[nodes.X]` entries with a `kind` field. Domain nodes use `type`; governing nodes use `kind`. Both are optional on the same node (a node can have both).

```toml
[nodes.summarizer]
kind = "summarizer"
policy = "TurnWindow(10)"

[nodes.summarizer.memory]
enabled = true
persist = ".mcl/memory/summarizer/"

[nodes.scheduler]
kind = "scheduler"
max_wait = 30
priority_order = ["combat", "narrator", "client"]
```

The `memory` subtable works the same for governing nodes as for domain nodes. Full syntax reference in `docs/graph-toml-extensions.md`.

## Registry Node

The registry is an append-only signal log backed by a content-addressed store. It records every signal the graph produces, runs an arbiter chain to validate them, and persists the results to disk for cross-session replay and audit.

### Signal Log Format

The registry writes a JSONL file (one JSON object per line) in `data_dir`. Each line is a `SignalLogEntry`:

```json
{
  "id": { "hash": "a1b2c3...64_hex_chars" },
  "signal": { "kind": "Action", "name": "attack", "args": { "target": "skeleton_guard" } },
  "origin": "narrator",
  "turn": 7,
  "channel": "narrative",
  "timestamp": 1713700000000,
  "verdict": "Accept"
}
```

Fields:

| Field | Type | Description |
|-------|------|-------------|
| `id` | SignalId | blake3 hash of (origin, turn, channel, signal). Deterministic for the same inputs. |
| `signal` | MclSignal | The extracted signal. |
| `origin` | NodeId | Which node emitted the signal. |
| `turn` | u64 | Super-step number when the signal was emitted. |
| `channel` | string | Channel the signal was emitted on. |
| `timestamp` | u64 | Monotonic milliseconds since UNIX epoch. |
| `verdict` | Verdict | Disposition applied by the arbiter chain. |

### Arbiter Chain

Before a signal enters the log, it passes through an ordered sequence of arbiters. Each arbiter inspects the signal and returns a verdict. If any arbiter rejects, the signal is dropped.

Default chain (configurable in `graph.toml`):

| Stage | Behavior |
|-------|----------|
| Schema | Validate signal payload against the channel's declared schema. |
| Dedup | Drop exact duplicates of recently seen signals (content hash comparison). |
| Hallucination | Flag or reject signals matching known hallucination patterns. |
| ActionWhitelist | Reject signals whose action type is not declared in the current context. |

Stages run in the order listed in `arbiter_chain`. An empty chain means no validation: all signals are accepted.

### Verdict Semantics

```rust
pub enum Verdict {
    Accept,
    Reject { reason: String },
    Defer,
}
```

- **Accept.** The signal passed all arbiter stages. It is appended to the signal log and applied to world state.
- **Reject.** The signal failed an arbiter stage. The reason string records which stage and why. The rejected signal is still logged (for audit), but not applied.
- **Defer.** The signal requires additional context before a final decision. It is held for re-evaluation in a subsequent super-step.

### SignalId

Each signal is identified by a blake3 hash computed from (origin node, turn number, channel name, serialized signal). The hash is 32 bytes, displayed as 64 hex characters. Two signals with identical content produce the same SignalId, enabling dedup and content-addressed retrieval.

### Persistence Layout

```
.mcl/registry/
├── signal_log.jsonl          # append-only signal log
├── index.meta.json           # vector index metadata
├── index.usearch             # HNSW vector index
├── causality.bin             # causality graph edges
├── schema_version.json       # current schema version for migration
└── summaries/
    ├── turn-000001.json
    ├── scene-000001.json
    └── session.json
```

The registry uses file-level locking (`flock`) to prevent concurrent writers. Crash recovery replays the JSONL log to rebuild the in-memory index. Schema migration handles format changes between versions.

Fsync policy controls durability:

| Policy | Behavior |
|--------|----------|
| `EveryStep` | fsync after every super-step. Safest, slowest. |
| `EveryNSteps(n)` | fsync every n steps. |
| `OnShutdown` | fsync once on graph shutdown. Fastest, risks data loss on crash. |

## Memory Tag Taxonomy

Memory entries carry structured metadata via the `MemoryTag` enum. Tags are assigned by the chunker at ingest time and enable scoped retrieval during prompt construction.

### MemoryTag Enum

```rust
pub enum MemoryTag {
    NodeId(String),    // node_id:narrator
    Turn(u64),         // turn:42
    SignalId(String),  // signal_id:a1b2c3...
    Scope(String),     // scope:combat
    Entity(String),    // entity:skeleton_guard
    Context(String),   // context:crypt_antechamber
    Custom(String),    // custom:any_label
}
```

| Variant | Wire format | Assigned by |
|---------|-------------|-------------|
| NodeId | `node_id:<name>` | Chunker, from the node that produced the entry. |
| Turn | `turn:<n>` | Chunker, from the current super-step counter. |
| SignalId | `signal_id:<hex>` | Chunker, from the blake3 hash of the signal. |
| Scope | `scope:<name>` | Knowledge filter, from channel name conventions. |
| Entity | `entity:<name>` | Chunker, when the entry pertains to a specific entity. |
| Context | `context:<name>` | Chunker, from the active room/scene context. |
| Custom | `custom:<label>` | User-defined, for domain-specific tagging. |

### Wire Format

Tags serialize as plain strings with a colon separator: `"prefix:value"`. This makes them human-readable in JSON logs and backward-compatible with any string-based storage.

Round-trip: `tag.to_string()` produces the wire format. `"node_id:abc".parse::<MemoryTag>()` reconstructs the enum variant. Unknown prefixes produce a parse error; `Turn` rejects non-numeric values.

### tag! Macro

A shorthand macro constructs tags without repeating the variant name:

```rust
use mcl_memory::tag;

let t = tag!(node_id = "narrator");  // MemoryTag::NodeId("narrator".into())
let t = tag!(turn = 42);             // MemoryTag::Turn(42)
let t = tag!(scope = "combat");      // MemoryTag::Scope("combat".into())
```

### Scope Filtering (Knowledge Filter v2)

The `Scope` enum in the knowledge filter determines visibility rules:

```rust
pub enum Scope {
    Global,           // visible to all
    Context(String),  // visible to this context + Global
    Entity(String),   // visible to this entity + Global
    Player,           // visible to player actions + Global
}
```

Scope is derived from channel name conventions at ingest time. Global entries are visible everywhere. Context-scoped entries are visible only within their context. Entity-scoped entries are visible only to that entity's interactions. This replaces the earlier prompt-only knowledge filter with a registry-backed approach that works across sessions.
