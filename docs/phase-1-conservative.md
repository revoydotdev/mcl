# MCL Phase 1: Niche Open-Source Tool

**Goal**: Ship a complete, honest, well-tested interactive fiction engine to crates.io. Close every gap between claim and reality. Build 2-3 non-RPG domains. Deploy a WASM browser demo.

**Timeline**: 2-4 weeks
**Cost**: ~$5 in API calls for eval/playtest runs
**Success metric**: Published on crates.io, browser demo live, 3+ domains, all tests green, knowledge filter wired in, determinism proven for all paths.

---

## Quality Gates

Every task must pass ALL gates before marking complete:

1. `cargo test --workspace` — 0 failures
2. `cargo clippy --workspace -- -D warnings` — 0 warnings
3. `cargo fmt --check` — 0 diffs
4. New code has tests written BEFORE implementation (TDD)
5. No `unwrap()` or `expect()` in production code (test code is fine)
6. Every new public function has at least one test

---

## Task 1.1: Wire Knowledge Filter into Prompt Builder

**Why**: `filter_state_for_knowledge()` exists in `crates/mcl-runtime/src/knowledge_filter.rs` with 6 passing tests, but is never called. The LLM sees full state. This is the biggest claim/reality gap.

**TDD — Write tests first:**

```rust
// crates/mcl-runtime/src/prompt.rs (add to existing test module)
#[test]
fn system_prompt_excludes_unknown_knowledge_from_state() {
    let (doc, mut state) = antechamber();
    state.set("secret_passage", MclValue::Bool(true)).unwrap();
    let prompt = build_system_prompt(&doc, &state);
    // secret_passage is in the !secret_passage knowledge boundary
    assert!(
        !prompt.contains("secret_passage"),
        "prompt should NOT contain secret_passage — got:\n{prompt}"
    );
}

#[test]
fn system_prompt_includes_known_knowledge_in_state() {
    let (doc, mut state) = antechamber();
    state.set("guards_tomb", MclValue::Bool(true)).unwrap();
    let prompt = build_system_prompt(&doc, &state);
    assert!(
        prompt.contains("guards_tomb"),
        "prompt should contain guards_tomb — got:\n{prompt}"
    );
}
```

**Implementation:**

In `crates/mcl-runtime/src/prompt.rs`, in `build_prompt_preamble()`, replace:
```rust
// BEFORE (line 30-36):
if !state.local.is_empty() {
    prompt.push_str("## Current State\n");
    for (key, val) in &state.local {
        prompt.push_str(&format!("- {key}: {val}\n"));
    }
}

// AFTER:
let filtered = crate::knowledge_filter::filter_state_for_knowledge(state, &doc.knowledge);
if !filtered.local.is_empty() {
    prompt.push_str("## Current State\n");
    for (key, val) in &filtered.local {
        prompt.push_str(&format!("- {key}: {val}\n"));
    }
}
```

Apply the same `filtered` state to the entities section below it.

**Files**: `crates/mcl-runtime/src/prompt.rs`
**Verify**: Tests pass. Run eval to confirm no accuracy regression.

---

## Task 1.2: Seed RNG for Full Determinism Proof

**Why**: Determinism tests only cover noop/mutate paths. Combat uses `thread_rng()` which is not seeded. The claim "deterministic execution" needs to hold for ALL paths.

**TDD — Write test first:**

```rust
// crates/mcl-runtime/tests/determinism.rs (add new test)
#[tokio::test]
async fn deterministic_session_combat_path() {
    let combat_response = "You strike!\nMCL::action(attack, target=skeleton_guard)";
    let inputs = ["attack the skeleton"];

    let first_state = run_session(&inputs, vec![combat_response.to_string()]).await;

    for i in 0..100 {
        let state = run_session(&inputs, vec![combat_response.to_string()]).await;
        assert_eq!(
            state.get_by_str("entities.skeleton_guard.hp"),
            first_state.get_by_str("entities.skeleton_guard.hp"),
            "iteration {i}: combat HP mismatch"
        );
    }
}
```

**Implementation:**

In `crates/mcl-domains/src/rpg/combat.rs`, the `resolve()` method takes `&mut dyn RngCore`. The session needs to pass a seeded `StdRng` instead of `thread_rng()`.

1. Add `rng: StdRng` field to `Session` in `crates/mcl-runtime/src/session.rs`
2. Initialize with `StdRng::seed_from_u64(42)` (or configurable seed)
3. Pass `&mut self.rng` to resolver calls instead of `&mut thread_rng()`

**Files**: `crates/mcl-runtime/src/session.rs`, `crates/mcl-domains/src/rpg/combat.rs` (if RNG interface needs adjustment)
**Verify**: New determinism test passes 100 iterations.

---

## Task 1.3: Run 10 Full LLM Playthroughs

**Why**: Zero real playthroughs exist. This surfaces bugs that benchmarks miss.

**Steps:**
```bash
export MCL_API_KEY=$(grep OPENROUTER_API_KEY ~/Dev/Personal/kdp-publisher/.env | cut -d= -f2)

# Scripted mode: 10 runs, 20 turns each
cargo run -- playtest examples/crypt/crypt_adventure.graph.toml \
  --mode scripted --runs 10 --max-turns 20 \
  --backend openai --model meta-llama/llama-4-scout \
  --base-url https://openrouter.ai/api/v1 --api-key "$MCL_API_KEY" \
  --output playtest-scripted.jsonl

# LLM player mode: 5 runs, 20 turns each
cargo run -- playtest examples/crypt/crypt_adventure.graph.toml \
  --mode llm --runs 5 --max-turns 20 \
  --backend openai --model meta-llama/llama-4-scout \
  --base-url https://openrouter.ai/api/v1 --api-key "$MCL_API_KEY" \
  --output playtest-llm.jsonl
```

**For each bug found:**
1. Record the exact LLM response that triggered it
2. Add a regression test to `crates/mcl-runtime/tests/playtest_regressions.rs`
3. Fix the bug
4. Verify the test passes

**Verify**: All runs complete. Zero crashes. playtest_regressions.rs has new tests for any bugs found.

---

## Task 1.4: Test Structured Output Mode in Eval

**Why**: JSON structured output may be more reliable than inline `MCL::*` signals. If it reaches 100% on Mistral too, that's a major finding.

**TDD:**
```rust
// crates/mcl-runtime/tests/structured_output_eval.rs
#[tokio::test]
async fn structured_output_parses_json_signals() {
    let json_response = r#"{"narration": "You swing!", "signals": [{"type": "action", "name": "attack", "args": {"target": "skeleton_guard"}}]}"#;
    let mock = MockLlmBackend::new(vec![json_response.to_string()]);
    let mut session = Session::with_config(
        Box::new(mock),
        SessionConfig { signal_mode: SignalMode::Structured, ..Default::default() },
    );
    // ... load context, turn, verify
}
```

**Implementation**: Add `--structured` flag to the `bench` and `eval` CLI commands. When set, use `build_structured_system_prompt()` and `extract_signals_from_json()`.

**Verify**: Run eval with `--structured` flag. Compare accuracy to inline mode.

---

## Task 1.5: Build "Debate" Domain

**Why**: Proves MCL isn't RPG-specific. A debate domain shows the architecture works for structured conversations.

**TDD:**
```rust
#[test]
fn debate_context_parses() {
    let src = include_str!("../../../examples/debate/opening_round.mcl");
    let doc = mcl_parse::parse_document(src).unwrap();
    assert_eq!(doc.frontmatter.context, "opening_round");
    assert!(!doc.actions.is_empty());
}
```

**Create files:**
- `examples/debate/opening_round.mcl` — Two debaters, a moderator NPC, actions: argue, rebut, concede, ask_question
- `examples/debate/closing.mcl` — Closing statements, voting
- `examples/debate/debate.graph.toml` — Narrator LLM + game engine (no combat/inventory nodes)

**MCL syntax example:**
```
---
context: opening_round
type: scene
domain: debate
---

lit: true
round: 1
topic: "Should AI systems have knowledge boundaries?"

- moderator:
    type: npc
    disposition: neutral
    knowledge: [topic, rules, both_positions]

- argue:
    description: "Present an argument for your position"
    trigger: [argue, present, claim, assert, contend]
- rebut:
    description: "Counter the opponent's last argument"
    trigger: [rebut, counter, disagree, refute, challenge]
    requires: [round > 0]
- concede:
    description: "Concede a point to the opponent"
    trigger: [concede, agree, accept, grant]
- ask_question:
    target: enum(entities)
    description: "Ask a clarifying question"
    trigger: [ask, question, clarify, what about]

- closing:
    when: round >= 3
    direction: next

tone: analytical
voice: third_person
tense: present
context: "A structured debate is underway. The moderator keeps time."
forbidden: [definitely, obviously, clearly]

- debate_topic
- !opponent_private_notes
- !judge_scoring_criteria
```

**Verify**: `cargo run -- validate examples/debate/opening_round.mcl` succeeds. Parse tests pass.

---

## Task 1.6: Build "Quiz" Domain

**Why**: Educational use case. Demonstrates MCL for guided experiences.

**Create:**
- `examples/quiz/question_1.mcl` — Multiple choice, entity is the quizmaster
- `examples/quiz/question_2.mcl` — Follow-up based on answer
- Knowledge boundaries: quizmaster knows answers, player doesn't

**Verify**: Parses and validates. Can run with mock LLM.

---

## Task 1.7: WASM Build Target

**Why**: "No Python framework can run in a browser." This is the most compelling demo.

**Steps:**
1. Add `wasm32-unknown-unknown` target
2. Identify and gate platform-specific code (stdin, fastembed, file I/O)
3. Create `mcl-wasm` crate that exposes `parse_document()`, `build_system_prompt()`, signal extraction
4. Build a minimal JS wrapper that calls WASM functions
5. Host on GitHub Pages

**TDD:**
```rust
// crates/mcl-wasm/tests/wasm_tests.rs (run with wasm-pack test)
#[wasm_bindgen_test]
fn parse_mcl_in_browser() {
    let src = include_str!("../../../examples/crypt/crypt_antechamber.mcl");
    let result = mcl_wasm::parse(src);
    assert!(result.is_ok());
}
```

**Verify**: `wasm-pack build` succeeds. Demo page loads in browser.

---

## Task 1.8: Initial Git Commit + Publish

**Steps:**
1. `git add -A && git commit -m "feat: initial MCL release — 349 tests, 12 crates, 17.6K LOC"`
2. Push to GitHub
3. `cargo publish -p mcl-spec` (then each crate in dependency order)
4. Create GitHub release with changelog

**Verify**: `cargo install mcl-cli` works on a clean machine. README quick start runs.

---

## Phase 1 Exit Criteria

All must be true:

- [ ] `filter_state_for_knowledge()` called in `build_system_prompt()` — knowledge boundaries enforced
- [ ] Combat determinism test passes 100 iterations
- [ ] 10+ scripted playthroughs complete without crashes
- [ ] 5+ LLM player playthroughs complete without crashes
- [ ] Structured output eval runs and accuracy is recorded
- [ ] 3 domains exist: RPG (crypt), Debate, Quiz
- [ ] WASM build compiles and runs in browser
- [ ] Published to crates.io
- [ ] GitHub repo has commits and CI is green
- [ ] All quality gates pass (tests, clippy, fmt)
