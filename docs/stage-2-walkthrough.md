# Stage 2 Walkthrough: Registry, Memory, and Cross-Session Persistence

This walkthrough takes you from an empty campaign to a multi-session playthrough with semantic retrieval and deterministic replay. You will use the crypt adventure example that ships with MCL.

Time: ~20 minutes. Prerequisites: Rust 1.75+, Ollama running locally with `llama3` pulled, MCL built from source.

## 1. Setup

Clone and build if you haven't already:

```bash
git clone https://github.com/revoydotdev/mcl.git
cd mcl
cargo build --release
```

Verify the build:

```bash
cargo run --release -- validate examples/crypt/
```

You should see:

```
✓ crypt_antechamber.mcl — valid
✓ crypt_inner.mcl — valid
✓ crypt_treasure.mcl — valid
```

Confirm Ollama is running with a model available:

```bash
ollama list
```

You should see at least one model (e.g., `llama3.1` or `llama3`) in the output.

## 2. First Session

Start a registry-backed campaign. The `--campaign` flag gives the run a name; `--registry` turns on the append-only signal log, arbiter chain, and vector index:

```bash
cargo run --release -p mcl-cli -- play examples/crypt/ \
  --campaign my_first_run \
  --registry \
  --backend ollama \
  --model llama3
```

You land in the antechamber:

```
=== crypt_antechamber ===
You stand at the entrance of an ancient crypt. Dust motes drift through
a shaft of pale light from the passage behind you. The air is thick with
the smell of decay and old stone.
>
```

Play for a few turns. Try these:

```
> look around
> examine skeleton
> attack skeleton
> pick up flint
> unlock door
```

Each turn, the LLM narrates while the engine tracks HP, inventory, and door state behind the scenes. Every accepted signal — actions, mutations, transitions — is content-addressed with blake3 and appended to the campaign log.

After 5–6 turns, type `quit` or press `Ctrl-D` to end the session.

## 3. Examining the Registry

The registry stores everything that happened during the session. Inspect it:

```bash
cargo run --release -p mcl-cli -- registry inspect my_first_run
```

You will see output like:

```
Campaign: my_first_run
Signals: 14
  turn 1  signal_id: a3f7c2…  kind: action(attack, skeleton_guard)  verdict: Accept
  turn 1  signal_id: b8e1d4…  kind: mutate(hp, skeleton_guard, 12→5)  verdict: Accept
  turn 2  signal_id: c4f9a0…  kind: action(pick_up, flint)  verdict: Accept
  turn 2  signal_id: d2b6e8…  kind: mutate(inventory, +flint)  verdict: Accept
  turn 3  signal_id: e7a3f1…  kind: action(unlock_door)  verdict: Accept
  turn 3  signal_id: f5c8d9…  kind: mutate(door_north, locked→unlocked)  verdict: Accept
  ...
Index entries: 14
Causality edges: 6
Log integrity: ✓ OK
```

Key fields:

- **signal_id** — first 6 hex chars of the blake3 hash. Identifies each signal uniquely.
- **kind** — the signal type and arguments. These correspond to the `MCL::*` inline grammar.
- **verdict** — the arbiter chain's decision. `Accept` means it passed all four stages (schema, dedup, hallucination, action_whitelist). Rejected signals show the rejection reason.

Verify the log's integrity (checks content hashes and index consistency):

```bash
cargo run --release -p mcl-cli -- registry verify my_first_run
```

Output:

```
Log integrity: ✓ OK (14 signals, 0 gaps)
Index consistency: ✓ OK (14 entries, 0 orphans)
Causality graph: ✓ OK (6 edges, 0 cycles)
```

## 4. Cross-Session Resume

Start the same campaign again with the same command:

```bash
cargo run --release -p mcl-cli -- play examples/crypt/ \
  --campaign my_first_run \
  --registry \
  --backend ollama \
  --model llama3
```

The runtime restores the world state, inventory, and memory from where you left off. If you had unlocked the door and moved to the inner chamber, you resume there:

```
=== crypt_inner ===
[restored from campaign: my_first_run, turn 6]
The narrow passage opens into a damp chamber. Water drips from stalactites
overhead. A rusted chest sits in the far corner.
>
```

The `[restored from campaign]` line confirms that session state was loaded from the registry log rather than starting fresh. Continue playing — new turns append to the same campaign log.

End the session again (`quit` or `Ctrl-D`). Inspect the registry to see signals from both sessions:

```bash
cargo run --release -p mcl-cli -- registry inspect my_first_run
```

The signal count will have grown, and `turn` numbers pick up where the first session ended.

## 5. Semantic Queries

The registry indexes every signal in a vector store. You can query it by meaning, not just exact match.

```bash
cargo run --release -p mcl-cli -- registry query \
  --semantic "skeleton guard" \
  --campaign my_first_run
```

Output:

```
Query: "skeleton guard"
Results (3):

  [0.89] turn 1  action(attack, skeleton_guard)
         signal_id: a3f7c2e1d4b8…

  [0.84] turn 1  mutate(hp, skeleton_guard, 12→5)
         signal_id: b8e1d4a3f7c2…

  [0.71] turn 2  context:crypt_antechamber
         "The skeleton slumps against the wall, its sword clattering..."
         signal_id: e5d2b8c4a9f1…
```

The bracketed numbers are cosine similarity scores (0–1). Results are ranked by relevance. This works because each signal's text is embedded at ingest time using the same model that backs the memory stores.

Try a different query:

```bash
cargo run --release -p mcl-cli -- registry query \
  --semantic "unlocking the iron door" \
  --campaign my_first_run
```

This finds the `unlock_door` action and the corresponding `mutate(door_north, locked→unlocked)` signal even though the query doesn't use the exact field names.

If you want to test semantic queries without a real embedding model (e.g., in CI), use the mock embedding flag:

```bash
MCL_MOCK_EMBEDDING=1 cargo run --release -p mcl-cli -- registry query \
  --semantic "anything" \
  --campaign my_first_run
```

Mock embeddings produce deterministic (but meaningless) vectors, so results will appear but similarity scores won't reflect real semantic similarity.

## 6. Deterministic Replay

Every campaign can be replayed exactly. The `MockLlmBackend` replays canned LLM responses from a recorded transcript, and the registry log ensures the game engine sees the same signals in the same order.

```bash
cargo run --release -p mcl-cli -- registry replay my_first_run
```

Output:

```
Replaying campaign: my_first_run
  turn 1/14  … ✓
  turn 2/14  … ✓
  turn 3/14  … ✓
  ...
  turn 14/14 … ✓

Replay complete. 14/14 signals matched. 0 discrepancies.
```

If any signal differs from the recorded log — a different action, a different mutation, a different order — the replay reports a discrepancy with the turn number and expected vs actual values.

Replay is useful for:

- **Regression testing.** After code changes, replay a campaign and confirm the same signals are produced.
- **Debugging.** If a player reports a bug, replay their campaign to reproduce the exact sequence.
- **Audit.** Prove that a specific session played out as claimed.

### Graph Mode Replay

If you used graph mode (the `graph` subcommand with a `.graph.toml` file), replay works the same way:

```bash
cargo run --release -p mcl-cli -- registry replay my_first_run
```

The graph topology is recorded in the campaign metadata, so replay reconstructs the same node graph before feeding signals through it.

## Summary

You have:

1. Started a registry-backed campaign with `--campaign` and `--registry` flags.
2. Inspected the signal log with `registry inspect`.
3. Verified log integrity with `registry verify`.
4. Resumed a campaign across sessions — state, inventory, and memory persist automatically.
5. Queried the vector index semantically with `registry query --semantic`.
6. Replayed a campaign deterministically with `registry replay`.

The registry is the foundation for everything in Stage 2: summarization, scheduling, entity-scoped memory, and telemetry all build on top of the append-only signal log and its vector index.

Next: read [`docs/graph-toml-extensions.md`](graph-toml-extensions.md) to configure governing nodes, arbiter chains, and per-node memory persistence in your own graph files.
