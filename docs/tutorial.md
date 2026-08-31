# Tutorial: Build a 2-Room Game with MCL

This tutorial walks you through creating a small adventure game with two rooms: a dark cave entrance and a treasure chamber. You will learn every major MCL feature by building it from scratch.

Time: ~30 minutes. Prerequisites: Rust 1.75+, Ollama running locally with a model (e.g., `llama3`).

## Step 1: Install MCL

Clone and build:

```bash
git clone https://github.com/revoydotdev/mcl.git
cd mcl
cargo build --release
```

This gives you the `mcl` binary at `target/release/mcl`. You can also run it directly with `cargo run --release`.

Verify it works:

```bash
cargo run --release -- validate examples/crypt/
```

You should see no errors.

## Step 2: Create Your Game Directory

```bash
mkdir -p my-game
cd my-game
```

All your game files will live here. MCL reads `.mcl` files for room definitions and `.graph.toml` files for graph topology.

## Step 3: Create Your First Room

Create `my-game/cave_entrance.mcl`:

```mcl
---
context: cave_entrance
type: room
domain: rpg
---

# State
torch_lit: false
door_open: false

# Entities
- goblin:
    type: enemy
    disposition: hostile
    hp: 8
    ac: 10
    knowledge: [cave_layout, fears_fire]

# Items
- torch: { on: wall, interactive: true }
- rusty_key: { on: floor, visible_when: torch_lit }

# Actions
- light_torch:
    description: "Light the torch on the wall"
    mutates: { torch_lit: true }
- attack:
    target: enum(entities.enemies)
    resolution: combat_system
    description: "Attack an enemy"
- pickup:
    target: enum(items)
    resolution: inventory_system
    description: "Pick up an item"
- look:
    description: "Look around the room"

# Transitions
- treasure_chamber:
    when: door_open == true
    direction: north

# Narrator
tone: suspense
voice: second_person
tense: present
context: "You stand at the mouth of a damp cave. Water drips from stalactites overhead. A torch sits unlit in a sconce on the wall. A small goblin lurks in the shadows, watching you. A heavy wooden door blocks the passage to the north."
style_notes: "The goblin is nervous and easily startled. Keep descriptions grounded and tense."
forbidden: [epic, awesome, incredible, amazing]

# Knowledge
- cave_layout
- fears_fire
- !treasure_location
- !exit_code

# Constraints
- The goblin attacks if the player approaches the door without the key
- The torch can be lit without any items
- The rusty key is only visible when the torch is lit
```

Let's break down what each section does:

- **Frontmatter** (`---` block): names the room, declares its type, and assigns a domain for grouping.
- **State**: boolean flags the runtime tracks. `torch_lit` starts false. `door_open` starts false.
- **Entities**: the goblin has type `enemy`, HP, AC, and knowledge tags. The combat resolver handles enemies.
- **Items**: the torch is on the wall. The rusty key only appears when `torch_lit` is true.
- **Actions**: things the player can do. `light_torch` mutates state directly. `attack` delegates to the combat resolver. `pickup` delegates to the inventory resolver.
- **Transitions**: links to other rooms. The treasure chamber is only accessible when `door_open` is true.
- **Narrator**: instructions for the LLM. Tone, voice, context description, forbidden words.
- **Knowledge**: what the narrator knows and must never mention. The narrator cannot hint at the treasure location.
- **Constraints**: freeform rules the engine enforces.

## Step 4: Run Your First Room

```bash
cd ..  # back to the mcl project root
cargo run --release -- play my-game/ --backend ollama --model llama3
```

You should see the room description and a prompt. Try typing:

```
look around
```

```
light the torch
```

```
pick up torch
```

The LLM narrates what happens. The runtime tracks state changes. If the LLM emits an `MCL::action(light_torch)` signal, the runtime applies `{ torch_lit: true }`. After that, the rusty key becomes visible.

If the LLM does not emit a signal, the runtime retries once with a nudge. If that fails too, the action falls through as narration without mechanical effect. This is the signal reliability tradeoff described in the [comparison doc](comparison.md).

## Step 5: Add Combat

Your room already has a goblin and an `attack` action with `resolution: combat_system`. When the LLM emits:

```
MCL::action(attack, target=goblin)
```

The runtime routes this to the combat resolver node. The resolver rolls a d20, adds the player's attack bonus, compares against the goblin's AC (10), and applies damage on a hit.

Try it in your running game:

```
attack the goblin
```

The combat resolver handles the mechanics. The LLM narrates the outcome. The goblin's HP decreases. At 0 HP, the goblin is defeated.

## Step 6: Add Knowledge Boundaries

Knowledge boundaries already exist in your room. The narrator knows `cave_layout` and `fears_fire` but cannot reference `treasure_location` or `exit_code`.

Try asking the narrator:

```
where is the treasure?
```

The narrator should deflect or say it doesn't know, because `!treasure_location` is a hard constraint. The narrator cannot hint at or reveal information marked with `!`.

This is one of MCL's key features: per-room knowledge boundaries that prevent the LLM from revealing information the player hasn't discovered yet.

## Step 7: Create a Second Room

Create `my-game/treasure_chamber.mcl`:

```mcl
---
context: treasure_chamber
type: room
domain: rpg
---

# State
chest_open: false
trap_sprung: false

# Entities
- guardian_spirit:
    type: npc
    disposition: neutral
    knowledge: [treasure_history, curse_warning, exit_riddle]

# Items
- gold_coins: { on: floor, visible_when: chest_open }
- ancient_amulet: { on: pedestal }
- stone_key: { on: floor }

# Actions
- open_chest:
    requires: [!trap_sprung]
    mutates: { chest_open: true, trap_sprung: true }
    description: "Open the ornate wooden chest"
- speak:
    target: enum(entities)
    mode: dialogue
    description: "Speak with someone"
- pickup:
    target: enum(items)
    resolution: inventory_system
    description: "Pick up an item"
- look:
    description: "Examine your surroundings"

# Transitions
- cave_entrance:
    direction: south

# Narrator
tone: wonder
voice: second_person
tense: present
context: "The treasure chamber opens before you. Gold light spills from cracks in the walls. A wooden chest sits on a stone pedestal at the center. A translucent figure hovers near the far wall, watching you with ancient eyes. An amulet glints on a smaller pedestal nearby."
style_notes: "The guardian spirit is ancient and solemn. It speaks in riddles. The chest has a trap that releases a puff of paralytic gas when opened carelessly."
forbidden: [modern, technology, electric]

# Knowledge
- treasure_history
- curse_warning
- exit_riddle
- !outside_world

# Constraints
- The guardian spirit warns about the trap if asked about the chest
- The chest trap is non-lethal but stuns the player briefly
- The amulet is the real treasure, not the gold coins
- The stone key can be used to safely open the chest (not yet implemented)
```

This room demonstrates:
- An NPC (not an enemy) the player can talk to
- Conditional item visibility (gold coins only appear when the chest is open)
- Actions with preconditions (`open_chest` requires `!trap_sprung`, meaning the trap hasn't been sprung yet)
- A trap that fires as a side effect of opening the chest
- Knowledge boundaries specific to this room (the narrator knows about the curse but not the outside world)
- A transition back to the first room

## Step 8: Connect the Rooms with a Graph

Now you need to tell the runtime how nodes connect. Create `my-game/adventure.graph.toml`:

```toml
[graph]
name = "cave_adventure"

[nodes.client]
type = "client"

[nodes.narrator]
type = "llm"
config.backend = "ollama"
config.model = "llama3"

[nodes.game_engine]
type = "game_engine"
config.contexts_dir = "my-game"
config.start_context = "cave_entrance"

[nodes.combat]
type = "combat_resolver"

[nodes.inventory]
type = "inventory_resolver"

# Client sends user input to the LLM
[[edges]]
from = "client"
to = "narrator"
channel = "user_input"

# LLM sends narrative to the game engine
[[edges]]
from = "narrator"
to = "game_engine"
channel = "narrative"

# Game engine sends display output to the client
[[edges]]
from = "game_engine"
to = "client"
channel = "display"

# Game engine routes combat actions to the combat node
[[edges]]
from = "game_engine"
to = "combat"
channel = "combat_action"

# Combat results go back to the game engine
[[edges]]
from = "combat"
to = "game_engine"
channel = "resolution"

# Game engine routes inventory actions to the inventory node
[[edges]]
from = "game_engine"
to = "inventory"
channel = "inventory_action"

# Inventory results go back to the game engine
[[edges]]
from = "inventory"
to = "game_engine"
channel = "resolution"

# Game engine sends feedback and system prompts to the LLM
[[edges]]
from = "game_engine"
to = "narrator"
channel = "feedback"

[[edges]]
from = "game_engine"
to = "narrator"
channel = "system_prompt"
```

This defines the node graph: client, narrator (LLM), game engine, combat resolver, and inventory resolver. Edges connect them through typed channels.

Run it:

```bash
cd ..  # mcl project root
cargo run --release -- graph my-game/adventure.graph.toml
```

Now you have a fully connected game with combat and inventory routing through the graph.

## Step 9: Play Through Both Rooms

Here's a sample playthrough:

1. **Start in cave_entrance**: "look around" to see the room.
2. **Light the torch**: "light the torch". The LLM should emit `MCL::action(light_torch)`, and `torch_lit` becomes true.
3. **Find the key**: "look around" again. The rusty key is now visible. "pick up rusty key".
4. **Fight the goblin**: "attack goblin". Combat resolver handles the fight.
5. **Open the door**: You need to add an `open_door` action to `cave_entrance.mcl` (see below) or have the key trigger it via a transition condition. For now, you could add:

```mcl
# Add to cave_entrance.mcl in the Actions section:
- open_door:
    requires: [inventory.rusty_key]
    mutates: { door_open: true }
    description: "Open the heavy wooden door to the north"
```

6. **Go north**: "go north" or "enter the door". The LLM should emit `MCL::transition(treasure_chamber)`.
7. **Explore the treasure chamber**: "look around", "speak with guardian_spirit".
8. **Open the chest**: "open the chest". The trap springs. `chest_open: true` and `trap_sprung: true`.
9. **Pick up the amulet**: "pick up ancient_amulet".
10. **Go back south**: "go south" returns to the cave entrance.

## Step 10: Add Knowledge Boundaries to the Graph

Knowledge boundaries are already working in your `.mcl` files. Each room's `# Knowledge` section controls what the narrator can reference.

To see the effect, try this in the cave entrance:

```
tell me about the treasure chamber
```

The narrator should not describe the treasure chamber in detail because `!treasure_location` is set. It might say something vague about deeper passages, but it cannot reveal what's inside.

After transitioning to the treasure chamber, the knowledge set changes. Now the narrator knows about `treasure_history` and `curse_warning`. Ask the guardian spirit:

```
ask the spirit about the history of this place
```

The narrator, now operating under the treasure chamber's knowledge boundaries, can draw on `treasure_history` to respond.

## What You Built

You now have a 2-room game with:
- State tracking (torch, door, chest, trap)
- An enemy with combat resolution (goblin)
- An NPC with dialogue (guardian spirit)
- Items with visibility conditions (rusty key, gold coins)
- Actions with preconditions and mutations
- Room transitions with conditions
- Knowledge boundaries per room
- A node-graph execution model with Pregel super-steps
- LLM narration constrained by tone, voice, forbidden words, and knowledge

## Next Steps

- Add more rooms by creating additional `.mcl` files and adding transitions
- Tune the narrator prompts (tone, context, style_notes) to improve LLM behavior
- Experiment with different models (`--model mistral`, `--model gpt-4o`)
- Try the structured signal fallback mode: add `config.signal_mode = "structured"` to the narrator node in your `.graph.toml`
- Read the [language reference](language-reference.md) for the full MCL syntax
- Read the [architecture doc](../ARCHITECTURE.md) to understand how the runtime works internally
