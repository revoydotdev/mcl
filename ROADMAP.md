# MCL Development Roadmap

> **Current mandate (2026-07-07): Stage 3 — become the ultimate signal validation layer.**
> Milestones 1–11 below are the original Stage 1 plan (complete) and Stage 2 lives in
> [`STAGE-2-TODOS.md`](STAGE-2-TODOS.md) (registry/arbiter/memory, ~86% landed). Both are
> historical record — kept as-is, not rewritten. **Stage 3 (this section) is the governing
> direction going forward** and is what the `revoy` ledger block at the bottom of this file
> now tracks. See [`docs/comparison.md`](docs/comparison.md) and ADR-053 in `revoy-spec/decisions/`
> for the argument this stage executes on.

## Stage 3 — The Ultimate Signal Validation Layer (Mandate)

### Why this stage exists

An outside pass (2026-07-07, prompted by an assessment of MCL as base material against
LangChain's weakest flanks — abstraction tax, paywalled observability, and the model
vendors reclaiming the agent-orchestration layer) concluded:

- MCL **cannot win** on "less abstraction than a raw SDK" (Rust + a bespoke DSL is never
  going to out-simple `pip install`). Do not chase that fight.
- MCL's registry/arbiter/signal-log core — append-only, blake3 content-addressed,
  deterministic-replay, knowledge-boundary-scoped — is **already a stronger observability
  and provenance primitive than most agent frameworks bolt on after the fact.** This is the
  real differentiator and the one worth doubling down on.
- The `revoymcl` Python↔Rust bridge (`~/Dev/revoy-spec/revoymcl`) already proves this
  generalizes past interactive fiction: it is bridging MCL's arbiter to `revoy`'s own
  coding-agent governance (ADR-053), not to a game. That is the wedge — **prove it
  properly, then productize it.**
- The single biggest unresolved risk, per the 2026-04-22 signal-validation sweep
  (`docs/signal-validation-20260422/`), is that reliability was measured on **one 20-scenario
  RPG fixture** (`crypt_antechamber.mcl`) with results ranging 41.7%–98.3% across 9 models
  and no clean size correlation. That is evidence the core bet survives contact with real
  LLMs — it is not evidence the signal protocol generalizes to an arbitrary tool/action
  vocabulary. Closing that gap is Stage 3's critical path.

**The mandate:** stop treating the signal/arbiter/registry core as infrastructure for a
text-adventure runtime and start treating it as a standalone, embeddable, provider- and
framework-agnostic **validation layer** for LLM-emitted actions — installable under
anything (a coding agent, a LangGraph node, a raw SDK call, a compliance workflow) the same
way Guardrails or Instructor install under anything, except with real deterministic replay,
knowledge-boundary scoping, and content-addressed provenance behind it instead of a
schema-check-and-hope.

"Ultimate" is measurable, not marketing — a signal validation layer earns the word when it is:

1. **Domain-agnostic** — validates arbitrary typed actions, not RPG verbs.
2. **Provider-agnostic** — Ollama/OpenAI/Anthropic today; the interface must not assume any one's quirks.
3. **Framework-agnostic** — usable standalone, not only inside `mcl play`/`mcl graph`.
4. **Reliability-measured, not assumed** — every claim backed by a re-runnable battery, re-run on a cadence, not a one-time sweep.
5. **Adversarially tested** — proven to catch a model *trying* to emit an undeclared or hallucinated action, not just an accidental one.
6. **Generation-time-hardened where possible** — structured output / forced tool-use / grammar constraint as first-class modes, not an "if needed" fallback.

### Milestone 12: Generalize the Core Beyond RPG

**Why:** `mcl-spec`, `mcl-graph`, `mcl-registry`, and `mcl-runtime` are already domain-agnostic;
only `mcl-domains` (combat/inventory) and the example vocabulary couple the story to
interactive fiction. This milestone proves the split is real by extracting a path that
never imports `mcl-domains` at all.

**Phase 12.1 — Extract a standalone validation crate.**
Goal: a new `mcl-validate` crate (or a `--no-default-features` slice of `mcl-registry` +
`mcl-signal` + `mcl-spec`) that takes (a) a JSON-Schema-shaped action/tool declaration and
(b) a stream of LLM output, and returns accept/reject/quarantine verdicts with a
content-addressed log — with zero dependency on `mcl-graph`'s Pregel executor or any RPG
type. Done when: a `cargo run -p mcl-validate -- validate --schema tools.json --input
transcript.jsonl` command runs standalone, with no game-world concept anywhere in the call
path.
Difficulty: 55. Priority: VHIGH (everything else in Stage 3 sits on top of this).

**Phase 12.2 — Generic `ActionResolver` / tool-call trait.**
Goal: replace the RPG-specific `ActionResolver` trait (currently modeled on
combat/inventory resolution) with a generic `ToolResolver` that maps 1:1 onto the shape of
a JSON-schema tool/function definition (name, typed args, return contract) — so an MCP
tool list or an OpenAI/Anthropic tool-use schema can be loaded directly, with RPG resolvers
becoming just one more implementation of the trait rather than the trait's shape.
Difficulty: 45. Priority: HIGH.

**Phase 12.3 — Harden `revoymcl` from prototype to primary path.**
Goal: the PyO3 binding to the real Rust engine (`revoymcl/_rust`) stops being a
cross-check alongside the pure-Python fallback and becomes the default execution path,
with the Python layer demoted to (a) an offline fallback when the compiled extension isn't
available and (b) a differential test oracle. Extend `revoymcl/CONTRACT.md` to document
this as a committed interface, not a prototype note. Done when revoy's own agent-governance
integration (ADR-053 item 9) calls the PyO3 path in its default configuration.
Difficulty: 40. Priority: HIGH.

### Milestone 13: Reliability at the Frontier — Generation-Time Constraint

**Why:** Every serious competitor in the constrained-output space (Guidance, LMQL,
Outlines, XGrammar) solves reliability at the token level, not after the fact. MCL's own
`docs/comparison.md` concedes this is currently a real weakness. The April sweep's own
recommended follow-ups (structured-output re-run, retry-on-missing-signal, few-shot
tightening) were never executed at the time of writing — do them, and stop treating this
mode as a "Plan B."

**Phase 13.1 — Promote structured-output mode from fallback to first-class.**
Goal: `signal_mode = "structured"` (OpenAI JSON-schema `response_format` / `strict:true`
tools, Anthropic forced tool-use) becomes a fully supported, benchmarked, documented mode
on equal footing with inline signals — not gated behind "if inline reliability is below
95%." Re-run the April battery's bottom four models (Gemma-3-4B, Mistral Nemo, Claude 3.5
Haiku, GPT-4o-mini) under structured mode and confirm the sweep's own hypothesis ("≥20-point
jump into the 95% band").
Difficulty: 35. Priority: VHIGH.

**Phase 13.2 — GBNF/XGrammar grammar compilation for open models.**
Goal: compile the declared action schema directly to a GBNF grammar (llama.cpp/Ollama) and
an XGrammar-compatible grammar (vLLM/SGLang-served open models), so a malformed signal is
*unsamplable* rather than merely detected — the generation-time lever ADR-053 calls out as
current SOTA. Ties directly into Milestone 12.1's schema-first validation crate.
Difficulty: 60. Priority: HIGH.

**Phase 13.3 — Wire the already-recommended retry/few-shot hardening.**
Goal: close out the April sweep's own unactioned recommendations — verify
retry-on-missing-signal is actually wired into the live `LlmNode` (not just designed), and
diff the per-model missing-signal transcripts to strengthen few-shot examples for the
specific action types that clustered failures.
Difficulty: 20. Priority: MED.

### Milestone 14: Exhaustive Signal-Reliability Battery 2.0

**Why:** One RPG fixture, 20 scenarios, one snapshot in time is not "the ultimate
validation layer" — it's a promising pilot. This milestone is what actually earns the
claim.

**Phase 14.1 — Non-RPG benchmark corpus.**
Goal: build 3+ new benchmark domains mirroring real coding/tool-use agent action spaces
(e.g., filesystem-op vocabulary, REST-API-call vocabulary, financial-approval vocabulary)
with the same 20-scenario-×-N-run structure as the crypt sweep, run across the same 9-model
spread. This is the direct test of "does the signal protocol generalize past interactive
fiction," which is currently just asserted.
Difficulty: 50. Priority: VHIGH.

**Phase 14.2 — Adversarial battery.**
Goal: scenarios that actively try to get the arbiter to accept an undeclared or
hallucinated action — prompt-injection-flavored inputs, conflicting instructions, and
knowledge-boundary probing (extending the existing `rag_defense`/`phi_export_gate`
examples into a formal adversarial suite). Measure catch rate, not just accept rate.
Difficulty: 45. Priority: HIGH.

**Phase 14.3 — Quarterly re-run cadence, not a one-off.**
Goal: wire the full battery (14.1 + 14.2 + the original crypt sweep) to run on a recurring
cadence and publish a diffable result set each time, so "signal reliability" is a
continuously-verified property, not an April 2026 press release. This is the direct
consumer of the research monitors below (new model releases should trigger a re-run).
Difficulty: 25. Priority: MED.

### Milestone 15: Framework Adapters — Be the Layer Under Everyone

**Why:** If Milestone 12 succeeds, the validation core no longer needs `mcl play` to be
useful. This milestone proves it by making it useful *inside* the frameworks it isn't
trying to replace.

**Phase 15.1 — LangGraph adapter.**
Goal: a thin Python package exposing an MCL-arbiter-backed callback/checkpointer for
LangGraph tool nodes — every tool call LangGraph would execute gets validated through the
`mcl-validate` core first. This is the concrete, shippable answer to "MCL vs LangGraph"
that turns the rivalry framing in `docs/comparison.md` into a complementary one.
Difficulty: 40. Priority: HIGH.

**Phase 15.2 — Raw-SDK wrapper.**
Goal: a minimal OpenAI/Anthropic SDK wrapper matching the Instructor/Pydantic-AI onboarding
bar (a few lines, no framework buy-in) that runs every tool call through the arbiter. This
is deliberately NOT competing on "less abstraction" — it's competing on "the same amount of
abstraction, but the calls are now provably validated and logged."
Difficulty: 30. Priority: MED.

**Phase 15.3 — `revoy` as the flagship integration.**
Goal: close out ADR-053's build order using the now-hardened `revoymcl` (Milestone 12.3):
generation-time constraint via MCP tool schemas, PreToolUse action-space restriction,
provenance-as-type on `!verified`, and the capability-token layer — proving the validation
layer's value on revoy's own real coding-agent governance problem before pitching it to
anyone else.
Difficulty: 65. Priority: HIGH.

### Milestone 16: Publish, Prove, Promote

**Phase 16.1 — Publish for real.** `mcl-validate` (and the supporting crates it depends
on) to crates.io; `revoymcl` to PyPI. No more "0.1.0 everywhere, nothing shipped."
Difficulty: 20. Priority: MED.

**Phase 16.2 — Public, regenerated leaderboard.** Turn `MODELS.md` + the Milestone 14
battery results into a generated, dated compatibility/reliability matrix — a page a
prospective user can check before committing, refreshed by the Milestone 14.3 cadence.
Difficulty: 20. Priority: MED.

**Phase 16.3 — Reposition, honestly.** Update `docs/comparison.md`'s verdicts to add the
validation-layer identity alongside the existing "use LangGraph for general agent
workflows" framing — the honest claim is "MCL is not your agent framework; MCL is what
proves your agent framework's tool calls are real," and the doc should say that plainly
rather than only disclaiming general-purpose use.
Difficulty: 10. Priority: LOW.

### Research workstream — two standing monitors (post-cutoff backgrounding)

Stage 3's mandate depends on facts that age fast: constrained-decoding SOTA, provider
structured-output API changes, and what LangGraph/Guardrails/Instructor/Langfuse ship next.
Rather than inventing an ad hoc research process, this wires directly into `revoy`'s
existing purpose-built substrate for exactly this (`revoymonitor`, kind=`intel`) — two
CODE-scoped monitor definitions, each backed by a real probe, each producing dated findings
an agent can act on rather than a stale assumption baked into this roadmap:

1. **`mcl-decoding-sota`** — tracks constrained-decoding / structured-output / verifier-guided
   generation research (feeds Milestones 13 and 14's technical approach):
   ```
   revoy intel add --id mcl-decoding-sota --scope MCL --cadence weekly \
     --probe-primitive arxiv_query \
     --probe-param query="grammar-constrained decoding LLM structured output" \
     --assessor "Flag any paper/result that changes the recommended approach for \
   Milestone 13 (structured-output promotion, GBNF/XGrammar compilation) or reveals a \
   faster/more reliable generation-time constraint technique than what Stage 3 currently plans."
   ```

2. **`mcl-agent-tooling-intel`** — tracks competing frameworks' releases and provider
   structured-output API changes (feeds Milestone 15/16 positioning and `MODELS.md`
   currency):
   ```
   revoy intel add --id mcl-agent-tooling-intel --scope MCL --cadence weekly \
     --probe-primitive gh_search \
     --probe-param query="langgraph OR guardrails-ai OR instructor OR langfuse OR pydantic-ai release" \
     --assessor "Flag any release, API change, or benchmark from a competing framework or \
   model provider (esp. OpenAI/Anthropic structured-output changes) that affects MCL's \
   competitive positioning (docs/comparison.md) or the MODELS.md compatibility matrix."
   ```

Both are registered `enabled=true`, weekly cadence, scoped to `MCL` — run via `revoy monitor
run` (or the loop's own cadence once armed) and surfaced via `revoy monitor findings MCL`.
Findings that invalidate an assumption in this roadmap should become a `revoy todo MCL
"..."` against whichever Stage 3 milestone they affect, not silently absorbed.

---

# MCL Development Roadmap — 11 Milestones (Stage 1, historical — complete)

## Outside Evaluation Summary (2026-04-12)

An independent evaluation of MCL identified one **existential risk** that must be addressed before any other work: the signal protocol has never been validated against real LLMs. The entire architecture assumes LLMs will reliably emit `MCL::*` signals inline with narration, but all testing to date uses `MockLlmBackend`. If signal reliability is below ~95% across major models, the architecture needs fundamental revision.

**Key findings:**
- MCL is **genuinely novel at the architectural level** — the signal protocol, knowledge boundaries, and constraint compilation are real innovations with no direct competitor
- The **interactive fiction market is too small** to sustain a product — EdTech/compliance training simulations are the strongest commercial pivot
- A senior engineer could replicate **70-80% of MCL's functionality** on LangGraph in 2-3 weeks — the moat is the design decisions, not the code
- **Zero playtesting exists** — unknown if any LLM actually respects signals, constraints, or knowledge boundaries in practice
- The original milestones 5-10 optimized for **technical completeness** rather than **product-market fit**

**The roadmap below has been restructured.** Milestone 5 (Signal Protocol Validation) is new and sits on the critical path. The original milestones 5-10 have been renumbered and reordered to front-load commercial viability concerns.

---

## How to Use This Document

Each milestone has 3-8 phases. Phases are sequential within a milestone. Milestones 1-4 are sequential; 5 is the new critical-path gate. Milestones 6-11 can be parallelized after Milestone 5.

**Per-phase format:**
- **Goal** — what this phase delivers
- **TDD** — tests to write first, before implementation
- **Implementation** — what to build
- **LLM Coder Notes** — reminders for AI-assisted development specific to this phase
- **Syntax/Examples** — code samples where helpful
- **Done When** — exit criteria

---

## Milestone 1: Bulletproof the Foundation

**Why:** The current codebase has 50+ unwrap() in production paths, 3 panic!() in signal parsing, zero integration tests, and no CI. Nothing else matters until the existing code is trustworthy.

### Phase 1.1: Eliminate All Production Panics ✅

**Goal:** Zero unwrap()/panic!() in non-test code paths.

**TDD:** Write tests that feed malformed input to every public function. Each test should prove the function returns Err instead of panicking.

```rust
// tests in mcl-signal/src/parse.rs
#[test]
fn malformed_action_signal_returns_error() {
    let result = parse_signal("MCL::action(");
    assert!(result.is_err());
}

#[test]
fn empty_mutate_signal_returns_error() {
    let result = parse_signal("MCL::mutate()");
    assert!(result.is_err());
}
```

**Implementation:**
- Audit every .rs file for `unwrap()`, `expect()`, `panic!()` outside `#[cfg(test)]`
- Replace with `ok_or(MclError::...)`, `map_err`, or `?` propagation
- Key files: `mcl-signal/src/parse.rs` (3 panic!), `mcl-graph/src/executor.rs` (~10 unwrap), `mcl-runtime/src/session.rs` (2 unwrap), `mcl-lang/src/emit.rs` (panics on unexpected AST)

**LLM Coder Notes:**
- Do NOT add new error variants unless the existing `MclError` variants can't express the failure. Check `mcl-spec/src/error.rs` first.
- Do NOT change function signatures unless forced to — many return Result already.
- Grep for `unwrap()` and `panic!` across all crates excluding test modules. Work file-by-file, not crate-by-crate.
- When replacing unwrap(), use the same error style already in the file. Don't invent new patterns.

**Done When:** `grep -rn 'unwrap\(\)\|\.expect(\|panic!' crates/*/src/*.rs | grep -v '#\[cfg(test)\]' | grep -v '#\[test\]'` returns zero hits (manually filter test blocks).

---

### Phase 1.2: Integration Test Harness

**Goal:** A test that proves the full loop: parse MCL file → build state → send to mock LLM → extract signals → mutate state → verify.

**TDD:** Write the integration test first. It will fail because there's no mock LLM backend.

```rust
// tests/integration/full_loop.rs
#[tokio::test]
async fn full_turn_loop() {
    let doc = mcl_parse::parse_document(CRYPT_MCL).unwrap();
    let mock_llm = MockLlmBackend::new(vec![
        "The skeleton guard lunges forward!\nMCL::action(attack, target=player)"
    ]);
    let mut session = Session::new(Box::new(mock_llm));
    session.load_context("crypt", doc);
    session.activate_context("crypt").unwrap();

    let result = session.turn("attack skeleton").await.unwrap();
    assert!(!result.narration.is_empty());
    assert!(!result.resolutions.is_empty());
}
```

**Implementation:**
1. Create `mcl-llm/src/mock.rs` — `MockLlmBackend` that returns canned responses in sequence
2. Export it behind `#[cfg(test)]` or a `test-support` feature flag
3. Create `tests/` directory at workspace root
4. Write 3 integration tests: happy path, malformed signal handling, context transition

**LLM Coder Notes:**
- The mock backend must implement `LlmBackend` trait from `mcl-llm/src/backend.rs`. Check that trait's exact signature before implementing.
- Use the existing example files at `examples/crypt/crypt_antechamber.mcl` as test fixtures — read them with `include_str!()`.
- The `Session` struct is in `mcl-runtime/src/session.rs`. Its `turn()` method is async and returns `Result<TurnResult, MclError>`.
- Don't mock more than necessary. The test should use real parsing, real signal extraction, real state mutation — only the LLM call is mocked.

**Done When:** `cargo test --workspace` passes with at least 3 new integration tests.

---

### Phase 1.3: CI Pipeline

**Goal:** Every push runs `cargo check`, `cargo test`, `cargo clippy`, `cargo fmt --check`.

**TDD:** N/A — infrastructure phase.

**Implementation:**
Create `.github/workflows/ci.yml`:
```yaml
name: CI
on: [push, pull_request]
jobs:
  check:
    runs-on: self-hosted
    steps:
      - uses: actions/checkout@v4
      - run: cargo fmt --check
      - run: cargo clippy --workspace -- -D warnings
      - run: cargo test --workspace
```

**LLM Coder Notes:**
- The user has a self-hosted runner (labels: `self-hosted, linux, x64`). Use it.
- Rust toolchain is already installed on the runner. Do NOT add `actions-rs/toolchain` or similar.
- Keep the workflow minimal. No caching, no matrix builds. Just the 3 checks.
- Clippy with `-D warnings` will catch the dead code warnings in mcl-memory and mcl-lang. Fix those first or add targeted `#[allow()]` attributes.

**Done When:** CI badge is green on push.

---

### Phase 1.4: Fix Clippy Warnings

**Goal:** `cargo clippy --workspace -- -D warnings` passes clean.

**TDD:** Clippy is the test.

**Implementation:**
- `mcl-memory/src/store.rs`: remove or use the `dimensions` field
- `mcl-lang/src/lib.rs`: remove unused `MclFile` import
- Fix any other warnings Clippy surfaces

**LLM Coder Notes:**
- Do NOT blanket `#[allow(clippy::*)]`. Fix the actual issue.
- If removing the `dimensions` field breaks anything, grep for its usage first. It's reportedly never read, so removing it should be safe.
- Run `cargo clippy --workspace 2>&1` and fix warnings one-by-one. Don't batch unrelated changes.

**Done When:** `cargo clippy --workspace -- -D warnings` exits 0.

---

### Phase 1.5: Test Coverage for Untested Crates

**Goal:** Every crate has at least basic tests. Target: mcl-llm, mcl-nodes, mcl-cli, mcl-domains, mcl-memory, mcl-spec.

**TDD:** Write tests for the public API surface of each untested crate.

```rust
// mcl-spec - test value conversions
#[test]
fn mcl_value_display() {
    assert_eq!(MclValue::Int(42).to_string(), "42");
    assert_eq!(MclValue::Bool(true).to_string(), "true");
}

// mcl-domains - test resolver registration
#[test]
fn rpg_resolvers_register() {
    let mut registry = ResolverRegistry::new();
    register_rpg_resolvers(&mut registry);
    assert!(registry.get("combat_system").is_some());
    assert!(registry.get("inventory_system").is_some());
}

// mcl-nodes - test node factory
#[test]
fn default_nodes_register() {
    let mut factory = NodeFactory::new();
    register_default_nodes(&mut factory);
    // verify all 5 node types registered
}
```

**Implementation:**
- mcl-spec: test Display impls, serde round-trips, FieldPath construction
- mcl-llm: test MockLlmBackend (from Phase 1.2), test ChatMessage construction
- mcl-nodes: test register_default_nodes, test node factory lookup
- mcl-domains: test resolver registration and basic resolve() calls
- mcl-memory: test MemoryStore insert/search/evict cycle
- mcl-cli: skip (integration tested via Phase 1.2)

**LLM Coder Notes:**
- mcl-memory tests require fastembed model download on first run. Use `#[ignore]` attribute if the model isn't available in CI, or mock the embedder.
- For mcl-nodes, you can't easily test `process()` without a full graph context. Test construction and capability reporting instead.
- Focus on "does it not panic with valid input" and "does it return Err with invalid input". Don't test internal logic exhaustively yet.

**Done When:** Every crate in the workspace has at least 2 tests. `cargo test --workspace` shows tests in all 11 crates.

---

## Milestone 2: Complete the Crypt Demo

**Why:** One complete, polished, fully playable demo is worth more than 28 theoretical use cases. The crypt dungeon adventure should be the proof that MCL works.

### Phase 2.1: Combat System — Real Mechanics ✅

**Goal:** CombatNode resolves attacks with d20 rolls, AC checks, HP tracking, damage, and death.

**TDD:**
```rust
#[test]
fn attack_roll_hits_when_exceeding_ac() {
    let mut state = WorldState::new();
    state.set("entities.skeleton_guard.hp", MclValue::Int(10));
    state.set("entities.skeleton_guard.ac", MclValue::Int(12));
    state.set("player.attack_bonus", MclValue::Int(5));

    // Mock a roll of 15 + 5 = 20 > AC 12 → hit
    let resolution = combat_resolve_with_roll(&state, "attack", "skeleton_guard", 15);
    assert_eq!(resolution.outcome, ResolutionOutcome::Success);
}

#[test]
fn entity_dies_at_zero_hp() {
    let mut state = WorldState::new();
    state.set("entities.skeleton_guard.hp", MclValue::Int(1));
    let resolution = apply_damage(&mut state, "skeleton_guard", 5);
    assert_eq!(state.get("entities.skeleton_guard.hp"), Some(&MclValue::Int(0)));
    assert!(resolution.details.contains("defeated"));
}
```

**Implementation:**
- Expand `mcl-nodes/src/combat.rs` with real d20 mechanics
- Add `mcl-domains/src/rpg/combat.rs` resolver with: attack rolls, AC comparison, damage dice, HP reduction, death at 0
- The combat node should accept `MCL::action(attack, target=X)` signals and return Resolution with outcome + narration hints

**LLM Coder Notes:**
- The existing `CombatNode` in `mcl-nodes/src/combat.rs` already does basic d20 rolls. Expand it, don't rewrite.
- `CombatResolver` in `mcl-domains/src/rpg/combat.rs` implements the `ActionResolver` trait from `mcl-runtime/src/resolver.rs`. Check that trait signature.
- Use `rand::thread_rng()` for dice. For tests, accept a seed or roll value as parameter to make tests deterministic.
- Don't over-engineer. D20 + modifier vs AC + damage is enough. No initiative, no spell slots, no multiattack yet.

**Done When:** You can play the crypt demo, attack the skeleton guard, and see HP decrease with d20 mechanics. Guard dies at 0 HP.

---

### Phase 2.2: Inventory System — Real Items ✅

**Goal:** InventoryNode tracks items the player picks up, drops, uses. Items have properties defined in .mcl context files.

**TDD:**
```rust
#[test]
fn pickup_item_adds_to_player_inventory() {
    let mut state = WorldState::new();
    state.set("items.rusty_key.location", MclValue::String("floor".into()));
    let resolution = inventory_resolve(&mut state, "pickup", "rusty_key");
    assert_eq!(resolution.outcome, ResolutionOutcome::Success);
    // item moved to player inventory
}

#[test]
fn pickup_nonexistent_item_fails() {
    let state = WorldState::new();
    let resolution = inventory_resolve(&state, "pickup", "unicorn");
    assert_eq!(resolution.outcome, ResolutionOutcome::Failure);
}
```

**Implementation:**
- Expand `mcl-nodes/src/inventory.rs` — handle pickup, drop, use, examine
- Track inventory as a list in player state: `player.inventory = ["rusty_key", "torch"]`
- Item visibility conditions from `ItemDef.visible_when` in mcl-spec

**LLM Coder Notes:**
- `ItemDef` is defined in `mcl-spec/src/document.rs`. It has `name`, `location`, and `visible_when` fields.
- Player inventory doesn't exist in WorldState yet. You'll need to initialize it as `player.inventory = []` and add list manipulation helpers to WorldState in `mcl-runtime/src/state.rs`.
- The `MclValue::List` variant already exists for this.

**Done When:** You can pick up the rusty key in the crypt, check inventory, and use items.

---

### Phase 2.3: Context Transitions — Moving Between Rooms ✅

**Goal:** `MCL::transition(crypt_inner)` signals cause the game to switch contexts, loading the new .mcl file and updating state.

**TDD:**
```rust
#[tokio::test]
async fn transition_signal_switches_context() {
    let mock_llm = MockLlmBackend::new(vec![
        "The door creaks open.\nMCL::transition(crypt_inner)"
    ]);
    let mut session = Session::new(Box::new(mock_llm));
    session.load_context("crypt_antechamber", parse_file("examples/crypt/crypt_antechamber.mcl"));
    session.load_context("crypt_inner", parse_file("examples/crypt/crypt_inner.mcl"));
    session.activate_context("crypt_antechamber").unwrap();

    let result = session.turn("go north").await.unwrap();
    assert_eq!(result.transitioned_to, Some("crypt_inner".to_string()));
}
```

**Implementation:**
- Wire `TransitionDef` conditions into the session turn loop
- When `MCL::transition(target)` signal is extracted, validate preconditions, switch active context, rebuild system prompt
- Carry forward persistent state (player HP, inventory) across transitions

**LLM Coder Notes:**
- `TransitionDef` is in `mcl-spec/src/document.rs` — it has `target`, `direction`, and `requires` fields.
- The `Session.turn()` method in `mcl-runtime/src/session.rs` already has a `transitioned_to` field in `TurnResult`. It may already partially handle this.
- The critical subtlety: WorldState must persist player-scoped and global-scoped state across transitions, but local state and entity state should reset to the new context's definitions.

**Done When:** You can walk from the antechamber to the inner crypt and back. State persists across transitions.

---

### Phase 2.4: System Prompt Quality ✅

**Goal:** The system prompt generated from .mcl context files is good enough that LLMs actually respect the constraints.

**TDD:**
```rust
#[test]
fn system_prompt_includes_knowledge_boundaries() {
    let doc = parse_document(include_str!("../../examples/crypt/crypt_antechamber.mcl")).unwrap();
    let prompt = build_system_prompt(&doc, &WorldState::new());
    assert!(prompt.contains("You know:") || prompt.contains("knows:"));
    assert!(prompt.contains("You do not know:") || prompt.contains("ignores:"));
}

#[test]
fn system_prompt_includes_narrator_directives() {
    let doc = parse_document(include_str!("../../examples/crypt/crypt_antechamber.mcl")).unwrap();
    let prompt = build_system_prompt(&doc, &WorldState::new());
    // Check tone, voice, forbidden words are present
    assert!(prompt.contains("tone:") || prompt.contains("Tone:"));
}
```

**Implementation:**
- Review and improve `mcl-runtime/src/prompt.rs` — `build_system_prompt()`
- Ensure all context sections are represented: narrator directives, knowledge boundaries, available actions with preconditions, current state, entity descriptions
- Test with actual LLM calls (manual QA) — does the LLM stay in character? Does it respect forbidden words?

**LLM Coder Notes:**
- `build_system_prompt()` is in `mcl-runtime/src/prompt.rs`. Read it first — it may already handle most of this.
- The prompt quality directly determines whether MCL's value proposition works. This is the most important phase for user-facing quality.
- System prompt should be structured with clear sections: Role, Setting, Rules, Available Actions, Current State, Knowledge Boundaries.
- Include the signal protocol format in the system prompt so the LLM knows to emit `MCL::action(...)` signals.

**Done When:** Playing the crypt demo with Ollama or OpenAI, the LLM consistently: stays in narrator voice, respects forbidden words, emits valid MCL signals, doesn't reveal hidden information.

---

### Phase 2.5: Polish the Example .mcl Files

**Goal:** `crypt_antechamber.mcl` and `crypt_inner.mcl` are complete, well-commented, and demonstrate every MCL feature.

**TDD:** Validate both files parse and validate clean:
```rust
#[test]
fn example_files_parse_and_validate() {
    for file in &["examples/crypt/crypt_antechamber.mcl", "examples/crypt/crypt_inner.mcl"] {
        let doc = mcl_parse::parse_file(Path::new(file)).unwrap();
        // No validation errors
    }
}
```

**Implementation:**
- Ensure both files use: state with typed constraints, entities with knowledge boundaries, items with visibility conditions, actions with preconditions, transitions with conditions, narrator directives with tone/voice/forbidden
- Add inline comments explaining MCL syntax for new users reading the examples
- Add a third room (e.g., `crypt_treasury.mcl`) to demonstrate a branching path

**LLM Coder Notes:**
- Read both existing .mcl files first. They may already be complete.
- Comments in .mcl files use `#` prefix (check `mcl-parse/src/lexer.rs` — `LineKind::Comment`).
- The examples are the project's front door. They should be self-documenting. A developer reading crypt_antechamber.mcl should understand MCL syntax without reading any other documentation.

**Done When:** A developer unfamiliar with MCL can read the examples and understand the language. Three rooms form a playable adventure.

---

### Phase 2.6: Playtest and Fix

**Goal:** Play the complete crypt adventure end-to-end. Fix every bug encountered.

**TDD:** Each bug found becomes a regression test before fixing.

**Implementation:**
- Play with at least 2 LLM backends (Ollama + one API)
- Document every failure: signal parsing errors, state corruption, prompt issues, transition bugs
- Fix each with test-first approach
- Minimum 10 complete playthroughs

**LLM Coder Notes:**
- This is a manual QA phase. The AI coder's role is fixing bugs reported from playtesting, not doing the playtesting.
- For each bug report, first write a test that reproduces it (using MockLlmBackend with the exact LLM output that caused the failure), then fix.
- Common failure modes: LLM doesn't emit signals in correct format, LLM ignores knowledge boundaries, state mutation applies to wrong scope, transition fails silently.

**Done When:** 10 consecutive playthroughs complete without crashes or state corruption. Filed bugs have regression tests.

---

## Milestone 3: The Language is the Product

**Why:** If MCL's bespoke language can't justify itself against YAML+Python, the project has no moat. The language needs to be pleasant to write, well-documented, and demonstrably better than alternatives.

### Phase 3.1: Language Reference Documentation ✅

**Goal:** A complete language reference showing every MCL construct with examples.

**TDD:** Every syntax example in the reference is also a parser test.

```rust
// For each example in the language reference, add a parse test:
#[test]
fn reference_example_state_section() {
    let source = r#"
---
context: example_room
type: room
---

## state
hp: int = 100
name: str = "hero"
alive: bool = true
"#;
    let doc = parse_document(source).unwrap();
    assert_eq!(doc.state.len(), 3);
}
```

**Implementation:**
Write `docs/language-reference.md` covering:
1. File structure (frontmatter, sections)
2. Frontmatter fields (context, type, domain, tags, extends)
3. Section types: state, entities, items, actions, transitions, narrator, knowledge, constraints
4. Value types: int, float, str, bool, list, map
5. Condition syntax: `field > value`, `field`, `!field`
6. Constraint tags: `<tone>`, `<affect>`, `<voice>`, `<constraint>`, `<practical>`
7. Signal protocol: `MCL::action(...)`, `MCL::mutate(...)`, etc.
8. Graph file syntax (for mcl-lang)

**LLM Coder Notes:**
- Read `mcl-lang/src/parser.rs` and `mcl-parse/src/document.rs` to understand what syntax is actually supported vs what's aspirational.
- Every example must parse. If you write a syntax example that doesn't parse, either the example is wrong or the parser needs fixing.
- Don't document features that don't work yet. Mark unimplemented features as "Planned" with a clear label.

**Done When:** `docs/language-reference.md` exists, every example in it has a corresponding parser test, all tests pass.

---

### Phase 3.2: Error Messages — Developer Experience ✅

**Goal:** When an .mcl file has a syntax error, the error message includes file, line, column, the offending line, and a suggestion.

**TDD:**
```rust
#[test]
fn error_reports_line_and_column() {
    let bad_mcl = "---\ncontext: test\n---\n\n## state\nhp: int = not_a_number\n";
    let err = parse_document(bad_mcl).unwrap_err();
    let msg = format!("{}", err);
    assert!(msg.contains("line 6")); // or however your spans work
}

#[test]
fn error_suggests_fix_for_common_mistakes() {
    let bad_mcl = "---\ncontext: test\ntype: dungeon\n---\n"; // invalid type
    let err = parse_document(bad_mcl).unwrap_err();
    let msg = format!("{}", err);
    assert!(msg.contains("room") || msg.contains("scene")); // suggests valid types
}
```

**Implementation:**
- Enhance `MclError` Display impl in `mcl-spec/src/error.rs`
- Use `Span` from `mcl-spec/src/span.rs` to include source location
- Add suggestion text for common errors (wrong type names, missing required fields, unclosed sections)
- Consider using `ariadne` or `miette` crate for pretty error output

**LLM Coder Notes:**
- The `Span` struct already exists in `mcl-spec/src/span.rs`. Check if it's actually populated during parsing — it may be defaulted/zeroed.
- `miette` is the Rust standard for pretty diagnostic output. It integrates with `thiserror`. Evaluate whether the added dependency is worth it.
- Don't over-engineer. Clear text errors are fine. Fancy colored output is a nice-to-have, not a requirement.

**Done When:** A typo in a .mcl file produces an error message that tells you exactly where the problem is and how to fix it.

---

### Phase 3.3: mcl-lang Constraint Tags — Full Implementation ✅

**Goal:** All constraint tag types compile to useful system prompt directives.

**TDD:**
```rust
#[test]
fn tone_tag_compiles_to_directive() {
    let result = compile_constraints_to_prompt(
        &[], 
        &[ConstraintTag { tag_type: ConstraintTagType::Tone, content: "ominous".into(), .. }]
    );
    assert!(result.contains("ominous"));
    assert!(result.contains("tone") || result.contains("Tone"));
}
```

**Implementation:**
- Review `mcl-lang/src/constraints.rs` — `parse_constraint_tags()` and `compile_constraints_to_prompt()`
- Ensure all tag types produce meaningful LLM instructions: tone, affect, practical, voice, constraint
- Test with LLM: do the compiled directives actually affect output?

**LLM Coder Notes:**
- `ConstraintTag` and `ConstraintTagType` are defined in `mcl-lang/src/constraints.rs`. Read the existing implementation first.
- The constraint system is one of MCL's most unique features. It should produce prompt text that demonstrably changes LLM behavior.
- Don't just pass through the tag content as-is. Compile it into structured, imperative instructions: "You MUST maintain an ominous tone throughout your narration."

**Done When:** Each constraint tag type produces a distinct, testable prompt directive. Manual testing confirms LLMs respond differently with and without constraints.

---

### Phase 3.4: Import/Extends System ✅

**Goal:** MCL files can extend other files: `extends: base_room` inherits state, entities, narrator directives from a base.

**TDD:**
```rust
#[test]
fn extended_context_inherits_state() {
    let base = parse_document("---\ncontext: base\ntype: room\n---\n## state\nhp: int = 100\n").unwrap();
    let child = parse_document("---\ncontext: child\ntype: room\nextends: base\n---\n## state\nmp: int = 50\n").unwrap();
    let merged = merge_contexts(&base, &child);
    assert!(merged.state.contains_key("hp")); // inherited
    assert!(merged.state.contains_key("mp")); // own
}
```

**Implementation:**
- The `Frontmatter` already has an `extends` field (check `mcl-spec/src/document.rs`)
- Implement `merge_contexts()` in `mcl-runtime/src/context.rs` — child overrides parent, parent fills gaps
- Wire into `ContextManager.load_context()`

**LLM Coder Notes:**
- Check if `Frontmatter.extends` is already populated by the parser. It may be parsed but ignored at runtime.
- Merge strategy: child sections override parent sections. Within sections, child fields override parent fields. Lists are replaced, not appended.
- This is a common footgun in template inheritance systems. Keep it simple: shallow merge, no recursive inheritance chains in v1.

**Done When:** A child .mcl file with `extends: base` inherits all sections from the base, with child overrides taking priority.

---

### Phase 3.5: LSP or Syntax Highlighting

**Goal:** MCL files get syntax highlighting in at least one editor.

**TDD:** N/A — tooling phase.

**Implementation:**
Pick ONE:
- **Option A (faster):** TextMate grammar (.tmLanguage.json) for VS Code. Covers keywords, sections, frontmatter, constraint tags, signals.
- **Option B (harder, more valuable):** Basic LSP server using `tower-lsp` crate. Parse on save, report diagnostics.

Recommend Option A for now.

**LLM Coder Notes:**
- TextMate grammars are JSON files with regex patterns. The MCL lexer in `mcl-parse/src/lexer.rs` already defines the line categories — translate those to TextMate scopes.
- Key scopes: `---` frontmatter delimiters, `## section` headers, `key: value` pairs, `# comments`, `MCL::signal()` patterns, `<tag>` constraint tags.
- Ship as a VS Code extension in a `editors/vscode/` directory. Package.json + tmLanguage.json is all you need.

**Done When:** Opening a .mcl file in VS Code shows colored syntax highlighting.

---

## Milestone 4: Graph Execution — Prove Pregel Works

**Why:** The Pregel-style graph executor is MCL's strongest technical differentiator. It needs to demonstrably work with the full crypt demo, not just unit tests.

### Phase 4.1: Graph Mode End-to-End ✅

**Goal:** `mcl graph examples/crypt/crypt_adventure.graph.toml` runs a complete game session.

**TDD:**
```rust
#[tokio::test]
async fn graph_mode_runs_crypt_adventure() {
    let def = TomlGraphDefParser.parse_file("examples/crypt/crypt_adventure.graph.toml").unwrap();
    let mut factory = NodeFactory::new();
    register_default_nodes(&mut factory);
    // Use mock LLM
    factory.register_fn("llm", |ctx| Box::new(LlmNode::new(Box::new(MockLlmBackend::new(...)), "")));
    let mut executor = GraphExecutor::build(def, &factory).unwrap();
    // Run one super-step
    executor.step().await.unwrap();
}
```

**Implementation:**
- Wire the CLI `graph` subcommand to actually work end-to-end
- Ensure `GraphExecutor.run_interactive()` handles the stdin→LLM→display loop
- Verify packets flow correctly: client → game_engine → llm → game_engine → combat/inventory → game_engine → client

**LLM Coder Notes:**
- The `GraphExecutor` is in `mcl-graph/src/executor.rs`. Read `run_interactive()` and `run_until_quiescent()`.
- Node registration happens in `mcl-nodes/src/lib.rs` — `register_default_nodes()`. Check that all 5 node types match the names used in `crypt_adventure.graph.toml`.
- The .graph.toml file at `examples/crypt/crypt_adventure.graph.toml` defines node names and edge connections. Verify these match the factory's registered names.
- Packet routing is in `mcl-graph/src/channel.rs`. If packets aren't arriving at the right nodes, check channel names in the .graph.toml match the subscriptions/publications in each node's `NodeCapability`.

**Done When:** `cargo run -- graph examples/crypt/crypt_adventure.graph.toml --backend ollama` runs a playable game.

---

### Phase 4.2: Determinism Proof ✅

**Goal:** Run the same inputs through the graph twice and get identical state mutations (excluding LLM output).

**TDD:**
```rust
#[tokio::test]
async fn deterministic_execution_with_same_inputs() {
    let inputs = vec!["look around", "attack skeleton", "pick up key"];
    let mock_responses = vec!["...", "...", "..."];

    let state1 = run_session_with_inputs(&inputs, &mock_responses).await;
    let state2 = run_session_with_inputs(&inputs, &mock_responses).await;

    assert_eq!(state1.changelog, state2.changelog);
    assert_eq!(state1.get("player.hp"), state2.get("player.hp"));
}
```

**Implementation:**
- Seed RNG for combat rolls (accept seed in config)
- Ensure WorldState changelog records mutations in deterministic order
- Use MockLlmBackend to eliminate the one source of non-determinism

**LLM Coder Notes:**
- The `rand` crate in Cargo.toml is version 0.8. Use `StdRng::seed_from_u64()` for deterministic RNG.
- Thread the RNG through the combat node. Currently `CombatNode` likely uses `thread_rng()` — replace with a seeded RNG stored in the node's state.
- This test is MCL's key differentiator proof. It should be prominent in the test suite and referenced in documentation.

**Done When:** The determinism test passes 100 times consecutively.

---

### Phase 4.3: Graph Visualization ✅

**Goal:** Export the running graph state to DOT format for Graphviz visualization.

**TDD:**
```rust
#[test]
fn graph_exports_to_dot() {
    let def = TomlGraphDefParser.parse_file("examples/crypt/crypt_adventure.graph.toml").unwrap();
    let dot = export_dot(&def);
    assert!(dot.contains("digraph"));
    assert!(dot.contains("client"));
    assert!(dot.contains("->"));
}
```

**Implementation:**
- Add `export_dot(def: &GraphDef) -> String` to `mcl-graph`
- Include node names, types, and edge labels (channel names)
- Add `mcl dot <file.graph.toml>` CLI subcommand

**LLM Coder Notes:**
- DOT format is simple: `digraph G { a -> b [label="channel"]; }`. No external crate needed.
- `GraphDef` is in `mcl-graph/src/graph_def.rs`. It has `nodes: IndexMap<String, NodeDef>` and `edges: Vec<EdgeDef>`.
- This is a quick win for documentation and debugging.

**Done When:** `mcl dot examples/crypt/crypt_adventure.graph.toml | dot -Tpng -o graph.png` produces a readable graph image.

---

### Phase 4.4: Super-Step Debugging

**Goal:** A `--debug` flag that prints every packet routed during each super-step.

**TDD:**
```rust
#[tokio::test]
async fn debug_mode_logs_packets() {
    // Capture log output during graph execution
    // Verify that each super-step shows: step number, packets sent, packets received, state changes
}
```

**Implementation:**
- Add structured logging with the `tracing` crate to `GraphExecutor`
- Log: super-step number, which nodes activated, packets routed (source→dest, channel, payload summary), state mutations
- `--debug` flag on CLI enables `TRACE` level

**LLM Coder Notes:**
- The project already depends on `log` + `env_logger`. You can either migrate to `tracing` (recommended, more structured) or stay with `log`.
- If staying with `log`: use `log::trace!()` for per-packet routing, `log::debug!()` for per-step summaries.
- Don't log full packet payloads — they can be huge (LLM responses). Log the first 80 chars + length.

**Done When:** Running with `--debug` shows a clear trace of execution: which nodes ran, what they sent, where it went.

---

### Phase 4.5: Error Recovery in Graph Execution

**Goal:** When a node fails mid-step, the graph executor logs the error, skips the failed node, and continues.

**TDD:**
```rust
#[tokio::test]
async fn graph_continues_after_node_failure() {
    // Register a node that always returns Err
    // Run graph with failing node + healthy nodes
    // Verify healthy nodes still process correctly
    // Verify error is logged
}
```

**Implementation:**
- In `GraphExecutor`, wrap each `node.process()` call in error handling
- On failure: log error with node name and step number, skip node, continue step
- Add a `max_consecutive_failures` threshold per node — disable a node if it fails N times in a row

**LLM Coder Notes:**
- Check `mcl-graph/src/executor.rs` — the current error handling may already do some of this.
- The unwrap() calls you removed in Milestone 1 should have been replaced with `?` propagation. Now you're adding a layer above that: catch errors per-node rather than per-executor.
- Use `Result::unwrap_or_else(|e| { log::error!(...); })` pattern at the per-node boundary.

**Done When:** A failing LLM backend (network timeout) doesn't crash the entire graph. The game reports an error and lets the player retry.

---

## Milestone 5: Signal Protocol Validation (EXISTENTIAL)

**Why:** MCL's entire architecture depends on LLMs reliably emitting `MCL::action(attack, target=skeleton_guard)` in the correct format inline with narration. There is zero enforcement at the decoding level. If the LLM emits a malformed signal, narrates an action without a signal, or invents signal types, the game state silently diverges from the narrative. Every competing approach (Guidance, Outlines, JSON structured output) solves this at the constraint level. MCL relies on prompt engineering alone. **This is the single biggest technical risk to the project.** Nothing else in milestones 6-11 matters until signal reliability is measured and proven.

### Phase 5.1: Real LLM Playtest Battery

**Goal:** Run 100+ turns across GPT-4, Claude, and Llama 3 (via Ollama) using the crypt demo. Measure signal emission reliability.

**TDD:**
```rust
// Not unit tests — this phase produces a measurement harness
// tests/signal_reliability/harness.rs
struct ReliabilityReport {
    model: String,
    total_turns: usize,
    correct_signals: usize,      // well-formed, expected type
    malformed_signals: usize,     // MCL:: prefix but wrong format
    missing_signals: usize,       // narrated action but no signal emitted
    hallucinated_signals: usize,  // signal for action not in available list
    accuracy: f64,
}
```

**Implementation:**
- Create `tools/signal_bench.rs` (or a `--benchmark` mode in mcl-cli)
- Script 20 scenarios with known expected signal types (e.g., "attack the skeleton" should produce `MCL::action(attack, ...)`)
- Run each scenario 5 times per model (100+ total turns)
- Classify each response: correct signal, malformed signal, missing signal, hallucinated signal
- Output a reliability report per model

**LLM Coder Notes:**
- Use the existing `mcl play` infrastructure but capture output instead of displaying it
- The existing signal parser in `mcl-signal/src/extract.rs` already classifies signals — extend it to also report "no signal found when one was expected"
- Test with at least: Ollama (llama3:8b, mistral), OpenAI (gpt-4o), Anthropic (claude-sonnet)
- The system prompt from `mcl-runtime/src/prompt.rs` is the primary lever — if signals are unreliable, the fix starts there

**Done When:** A reliability report exists for 3+ models. The accuracy numbers determine whether Phase 5.2 or 5.3 is needed.

---

### Phase 5.2: Signal Reliability Hardening

**Goal:** If Phase 5.1 shows accuracy below 95%, harden the signal protocol without abandoning the inline approach.

**TDD:**
```rust
#[tokio::test]
async fn retry_on_missing_signal() {
    // Scenario: player says "attack skeleton", LLM narrates the attack but emits no signal
    // System should detect missing signal and re-prompt with explicit signal request
    let mock = MockLlmBackend::new(vec![
        "The skeleton's jaw creaks open as you swing your sword!", // no signal
        "MCL::action(attack, target=skeleton_guard)\nYou strike true!", // retry succeeds
    ]);
    let result = session.turn("attack skeleton").await.unwrap();
    assert!(!result.resolutions.is_empty());
}

#[test]
fn signal_format_examples_in_prompt() {
    let prompt = build_system_prompt(&doc, &state, &world);
    // Prompt should include concrete signal examples, not just format description
    assert!(prompt.contains("MCL::action(attack, target=skeleton_guard)"));
}
```

**Implementation:**
- Add few-shot signal examples to the system prompt (not just format description — actual examples with the current context's actions)
- Add a "signal expected" detection heuristic: if the player's input maps to a known action and the LLM response contains no signal, re-prompt once with an explicit nudge: "Your response should include an MCL:: signal for the action taken."
- Add signal validation: if a signal references an action not in the available actions list, log a warning and optionally re-prompt
- Cap retries at 1 to avoid infinite loops

**LLM Coder Notes:**
- The system prompt builder is in `mcl-runtime/src/prompt.rs`. The signal format section currently describes the format abstractly — add concrete examples using the current context's actual actions.
- The retry logic belongs in `mcl-runtime/src/session.rs` in the `turn()` method, after signal extraction.
- Do NOT implement structured output (JSON mode) yet — that's Phase 5.3 if needed. This phase stays within the inline signal paradigm.

**Done When:** Re-run the Phase 5.1 benchmark. Accuracy should be >95% for GPT-4 and Claude. If not, proceed to Phase 5.3.

---

### Phase 5.3: Structured Output Fallback (If Needed)

**Goal:** If inline signals remain unreliable (<95% after hardening), implement a hybrid approach: structured JSON output for game mechanics, freeform text for narration.

**TDD:**
```rust
#[tokio::test]
async fn structured_output_mode() {
    // LLM returns JSON with narration + signals separated
    let mock_response = r#"{
        "narration": "The skeleton guard lunges forward!",
        "signals": [
            {"type": "action", "name": "attack", "target": "skeleton_guard"}
        ]
    }"#;
    let mock = MockLlmBackend::new(vec![mock_response.to_string()]);
    let result = session.turn("attack skeleton").await.unwrap();
    assert!(!result.narration.is_empty());
    assert!(!result.resolutions.is_empty());
}
```

**Implementation:**
- Add a `signal_mode` config: `inline` (current, default) or `structured` (JSON)
- In `structured` mode, the system prompt requests a JSON response with `narration` and `signals` fields
- Use the model's JSON mode / structured output if available (OpenAI `response_format`, Anthropic tool use)
- The signal parser gets a new path: `extract_from_json()` alongside the existing `extract_from_text()`
- Graph config: `[nodes.narrator.config] signal_mode = "structured"`

**LLM Coder Notes:**
- This is a Plan B, not a replacement. The inline signal approach is more elegant and produces better narration. But if models can't reliably emit signals inline, this fallback ensures the game works.
- OpenAI's JSON mode and Anthropic's tool use both provide reliable structured output. Ollama models are less reliable with JSON — may need to stay in inline mode for local models.
- The structured output approach sacrifices some narration quality (LLM must think in JSON structure) but gains near-100% signal reliability.

**Done When:** Structured output mode achieves >99% signal accuracy. Both modes are supported and configurable.

---

## Milestone 6: Memory System Integration

**Why:** Per-node vector memory is a differentiator claimed in the architecture. It currently exists as disconnected code. Wire it in or remove it.

### Phase 5.1: Memory Store Tests

**Goal:** Prove MemoryStore works: insert, search by similarity, evict.

**TDD:**
```rust
#[tokio::test]
async fn memory_store_insert_and_search() {
    let store = MemoryStore::new(384); // MiniLM dimension
    let embedder = create_embedder(&EmbeddingConfig::default());
    store.insert("The skeleton guard attacks!", &embedder).await;
    store.insert("You find a rusty key on the floor.", &embedder).await;
    
    let results = store.search("combat", &embedder, 1).await;
    assert_eq!(results[0].text, "The skeleton guard attacks!");
}

#[test]
fn memory_evicts_oldest_when_full() {
    // Insert capacity + 1 entries
    // Verify oldest is evicted
}
```

**Implementation:**
- Fix the unused `dimensions` field warning in `mcl-memory/src/store.rs`
- Write comprehensive tests for MemoryStore CRUD
- Test the chunker variants (ParagraphChunker, SignalChunker, WholePacketChunker)

**LLM Coder Notes:**
- fastembed downloads ~80MB model on first use. CI tests should use `#[ignore]` or a mock embedder.
- `MemoryStore` uses `usearch::Index` for HNSW. The `dimensions` field should match the embedding model dimension (384 for all-MiniLM-L6-v2).
- If the store tests are flaky due to embedding similarity thresholds, use a wider threshold. Embedding search is approximate.

**Done When:** MemoryStore tests pass, including insert/search/evict lifecycle.

---

### Phase 5.2: Wire Memory into LlmNode

**Goal:** LlmNode stores conversation history in vector memory and retrieves relevant context for each turn.

**TDD:**
```rust
#[tokio::test]
async fn llm_node_retrieves_relevant_memory() {
    // Play 5 turns discussing combat
    // Play 3 turns discussing treasure
    // On turn 9, ask about "the skeleton fight"
    // Verify the system prompt includes retrieved memory about combat, not treasure
}
```

**Implementation:**
- Add `NodeMemory` to `LlmNode` in `mcl-nodes/src/llm.rs`
- After each turn, store the conversation exchange in memory
- Before each turn, retrieve top-K relevant memories and inject into system prompt
- Make memory optional (off by default, enabled in .graph.toml config)

**LLM Coder Notes:**
- `NodeMemory` is in `mcl-memory/src/node_memory.rs`. It wraps an embedder + MemoryStore.
- The `LlmNode` already manages conversation history as a Vec<ChatMessage>. Memory augments this — don't replace the explicit history, supplement it.
- Memory retrieval should go in the system prompt's "relevant context" section, clearly delineated from the current context's rules and state.
- Config in .graph.toml: `[nodes.narrator.config] memory_enabled = true, memory_capacity = 100`

**Done When:** A 20-turn conversation references events from turn 3 accurately via memory retrieval.

---

### Phase 5.3: Wire Memory into GameEngineNode

**Goal:** GameEngineNode stores state change history in vector memory for auditing and context.

**TDD:**
```rust
#[tokio::test]
async fn game_engine_memory_tracks_state_changes() {
    // Make 10 state mutations
    // Search memory for "skeleton" 
    // Verify it returns the skeleton-related mutations
}
```

**Implementation:**
- Add `NodeMemory` to `GameEngineNode`
- Store each state mutation as a memory entry with tags (scope, field path, old value, new value)
- Expose a `recall(query, k)` method for other nodes to query the game engine's memory

**LLM Coder Notes:**
- `GameEngineNode` is in `mcl-nodes/src/game_engine.rs`. It manages WorldState.
- Each state mutation already goes through WorldState's set/get methods. Add memory insertion at the mutation point.
- Tag memory entries with the scope (global/player/entity/local) for filtered retrieval.

**Done When:** The game engine's memory can answer "what happened to the skeleton guard?" with relevant state change history.

---

### Phase 5.4: Memory Persistence

**Goal:** Memory survives session restart — save to disk, load on start.

**TDD:**
```rust
#[test]
fn memory_round_trips_through_disk() {
    let store = MemoryStore::new(384);
    // Insert entries
    store.save("test_memory.bin").unwrap();
    let loaded = MemoryStore::load("test_memory.bin").unwrap();
    assert_eq!(loaded.len(), store.len());
}
```

**Implementation:**
- Add serde serialization to MemoryEntry and MemoryStore
- Save/load to a binary file (bincode or MessagePack)
- usearch Index has its own save/load API — use it for the HNSW index
- Wire into CLI: `--memory-dir <path>` flag

**LLM Coder Notes:**
- usearch's Index has `.save(path)` and `Index::load(path)` methods. Use these for the vector index.
- Store the metadata (MemoryEntry structs without vectors) separately in a serde-compatible format.
- Two files per memory store: `{name}.usearch` (index) + `{name}.meta.bin` (entries).

**Done When:** Quit the game, restart, and memory recalls events from the previous session.

---

## Milestone 7: Multi-Backend Robustness

**Why:** MCL supports 3 LLM backends but none are tested. Real-world usage will hit API quirks, rate limits, and format differences.

### Phase 6.1: Backend Response Normalization ✅

**Goal:** All backends return responses in the same format regardless of API quirks.

**TDD:**
```rust
#[tokio::test]
async fn ollama_backend_handles_empty_response() {
    let backend = OllamaBackend::new("nonexistent-model");
    let result = backend.complete("system", &[], "hello").await;
    assert!(result.is_err()); // not a panic
}

#[tokio::test]
async fn anthropic_response_strips_thinking_tags() {
    // Anthropic sometimes returns <thinking>...</thinking> with extended thinking
    // MCL should strip these before signal extraction
}
```

**Implementation:**
- Test each backend with: empty response, error response, very long response, response with unexpected formatting
- Normalize all responses: trim whitespace, strip provider-specific artifacts
- Return `MclError` on API errors, never panic

**LLM Coder Notes:**
- These tests require network access or recorded responses (vcr-style). For CI, use recorded responses.
- Consider `wiremock` crate for HTTP mocking — all three backends use `reqwest` under the hood.
- Each backend is in its own file: `mcl-llm/src/ollama.rs`, `openai.rs`, `anthropic.rs`.

**Done When:** Each backend has at least 5 tests covering error cases. All pass in CI.

---

### Phase 6.2: Streaming Support ✅

**Goal:** LLM responses stream token-by-token to the client for real-time display.

**TDD:**
```rust
#[tokio::test]
async fn streaming_yields_partial_responses() {
    let backend = MockStreamingBackend::new("Hello world");
    let mut stream = backend.stream("system", &[], "hi").await;
    let mut chunks = Vec::new();
    while let Some(chunk) = stream.next().await {
        chunks.push(chunk);
    }
    assert!(chunks.len() > 1); // got multiple chunks
    assert_eq!(chunks.join(""), "Hello world");
}
```

**Implementation:**
- Add `stream()` method to `LlmBackend` trait (or a separate `StreamingBackend` trait)
- Implement SSE parsing for OpenAI and Anthropic backends
- Ollama already supports streaming via its `/api/generate` endpoint
- Signal extraction happens on the *complete* response, not mid-stream

**LLM Coder Notes:**
- This changes the `LlmBackend` trait in `mcl-llm/src/backend.rs`. Consider adding a default method that falls back to non-streaming: `async fn stream(...) -> impl Stream<Item=String> { stream::once(self.complete(...)) }`
- Use `futures::Stream` or `tokio_stream::Stream` for the return type.
- The critical design decision: signal extraction must wait for the full response. Buffer the stream, display incrementally, extract signals at the end.

**Done When:** `mcl play --backend openai` shows tokens appearing one by one, and signals are still extracted correctly from the full response.

---

### Phase 6.3: Retry and Rate Limiting ✅

**Goal:** Failed API calls retry with exponential backoff. Rate limits are respected.

**TDD:**
```rust
#[tokio::test]
async fn retries_on_transient_failure() {
    let backend = MockFailThenSucceedBackend::new(2, "success"); // fails twice, then succeeds
    let result = backend.complete("system", &[], "hi").await;
    assert_eq!(result.unwrap(), "success");
}
```

**Implementation:**
- Wrap each backend's HTTP call with retry logic (max 3 attempts, exponential backoff)
- Parse `Retry-After` headers from 429 responses
- Log retries at `warn` level

**LLM Coder Notes:**
- Don't add a retry crate. A simple loop with `tokio::time::sleep()` is sufficient for 3 retries.
- Exponential backoff: 1s, 2s, 4s. Cap at 3 attempts.
- Rate limit (429) and server error (500, 502, 503) are retryable. Client errors (400, 401, 403) are not.

**Done When:** Tests prove retry works. Manual test: unplug network briefly during a game session — it recovers.

---

## Milestone 8: Save/Load — Persistence Layer

**Why:** Every use case beyond throwaway demos needs save/restore. This is the #1 missing feature for any real deployment.

### Phase 7.1: WorldState Serialization

**Goal:** WorldState round-trips through serde (serialize and deserialize without data loss).

**TDD:**
```rust
#[test]
fn world_state_round_trips() {
    let mut state = WorldState::new();
    state.set("player.hp", MclValue::Int(85));
    state.set("entities.skeleton.hp", MclValue::Int(0));
    state.set("global.room_visited", MclValue::Bool(true));
    
    let json = serde_json::to_string(&state).unwrap();
    let loaded: WorldState = serde_json::from_str(&json).unwrap();
    
    assert_eq!(loaded.get("player.hp"), Some(&MclValue::Int(85)));
    assert_eq!(loaded.get("entities.skeleton.hp"), Some(&MclValue::Int(0)));
}
```

**Implementation:**
- Add `Serialize`/`Deserialize` derives to WorldState and all nested types
- `MclValue` already has serde — verify it round-trips correctly for all variants
- Include changelog in serialization

**LLM Coder Notes:**
- Check `mcl-runtime/src/state.rs` — WorldState likely uses `HashMap<String, MclValue>` for each scope. These should serialize naturally with serde.
- The `FieldPath` type in `mcl-spec/src/expression.rs` may need custom serde if it's not already derived.
- Test with JSON for debugging, but consider MessagePack for production saves (smaller, faster).

**Done When:** WorldState serde round-trip test passes with all value types.

---

### Phase 7.2: Session Save/Load

**Goal:** `mcl play --save session.json` saves state on quit. `mcl play --load session.json` resumes.

**TDD:**
```rust
#[tokio::test]
async fn session_save_and_load() {
    let mut session = create_test_session().await;
    session.turn("attack skeleton").await.unwrap();
    session.save("test_session.json").unwrap();
    
    let loaded = Session::load("test_session.json", Box::new(mock_llm)).unwrap();
    assert_eq!(
        loaded.state().get("entities.skeleton.hp"),
        session.state().get("entities.skeleton.hp")
    );
}
```

**Implementation:**
- Add `save()` and `load()` methods to Session
- Save: WorldState + active context name + conversation history + changelog
- Load: deserialize, re-parse context files, apply saved state over defaults
- CLI flags: `--save <path>` (auto-saves on quit), `--load <path>` (resume)

**LLM Coder Notes:**
- Session is in `mcl-runtime/src/session.rs`. Add `save(&self, path: &Path) -> Result<(), MclError>` and `load(path: &Path, llm: Box<dyn LlmBackend>) -> Result<Self, MclError>`.
- The LLM backend can't be serialized — it must be provided fresh on load. Save the backend config (model name, endpoint) as metadata.
- Conversation history (`Vec<ChatMessage>`) should be saved for LLM context continuity.

**Done When:** Save mid-game, restart, load — game continues exactly where you left off.

---

### Phase 7.3: Audit Log

**Goal:** Every state mutation is logged to an append-only file for compliance and debugging.

**TDD:**
```rust
#[test]
fn audit_log_records_mutations() {
    let mut state = WorldState::new();
    state.set("player.hp", MclValue::Int(100));
    state.set("player.hp", MclValue::Int(85));
    
    let log = state.changelog();
    assert_eq!(log.len(), 2);
    assert_eq!(log[1].field, "player.hp");
    assert_eq!(log[1].old_value, Some(MclValue::Int(100)));
    assert_eq!(log[1].new_value, MclValue::Int(85));
}
```

**Implementation:**
- WorldState already has a changelog concept — formalize it
- Each entry: timestamp, field path, old value, new value, source (which node/signal caused it)
- Write to file incrementally (append, never rewrite)
- CLI flag: `--audit-log <path>`

**LLM Coder Notes:**
- Check if WorldState already tracks changelog entries. If so, this is mostly about persisting them to disk.
- Use JSONL (one JSON object per line) for the audit log format — easy to parse, append-friendly, grep-friendly.
- Don't buffer — write and flush each entry immediately. This is an audit log; data loss on crash is unacceptable.

**Done When:** After a game session, the audit log file contains every state mutation with timestamps.

---

## Milestone 9: Multi-LLM and Multi-Persona

**Why:** The writers' room, wargaming, and many other use cases need multiple LLM personas in one graph.

### Phase 8.1: Multiple LlmNode Instances

**Goal:** A graph can have multiple LlmNode instances, each with different system prompts and potentially different backends.

**TDD:**
```rust
#[tokio::test]
async fn graph_with_two_llm_nodes() {
    let def = parse_graph(r#"
        [nodes.narrator]
        type = "llm"
        [nodes.narrator.config]
        persona = "gothic narrator"
        
        [nodes.advisor]
        type = "llm"  
        [nodes.advisor.config]
        persona = "tactical advisor"
    "#);
    // Both nodes should have different system prompts
    // Both should process independently in the same super-step
}
```

**Implementation:**
- LlmNode already accepts a system prompt — verify it can be instantiated multiple times with different configs
- Update `register_default_nodes` to support per-instance config
- Ensure each LlmNode has its own conversation history

**LLM Coder Notes:**
- `LlmNode::from_config(ctx: &NodeContext)` in `mcl-nodes/src/llm.rs` reads config from NodeContext. Verify it reads persona/system_prompt from the node's config section.
- The NodeFactory in `mcl-graph/src/factory.rs` uses type names to create nodes. Multiple nodes of the same type should work — they just share the factory constructor but get different configs.
- Don't create a new node type for each persona. Same LlmNode, different config.

**Done When:** Two LLM nodes in one graph produce different-voiced responses to the same input.

---

### Phase 8.2: Per-Node Knowledge Boundaries

**Goal:** Each LLM node sees only the state it's allowed to see, based on its knowledge configuration.

**TDD:**
```rust
#[test]
fn knowledge_filtered_state() {
    let full_state = WorldState::with_data(/* all game state */);
    let narrator_knowledge = Knowledge { knows: vec!["room.*"], ignores: vec!["player.hidden_items"] };
    let filtered = filter_state_for_knowledge(&full_state, &narrator_knowledge);
    assert!(filtered.get("room.description").is_some());
    assert!(filtered.get("player.hidden_items").is_none());
}
```

**Implementation:**
- Add `Knowledge` struct with `knows` and `ignores` glob patterns
- Implement `filter_state_for_knowledge()` that returns a view of WorldState visible to a specific node
- Wire into LlmNode's system prompt construction

**LLM Coder Notes:**
- `NarratorDef` in mcl-spec already has knowledge-related fields. Check the `knowledge` section in MclDocument.
- Glob matching on field paths: `player.*` matches all player fields, `entities.skeleton.*` matches skeleton fields.
- This is the feature MCL claims as its killer differentiator. Make it work well.

**Done When:** An LLM node with `ignores: ["player.inventory"]` never mentions items the player hasn't discovered yet.

---

### Phase 8.3: Turn-Taking Protocol

**Goal:** In multi-LLM graphs, nodes take turns rather than all responding simultaneously.

**TDD:**
```rust
#[tokio::test]
async fn turn_taking_alternates_llm_nodes() {
    // Graph with narrator + advisor
    // On step 1: narrator speaks
    // On step 2: advisor responds to narrator
    // Verify ordering
}
```

**Implementation:**
- Add `turn_order` config to graph definition
- Graph executor respects turn order within super-steps
- Default: all nodes process simultaneously (current behavior). With turn order: sequential within a step.

**LLM Coder Notes:**
- This could be as simple as a `priority` field on NodeDef. Nodes with lower priority run first within a super-step.
- Or: split a super-step into sub-phases based on turn order. Phase 1 nodes run, their output becomes input for phase 2 nodes.
- Keep it simple. Priority ordering within a step is sufficient for v1.

**Done When:** A 2-LLM graph produces coherent dialogue where the second LLM responds to the first.

---

## Milestone 10: WebSocket Client and Web UI

**Why:** A stdio REPL is developer tooling. A web client makes MCL accessible.

### Phase 9.1: WebSocket ClientNode

**Goal:** Replace stdin/stdout with a WebSocket connection.

**TDD:**
```rust
#[tokio::test]
async fn websocket_client_sends_and_receives() {
    let (client, server) = create_test_ws_pair().await;
    client.send("hello").await;
    let received = server.recv().await;
    assert_eq!(received, "hello");
}
```

**Implementation:**
- New node: `WebSocketClientNode` in `mcl-nodes/src/ws_client.rs`
- Uses `tokio-tungstenite` for WebSocket
- Implements the same `Node` trait as `ClientNode`
- Config: `host`, `port`

**LLM Coder Notes:**
- The `ClientNode` in `mcl-nodes/src/client.rs` reads stdin and writes stdout. The WebSocket variant replaces those I/O channels with a WebSocket connection.
- Copy the structure of `ClientNode`, swap stdin/stdout for ws send/recv.
- Register as a new node type in `register_default_nodes()`: `"ws_client"`.

**Done When:** A WebSocket client (wscat, browser JS) can play the crypt adventure.

---

### Phase 9.2: HTTP Server Wrapper

**Goal:** An HTTP server that accepts WebSocket connections and spins up a graph per session.

**TDD:**
```rust
#[tokio::test]
async fn http_server_accepts_connections() {
    let server = start_test_server(3456).await;
    let client = connect_ws("ws://localhost:3456/play").await;
    client.send("look around").await;
    let response = client.recv().await;
    assert!(!response.is_empty());
}
```

**Implementation:**
- New crate: `mcl-server` using `axum` + `tokio-tungstenite`
- Routes: `GET /play` (WebSocket upgrade), `GET /health`
- Each WebSocket connection creates a new graph executor with a WebSocketClientNode
- CLI: `mcl serve --port 8080 --graph examples/crypt/crypt_adventure.graph.toml`

**LLM Coder Notes:**
- Keep the server minimal. Axum is already well-suited for this.
- Each connection gets its own graph executor, its own WorldState, its own session. No shared state between connections (that's Milestone 10).
- Don't build auth yet. Just serve the graph.

**Done When:** `mcl serve` starts, a browser connects via WebSocket, and the game is playable.

---

### Phase 9.3: Minimal Web Frontend

**Goal:** A single HTML page that connects to the MCL server and displays the game.

**TDD:** Manual QA — open in browser, play the game.

**Implementation:**
- `static/index.html` — single file, no build step
- WebSocket connection to the server
- Text display area (scrolling output)
- Input field at the bottom
- Optional: basic CSS for a terminal aesthetic

**LLM Coder Notes:**
- Serve the static file from axum: `axum::routing::get_service(ServeFile::new("static/index.html"))`
- No React, no build tools. Vanilla HTML + JS + CSS. Under 200 lines.
- The WebSocket protocol: client sends text strings (user input), server sends text strings (narration + signals formatted as display text).

**Done When:** Open `http://localhost:8080` in a browser, play the crypt adventure through to completion.

---

## Milestone 11: Competitive Positioning and Documentation

**Why:** Technical merit means nothing without clear positioning, honest documentation, and developer-accessible resources.

### Phase 10.1: Competitive Analysis — Honest Version

**Goal:** A document that honestly compares MCL to LangGraph, LMQL, Guidance, DSPy, and Rivet.

**Implementation:**
Build the crypt dungeon (or a simpler example) in LangGraph. Compare:
- Lines of code
- Readability
- Determinism (can you replay?)
- Knowledge boundary expressiveness
- Setup complexity

Write `docs/comparison.md` with side-by-side code samples.

**LLM Coder Notes:**
- This is research, not code. The AI coder should help write the LangGraph equivalent and the comparison document.
- Be honest. If LangGraph is better at something, say so. Credibility comes from honesty.
- The comparison should be runnable — reader can clone both versions and compare directly.

**Done When:** A developer can read `docs/comparison.md` and understand exactly where MCL is better, where it's worse, and why they might choose it.

---

### Phase 10.2: README Rewrite

**Goal:** The README accurately reflects what MCL *is today*, not what it aspires to be.

**Implementation:**
Structure:
1. One-paragraph pitch
2. Quick start (install, run the demo)
3. 30-second syntax example
4. What MCL does well (with evidence)
5. What MCL doesn't do yet (honest roadmap)
6. Comparison to alternatives (link to docs/comparison.md)

**LLM Coder Notes:**
- Kill all aspirational language. Replace "MCL supports 28 use cases across 9 domains" with "MCL powers interactive fiction with LLM narration. Here's a playable demo."
- The quick start must work in under 5 minutes. Test it on a clean machine (or fresh checkout).
- Include a GIF or asciinema recording of the crypt demo.

**Done When:** A developer who has never heard of MCL can go from README to playing the demo in under 5 minutes.

---

### Phase 10.3: Architecture Doc — Grounded Version

**Goal:** ARCHITECTURE.md describes the system as built, not as designed.

**Implementation:**
- Remove or clearly label all "Planned" features
- Add architecture diagrams generated from the actual graph (Phase 4.3's DOT export)
- Document actual crate dependencies (not aspirational ones)
- Include the determinism proof results (Phase 4.2)

**LLM Coder Notes:**
- Read the current ARCHITECTURE.md first. Keep everything that's accurate, clearly label everything that's aspirational.
- Use a section header pattern: "## Implemented" vs "## Planned" to make the distinction unmissable.

**Done When:** Every claim in ARCHITECTURE.md is either demonstrably true (with a test or running code that proves it) or explicitly labeled as planned.

---

### Phase 10.4: Tutorial — Build Your Own MCL Game

**Goal:** A step-by-step tutorial that walks a developer through creating a new MCL game from scratch.

**Implementation:**
Write `docs/tutorial.md`:
1. Install MCL
2. Create a .mcl context file for a single room
3. Run it with `mcl play`
4. Add entities, items, actions
5. Add a second room with transitions
6. Create a .graph.toml for graph mode
7. Add combat resolution
8. Add knowledge boundaries

Each step should be runnable. Each step builds on the previous.

**LLM Coder Notes:**
- Every code block must be copy-pasteable and work. Test each step independently.
- This is the most important document for adoption. The tutorial IS the product for most developers.
- Reference the language reference (Phase 3.1) for detailed syntax, don't duplicate it.

**Done When:** A developer with no MCL experience can follow the tutorial and build a working 2-room game in under 30 minutes.

---

### Phase 10.5: Publish and Announce

**Goal:** MCL is installable via `cargo install` and discoverable.

**Implementation:**
- Publish to crates.io (or prepare for publishing)
- Create a project website (even a GitHub Pages single-page)
- Write an announcement post: "MCL: A Language for LLM-Constrained Games"
- Post to: r/rust, r/gamedev, Hacker News, Rust community Discord

**LLM Coder Notes:**
- Before publishing, ensure all crate metadata is complete: `[package]` in each Cargo.toml needs description, license, repository, keywords.
- `cargo publish --dry-run` to verify everything is ready.
- The announcement should lead with the demo, not the architecture. Show, don't tell.

**Done When:** `cargo install mcl-cli` works. The demo is one command away.

---

## Milestone Priority Matrix

| Milestone | Priority | Depends On | Estimated Effort | Notes |
|-----------|----------|------------|-----------------|-------|
| 1. Bulletproof Foundation | CRITICAL | Nothing | 1-2 weeks | ✅ Complete |
| 2. Complete Crypt Demo | CRITICAL | M1 | 2-3 weeks | ✅ Complete (playtest FAIL) |
| 3. Language is Product | HIGH | M1 | 2-3 weeks | ✅ Complete |
| 4. Prove Pregel Works | HIGH | M2 | 1-2 weeks | ✅ Complete |
| **5. Signal Protocol Validation** | **EXISTENTIAL** | **M4** | **1-3 weeks** | **NEW — blocks everything** |
| 6. Memory Integration | MEDIUM | M1 | 1-2 weeks | ✅ Memory wired into LlmNode and GameEngineNode |
| 7. Multi-Backend | MEDIUM | M5 | 1 week | ✅ Complete (normalize, streaming, retry) |
| 8. Save/Load | HIGH | M2 | 1 week | |
| 9. Multi-LLM | MEDIUM | M4 | 1-2 weeks | |
| 10. Web Client | LOW | M4, M8 | 2 weeks | ✅ WebSocket server and frontend exist |
| 11. Documentation | CRITICAL | M5 | 1-2 weeks | Depends on signal validation results |

**Critical path:** M1 → M2 → M4 → **M5** → M11

M5 is the existential gate. If signal reliability is below 95%, the architectural approach must be revised (Phase 5.3) before any other milestone delivers real value. M6-10 are parallelizable after M5.

### Commercial Viability Path (Beyond M11)

The current milestones optimize for technical completeness. For commercial viability, the following should follow M11:

| Future Milestone | Why |
|---|---|
| **Game engine integration** (Unity/Godot plugin) | Without this, MCL requires devs to build their own bridge. With it, MCL drops into existing pipelines. |
| **Non-game vertical demo** (EdTech/compliance training) | The interactive fiction market is too small (~200 studios). Training simulation is a $2-5B market. |
| **Visual editor** (web-based, like Twine) | Non-technical content authors need this. Without it, MCL stays an engineer's tool. |
| **Cloud-hosted runtime** (`mcl serve` as managed service) | Per-session pricing is the scalable business model. |

---

## revoy ledger block

Machine-readable current phase for the revoy cross-project ledger. Mirrors the current outstanding items above; keep in sync when items land.

<!-- revoy:begin -->
```toml
phase = "Stage 3 — Ultimate Signal Validation Layer (M12: generalize the core)"

[[todo]]
line = "M12.1: extract standalone mcl-validate crate (schema+transcript in, verdict+log out, zero mcl-graph/RPG deps)"
difficulty = 55
priority = "VHIGH"

[[todo]]
line = "M13.1: promote structured-output signal_mode to first-class; re-run bottom-4-model sweep under it"
difficulty = 35
priority = "VHIGH"

[[todo]]
line = "M14.1: non-RPG benchmark corpus (filesystem/API/approval action vocabularies) to test generalization past crypt_antechamber"
difficulty = 50
priority = "VHIGH"

[[todo]]
line = "Register mcl-decoding-sota + mcl-agent-tooling-intel revoy intel monitors (weekly, CODE=MCL) for post-cutoff research backgrounding"
difficulty = 10
priority = "HIGH"

[[todo]]
line = "M2.12 publish readiness (carried over, still open): per-crate Cargo.toml metadata + cargo publish --dry-run green"
difficulty = 15
priority = "HIGH"

[[todo]]
line = "M12.3: harden revoymcl PyO3 path to primary (not fallback); revoy ADR-053 item 9 calls it by default"
difficulty = 40
priority = "HIGH"
```
<!-- revoy:end -->
