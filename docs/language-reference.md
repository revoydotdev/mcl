# MCL Language Reference

MCL (Model Control Language) defines game contexts, state, entities, items, actions, and narration rules for LLM-powered applications. The runtime reads `.mcl` files, builds a world state, and uses the declarations to constrain LLM behavior.

## File Structure

An MCL file has two parts: a frontmatter header wrapped in `---` delimiters, followed by named sections marked with `# SectionName` headers.

```
---
context: <name>
type: <context_type>
---

# SectionName
<content>

# AnotherSection
<content>
```

Comments use `//`:

```mcl
// This is a comment
lit: false  // inline comment after value
```

Blank lines are ignored everywhere.

Sections are detected by `# Name` where Name is one of: `State`, `Entities`, `Items`, `Actions`, `Transitions`, `Narrator`, `Knowledge`, `Constraints`, `Includes`. Decorated headers like `# -- State ----` also work.

## Frontmatter

The frontmatter is required. It sits between two `---` lines at the top of the file.

```mcl
---
context: crypt_antechamber
type: room
domain: rpg
version: 1
tags: [dungeon, dark]
extends: base_room
---
```

### Fields

| Field | Required | Description |
|-------|----------|-------------|
| `context` | Yes | Unique identifier for this context |
| `type` | Yes | One of: `room`, `scene`, `step`, `dialogue`, `encounter` |
| `domain` | No | Groups related contexts (e.g., `rpg`, `tutorial`) |
| `version` | No | Integer version number |
| `tags` | No | String list `[tag1, tag2]` or single string |
| `extends` | No | Inherit from another context file |

Unknown keys in frontmatter are preserved as extra metadata.

## State Section

Room-level variables. Each entry is `key: value` where the value is inferred from its literal form.

```mcl
# State
lit: false
door_north: locked
torch_count: 3
temperature: 0.7
label: "ancient crypt"
features: [dark, damp]
metadata: { owner: skeleton_king, age: 500 }
```

### Value Types

| Literal | Type | Example |
|---------|------|---------|
| `true` / `false` | bool | `lit: false` |
| Integer digits | int | `hp: 12` |
| Digits with `.` | float | `temperature: 0.7` |
| Double-quoted | string | `label: "ancient crypt"` |
| Bare identifier | string | `door_north: locked` |
| `[a, b, c]` | list | `features: [dark, damp]` |
| `{ k: v }` | map | `metadata: { owner: skeleton_king }` |
| `null` or `~` | null | `empty: null` |

Values can be nested. A map can contain lists and other maps.

## Entities Section

Entities are NPCs, enemies, or other actors. Each starts with `- name:` followed by indented properties.

```mcl
# Entities
- skeleton_guard:
    type: enemy
    disposition: hostile
    hp: 12
    ac: 12
    knowledge: [guards_tomb, knows_nothing_of_key]
```

### Entity Fields

| Field | Required | Description |
|-------|----------|-------------|
| (name after `-`) | Yes | Entity identifier |
| `type` | No | Entity category (e.g., `enemy`, `npc`, `ally`) |
| `disposition` | No | Attitude toward the player (e.g., `hostile`, `friendly`) |
| `knowledge` | No | List of facts this entity knows `[fact1, fact2]` |
| Any other key | No | Stored as custom properties |

### Inline Entity Properties

Properties can also go on the same line using `{ }` map syntax:

```mcl
- ghost_king: { type: boss, hp: 50 }
```

Both inline and indented properties can be used together. If a property appears in both, the indented value wins.

## Items Section

Items are objects the player can find, pick up, or interact with.

```mcl
# Items
- rusty_key: { on: floor, visible_when: lit }
- torch_sconce: { on: wall, interactive: true }
- flint: { on: ground, near: entrance }
```

Items use the same list syntax as entities. The name follows `- `, and an optional inline map specifies properties. Indented properties also work:

```mcl
- magic_sword:
    damage: 10
    enchantment: fire
    weight: 3.5
```

All keys beyond the name are stored as custom properties. There are no reserved item fields.

## Actions Section

Actions the player can trigger. Each action has optional preconditions, mutation effects, and a resolution system.

```mcl
# Actions
- light_torch:
    requires: [inventory.flint]
    mutates: { lit: true }
    description: "Light the torch sconce on the wall"
- attack:
    target: enum(entities.enemies)
    resolution: combat_system
    description: "Attack an enemy"
- unlock_door:
    target: door_north
    requires: [inventory.rusty_key]
    mutates: { door_north: unlocked }
    description: "Unlock the northern door with a key"
- speak:
    target: enum(entities)
    mode: dialogue
    description: "Attempt to speak with someone"
- pickup:
    target: enum(items)
    resolution: inventory_system
    description: "Pick up an item"
- look:
    description: "Look around the room"
```

### Action Fields

| Field | Required | Description |
|-------|----------|-------------|
| (name after `-`) | Yes | Action identifier |
| `requires` | No | List of conditions that must all be true |
| `requires_any` | No | List of conditions where at least one must be true |
| `target` | No | `enum(field.path)` for dynamic targets, or a fixed name |
| `mutates` | No | Inline map of state changes `{ key: new_value }` |
| `resolution` | No | Name of the external resolver system |
| `mode` | No | Action mode (e.g., `dialogue`) |
| `cooldown` | No | Integer cooldown in turns |
| `description` | No | Human-readable description (quoted string) |
| `args` | No | Inline map of extra arguments `{ key: value }` |

### Condition Syntax

Conditions appear in `requires` and `requires_any`:

| Syntax | Meaning | Example |
|--------|---------|---------|
| `field.path` | Truthy check | `inventory.torch` |
| `!field.path` | Falsy check | `!lit` |
| `field == value` | Equality | `door_north == unlocked` |
| `field != value` | Inequality | `status != dead` |
| `field > value` | Greater than | `player.hp > 0` |
| `field < value` | Less than | `enemy.hp < 5` |
| `field >= value` | Greater or equal | `level >= 3` |
| `field <= value` | Less or equal | `weight <= 10` |

Comparison values can be integers, floats, booleans, quoted strings, or bare identifiers (treated as strings). Dot-separated field paths traverse nested state (e.g., `player.hp`).

### Target Specification

The `target` field supports two forms:

- **Dynamic**: `enum(field.path)` picks valid targets from the matching state path
- **Fixed**: a plain string names a specific target

```mcl
target: enum(entities.enemies)    # dynamic: any entity with type == enemy
target: enum(items)               # dynamic: any item
target: door_north                # fixed: the door_north state variable
```

## Transitions Section

Links to other contexts (rooms/scenes). A transition fires when its `when` condition becomes true.

```mcl
# Transitions
- crypt_inner:
    when: door_north == unlocked
    direction: north
```

### Transition Fields

| Field | Required | Description |
|-------|----------|-------------|
| (target after `-`) | Yes | Context to transition to |
| `when` | No | Condition that must be true |
| `direction` | No | Compass direction or label (`north`, `south`, `up`, etc.) |
| `auto` | No | `true` to auto-trigger without player input |
| `narrate` | No | Quoted narration text for the transition |

### Auto Transitions

```mcl
- collapse:
    when: timer == 0
    auto: true
    narrate: "The ceiling gives way!"
```

When `auto: true`, the transition fires automatically when the condition is met, without the player choosing it.

## Narrator Section

Instructions for the LLM when narrating this context. Uses `key: value` pairs.

```mcl
# Narrator
tone: dread
voice: second_person
tense: present
context: "You stand at the entrance of an ancient crypt. Dust motes drift through pale light."
style_notes: "Keep descriptions atmospheric and tense."
forbidden: [epic, awesome, incredible]
```

### Narrator Fields

| Field | Required | Description |
|-------|----------|-------------|
| `tone` | No | Emotional mood (e.g., `dread`, `whimsical`, `tense`) |
| `voice` | No | Narrative perspective (e.g., `second_person`, `third_person`) |
| `tense` | No | Narrative tense (e.g., `present`, `past`) |
| `context` | No | Quoted string: scene description for the LLM |
| `style_notes` | No | Quoted string: additional style guidance |
| `forbidden` | No | List of words the narrator must never use |

`forbidden` can be a bracketed list `[word1, word2]` or a single bare word.

## Knowledge Section

Facts the LLM narrator knows or must ignore. Uses list syntax.

```mcl
# Knowledge
- guards_tomb
- !secret_passage
- !treasure_room
```

- **Known**: `- fact_name` makes the fact available to the narrator
- **Unknown**: `- !fact_name` prevents the narrator from referencing it

The `!` prefix is a hard constraint. The narrator cannot mention or hint at unknown facts under any circumstances.

## Constraints Section

Free-form rules enforced by the game engine. Each is a bulleted string.

```mcl
# Constraints
- The skeleton does not attack first unless the player approaches the door
- The rusty key is only visible when the room is lit
- The flint is near the entrance and easy to find
```

Constraints are softer than state/action rules. They guide LLM behavior and runtime logic but don't block execution.

## Constraint Tags

Inline XML-style tags annotate prose with tonal, affective, and behavioral constraints. These are used in narrative output, not in `.mcl` files directly.

```mcl
<tone mood="dread" intensity="high">
    The air is thick with decay.
</tone>

<affect empathy="low" hostility="guarded" />
<practical goal="prevent_passage" fallback="attack">
    Guard the door at all costs.
</practical>
<voice register="archaic" pace="slow" volume="whisper">
    Speak ye not of the dead.
</voice>
<constraint>
    Never reveal the location of the treasure.
</constraint>
```

### Tag Types

| Tag | Purpose | Common Attributes |
|-----|---------|-------------------|
| `<tone>` | Emotional mood/atmosphere | `mood`, `intensity`, `temperature` |
| `<affect>` | Emotional delivery | `empathy`, `hostility`, `warmth`, `urgency` |
| `<practical>` | Behavioral goals | `goal`, `fallback`, `boundary`, `priority` |
| `<voice>` | Delivery style | `register`, `pace`, `volume`, `formality`, `dialect` |
| `<constraint>` | Hard rule (body text is the rule) | None |
| `<custom-name>` | Extensible for any purpose | Any |

Tags can be **block** (opening + closing), **self-closing** (`<tone mood="dread" />`), or **nested** (inner tags inherit outer context and override on conflict).

## Signal Protocol

Signals are the protocol between the LLM and the runtime. The LLM embeds them in its output text. The signal parser extracts them.

All signals follow the format: `MCL::<type>(<args>)`

### action

Triggers a game action.

```
MCL::action(attack, target=skeleton_guard)
```

First argument is the action name. Remaining arguments are `key=value` pairs.

### mutate

Changes a state variable.

```
MCL::mutate(lit, true)
MCL::mutate(player.hp, 0)
MCL::mutate(door_north, unlocked)
```

First argument is the field path. Second is the new value.

### transition

Moves to a different context.

```
MCL::transition(crypt_inner)
```

Single argument: the target context identifier.

### dialogue

NPC speech.

```
MCL::dialogue(skeleton_guard, "Who dares enter my tomb?")
```

First argument is the entity name. Second is the quoted dialogue text.

### narrate

Narrative tag for scene transitions or emphasis.

```
MCL::narrate(combat_start)
```

Single argument: a tag name.

### query

Queries a state value.

```
MCL::query(player.inventory)
```

Single argument: the field path to query.

### noop

No operation. Acknowledges input without side effects.

```
MCL::noop()
```

No arguments.

### error

Reports an error condition.

```
MCL::error("cannot reach that from here")
```

Single argument: a quoted error message.

## Graph Files

Graph files define the node-graph execution model. They use TOML syntax with a `.graph.toml` extension.

```toml
[graph]
name = "crypt_adventure"

[nodes.client]
type = "client"

[nodes.narrator]
type = "llm"
config.backend = "ollama"
config.model = "llama3"

[nodes.game_engine]
type = "game_engine"
config.contexts_dir = "examples/crypt"
config.start_context = "crypt_antechamber"

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
```

### Graph Structure

| Section | Purpose |
|---------|---------|
| `[graph]` | Graph name |
| `[nodes.<name>]` | Node definition with `type` and optional `config.*` |
| `[[edges]]` | Directed edge with `from`, `to`, `channel` |

Node types include: `client`, `llm`, `game_engine`, `combat_resolver`, `inventory_resolver`. Channels are typed message routes between nodes.
