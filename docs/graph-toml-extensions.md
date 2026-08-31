# graph.toml Syntax Extensions (Stage 2)

This document specifies new TOML sections and fields for `graph.toml` files.
These extensions add governing nodes (registry, summarizer, scheduler),
per-node memory persistence, and arbiter chains. All extensions are opt-in.
Existing `graph.toml` files parse without changes.

## Existing Sections (unchanged)

The current `graph.toml` format has three sections. Nothing here changes.

```toml
[graph]
name = "my_graph"

[nodes.narrator]
type = "llm"
config.backend = "ollama"
config.model = "llama3"
priority = 1

[[edges]]
from = "client"
to = "narrator"
channel = "user_input"
```

Fields that remain as-is:

| Section | Field | Required | Default |
|---------|-------|----------|---------|
| `[graph]` | `name` | yes | - |
| `[nodes.X]` | `type` | yes | - |
| `[nodes.X]` | `config.*` | no | empty |
| `[nodes.X]` | `priority` | no | `0` |
| `[[edges]]` | `from` | yes | - |
| `[[edges]]` | `to` | yes | - |
| `[[edges]]` | `channel` | yes | - |

---

## New: `[registry]` Section

A top-level section that configures the graph-wide registry. The registry
tracks node capabilities, channel subscriptions, and campaign metadata.

```toml
[registry]
enabled = true
campaign_id = "dogfood"
fsync_policy = "every_step"
data_dir = ".mcl/registry/"
arbiter_chain = ["schema", "dedup", "hallucination", "action_whitelist"]
```

### Fields

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `enabled` | bool | no | `false` | Activate the registry governing node. |
| `campaign_id` | string | no | `""` | Label for this run. Written into memory tags and log entries. Useful for filtering during analysis. |
| `fsync_policy` | string | no | `"on_shutdown"` | When persisted data is flushed to disk. One of `"every_step"`, `"every_n_steps(n)"`, `"on_shutdown"`. |
| `data_dir` | string | no | `".mcl/registry/"` | Directory for registry persistence files. Created on first write if missing. |
| `arbiter_chain` | string[] | no | `[]` | Ordered list of validation stages applied to packets before they enter the registry. See Arbiter Chain below. |

### Fsync Policies

| Policy | Behavior |
|--------|----------|
| `"every_step"` | Flush after every super-step. Safest, slowest. |
| `"every_n_steps(n)"` | Flush every n super-steps. Replace `n` with an integer, e.g. `"every_n_steps(5)"`. |
| `"on_shutdown"` | Flush once when the graph shuts down. Fastest, risk of data loss on crash. |

### Arbiter Chain

The arbiter chain validates incoming packets through a sequence of checks.
Each stage can accept, reject, or modify a packet. If any stage rejects,
the packet is dropped.

Available stages:

| Stage | Behavior |
|-------|----------|
| `"schema"` | Validate packet payload against the channel's declared schema. |
| `"dedup"` | Drop packets that are exact duplicates of recently seen packets. |
| `"hallucination"` | Flag or reject packets that match known hallucination patterns. |
| `"action_whitelist"` | Reject packets whose action type is not on the whitelist. |

Stages run in the order listed. An empty chain means no validation.

### Minimal Registry Example

Just turn it on, accept all defaults:

```toml
[graph]
name = "minimal_registry"

[registry]
enabled = true

[nodes.client]
type = "client"

[nodes.narrator]
type = "llm"
config.backend = "ollama"
config.model = "llama3"

[[edges]]
from = "client"
to = "narrator"
channel = "user_input"
```

---

## New: Governing Nodes

Governing nodes oversee the graph's operation rather than handling a single
domain. They are declared as `[nodes.X]` entries with a `kind` field instead
of (or alongside) a `type` field.

Three kinds exist: `summarizer`, `scheduler`, and `registry`.

### `[nodes.summarizer]` — Summarizer Node

Condenses graph activity into shorter outputs. Subscribes to wildcard channels
so it sees every packet. The summarizer uses a policy to decide what to keep.

```toml
[nodes.summarizer]
kind = "summarizer"
policy = "TurnWindow(10)"
```

#### Fields

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `kind` | string | yes | - | Must be `"summarizer"`. |
| `policy` | string | no | `"TurnWindow(20)"` | Summarization policy. See below. |

#### Summarizer Policies

| Policy | Syntax | Description |
|--------|--------|-------------|
| TurnWindow | `"TurnWindow(n)"` | Keep the last n turns of conversation in full, summarize older turns into a condensed form. |
| TokenBudget | `"TokenBudget(n)"` | Keep as many recent turns as fit within n tokens. Older turns are summarized or dropped. |

Examples:

```toml
policy = "TurnWindow(10)"      # Keep 10 turns
policy = "TokenBudget(4000)"   # Stay within 4000 tokens
```

### `[nodes.scheduler]` — Scheduler Node

Reorders or delays work across nodes. Governs execution priority and timing.

```toml
[nodes.scheduler]
kind = "scheduler"
max_wait = 30
priority_order = ["combat", "narrator", "client"]
```

#### Fields

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `kind` | string | yes | - | Must be `"scheduler"`. |
| `max_wait` | integer | no | `60` | Maximum seconds a packet can wait before forced dispatch. |
| `priority_order` | string[] | no | `[]` | Node names listed in dispatch priority. Earlier names get processed first. Empty means no reordering. |

### `[nodes.registry]` — Registry Node (explicit)

When you need per-node control over the registry (custom memory settings,
for example), declare it as a node entry. The top-level `[registry]` section
creates a registry implicitly; this explicit form lets you override defaults.

```toml
[nodes.registry]
kind = "registry"
```

This node has no additional fields beyond `kind` and the shared memory
settings described below.

---

## New: Per-Node Memory Configuration

Any node (governing or domain) can enable persistent vector memory. Without
these settings, nodes use in-memory stores that are lost on shutdown.

```toml
[nodes.narrator]
type = "llm"
config.backend = "ollama"
config.model = "llama3"

[nodes.narrator.memory]
enabled = true
persist = ".mcl/memory/narrator/"
capacity = 500
```

The `memory` section is a TOML subtable under the node. In flat-dot form:

```toml
[nodes.narrator]
type = "llm"
config.backend = "ollama"
memory.enabled = true
memory.persist = ".mcl/memory/narrator/"
memory.capacity = 500
```

Both forms are valid TOML. The subtable form is easier to read when there
are multiple memory fields.

### Fields

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `memory.enabled` | bool | no | `false` | Turn on the three-store vector memory (incoming, outgoing, local) for this node. |
| `memory.persist` | string | no | `None` | File path for automatic persistence. When set, memory is saved on shutdown and loaded on startup. Relative paths resolve from the graph file's parent directory. |
| `memory.capacity` | integer | no | `100` | Maximum entries per store (each node has 3 stores: incoming, outgoing, local). |

When `memory.persist` is set, the runtime creates three files per store:
`{persist}-incoming.meta.json`, `{persist}-incoming.usearch`, and so on
for `outgoing` and `local`.

### Memory Tags

Stored entries carry structured metadata via the `MemoryTag` enum. Tags are
assigned automatically by the chunker and include:

| Tag | Format | Example |
|-----|--------|---------|
| NodeId | `node_id:value` | `node_id:narrator` |
| Turn | `turn:n` | `turn:42` |
| SignalId | `signal_id:value` | `signal_id:blake3hash` |
| Scope | `scope:value` | `scope:combat` |
| Entity | `entity:value` | `entity:skeleton_guard` |
| Context | `context:value` | `context:crypt_antechamber` |
| Custom | `custom:value` | `custom:anything` |

Tags are not configurable in graph.toml. The chunker assigns them based on
channel name and packet content.

---

## New: Wildcard Channel Subscription

Governing nodes subscribe to all channels via a wildcard. This is expressed
in the edges section using a special channel name.

```toml
[[edges]]
from = "summarizer"
to = "summarizer"
channel = "*"
```

The `channel = "*"` edge means the node receives packets from every published
channel. This is the standard pattern for governing nodes.

For domain nodes, continue using named channels:

```toml
[[edges]]
from = "client"
to = "narrator"
channel = "user_input"
```

---

## Full Example

A graph.toml using every extension:

```toml
[graph]
name = "full_demo"

# ── Registry ──────────────────────────────────────────────
[registry]
enabled = true
campaign_id = "dogfood"
fsync_policy = "every_step"
data_dir = ".mcl/registry/"
arbiter_chain = ["schema", "dedup", "hallucination", "action_whitelist"]

# ── Domain Nodes ──────────────────────────────────────────
[nodes.client]
type = "client"

[nodes.narrator]
type = "llm"
config.backend = "ollama"
config.model = "llama3"

[nodes.narrator.memory]
enabled = true
persist = ".mcl/memory/narrator/"
capacity = 500

[nodes.game_engine]
type = "game_engine"
config.contexts_dir = "examples/crypt"
config.start_context = "crypt_antechamber"

[nodes.game_engine.memory]
enabled = true
persist = ".mcl/memory/engine/"
capacity = 1000

[nodes.combat]
type = "combat_resolver"

# ── Governing Nodes ───────────────────────────────────────
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

# ── Edges ─────────────────────────────────────────────────
# Domain edges
[[edges]]
from = "client"
to = "narrator"
channel = "user_input"

[[edges]]
from = "narrator"
to = "game_engine"
channel = "narrative"

[[edges]]
from = "game_engine"
to = "client"
channel = "display"

[[edges]]
from = "game_engine"
to = "combat"
channel = "combat_action"

[[edges]]
from = "combat"
to = "game_engine"
channel = "resolution"

# Wildcard edges for governing nodes
[[edges]]
from = "summarizer"
to = "summarizer"
channel = "*"

[[edges]]
from = "scheduler"
to = "scheduler"
channel = "*"
```

---

## Field Reference (all extensions)

| Section | Field | Type | Default |
|---------|-------|------|---------|
| `[registry]` | `enabled` | bool | `false` |
| `[registry]` | `campaign_id` | string | `""` |
| `[registry]` | `fsync_policy` | string | `"on_shutdown"` |
| `[registry]` | `data_dir` | string | `".mcl/registry/"` |
| `[registry]` | `arbiter_chain` | string[] | `[]` |
| `[nodes.X]` | `kind` | string | - |
| `[nodes.X]` | `policy` | string | `"TurnWindow(20)"` |
| `[nodes.X]` | `max_wait` | integer | `60` |
| `[nodes.X]` | `priority_order` | string[] | `[]` |
| `[nodes.X.memory]` | `enabled` | bool | `false` |
| `[nodes.X.memory]` | `persist` | string | - |
| `[nodes.X.memory]` | `capacity` | integer | `100` |
| `[[edges]]` | `channel` | string | - (supports `"*"`) |

---

## Backward Compatibility

All new fields are optional with sensible defaults. This means:

1. Any existing `graph.toml` parses without errors.
2. Graphs that don't use `[registry]`, `kind`, or `memory` behave identically to before.
3. The `channel = "*"` wildcard is new syntax. Existing graphs that use `"*"` as a
   literal channel name (unlikely but possible) should rename that channel before
   upgrading.
4. The `kind` field on a node is optional. Nodes without `kind` are treated as
   domain nodes, same as today.
5. The `memory` subtable on a node is optional. Nodes without it get no persistent
   memory, same as today.

The only breaking edge case is a pre-existing channel literally named `"*"`.
If you have one, rename it before adopting these extensions.
