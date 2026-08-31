# MCL Phase 2: Constrained-World Platform

**Prerequisite**: Phase 1 complete (published, 3 domains, WASM demo, knowledge filter wired in)
**Goal**: Become the standard runtime for applications where LLMs participate but don't control outcomes. Interactive fiction is the beachhead; education, training simulations, and guided experiences follow.
**Timeline**: 1-3 months after Phase 1
**Success metric**: MCP server shipping, game engine plugin, 3+ external content creators using `.mcl`, hosted demo, $10-50K MRR trajectory

---

## Quality Gates

All Phase 1 gates plus:

1. Every MCP tool has integration tests with real MCP client
2. Every game engine binding has a working sample project
3. Non-programmer content creators can author `.mcl` files without Rust knowledge (validated by user testing)
4. Hosted demo stays up with 99% uptime for 30 days
5. Performance: <100ms for signal extraction, <50ms for state resolution (excluding LLM latency)

---

## Task 2.1: MCP Server — Expose MCL Resolvers as Tools

**Why**: MCP has 97M monthly downloads. Any Claude/ChatGPT/Gemini agent can call MCP tools. MCL's combat/inventory/transition resolvers become usable by the entire ecosystem.

**TDD:**
```rust
// crates/mcl-mcp/tests/mcp_integration.rs
#[tokio::test]
async fn mcp_list_tools_returns_mcl_resolvers() {
    let server = start_test_mcp_server().await;
    let tools = server.list_tools().await;
    assert!(tools.iter().any(|t| t.name == "mcl.resolve_action"));
    assert!(tools.iter().any(|t| t.name == "mcl.get_state"));
    assert!(tools.iter().any(|t| t.name == "mcl.transition"));
}

#[tokio::test]
async fn mcp_resolve_action_returns_resolution() {
    let server = start_test_mcp_server().await;
    let result = server.call_tool("mcl.resolve_action", json!({
        "context": "crypt_antechamber",
        "action": "attack",
        "target": "skeleton_guard"
    })).await;
    assert!(result["outcome"].as_str().is_some());
    assert!(result["details"]["set:entities.skeleton_guard.hp"].is_some());
}

#[tokio::test]
async fn mcp_get_state_respects_knowledge_boundaries() {
    let server = start_test_mcp_server().await;
    let state = server.call_tool("mcl.get_state", json!({
        "context": "crypt_antechamber"
    })).await;
    // Should NOT expose secret_passage (knowledge boundary)
    assert!(!state.to_string().contains("secret_passage"));
}
```

**Implementation:**

Create new crate: `crates/mcl-mcp/`
- Depends on: `mcl-runtime`, `mcl-spec`, `mcl-parse`, `mcl-domains`
- Implements MCP server protocol (stdio transport)
- Tools exposed:
  - `mcl.load_context` — Load a `.mcl` file into the session
  - `mcl.get_state` — Get current world state (filtered by knowledge boundaries)
  - `mcl.resolve_action` — Resolve an action signal and return outcome
  - `mcl.transition` — Move to another context
  - `mcl.list_actions` — List available actions for current context
  - `mcl.get_prompt` — Get the system prompt for the current context
- Resources exposed:
  - `mcl://contexts` — List loaded contexts
  - `mcl://state` — Current world state

**MCP tool schema example:**
```json
{
  "name": "mcl.resolve_action",
  "description": "Resolve a player action through MCL's deterministic game engine. Returns outcome, state changes, and narration hints.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "action": { "type": "string", "description": "Action name (e.g., 'attack', 'pickup')" },
      "target": { "type": "string", "description": "Target entity or item ID" },
      "args": { "type": "object", "description": "Additional action arguments" }
    },
    "required": ["action"]
  }
}
```

**CLI integration:**
```
mcl serve-mcp --contexts examples/crypt/ --start crypt_antechamber
```

**Files:**
- `crates/mcl-mcp/Cargo.toml`
- `crates/mcl-mcp/src/lib.rs`
- `crates/mcl-mcp/src/server.rs` — MCP server implementation
- `crates/mcl-mcp/src/tools.rs` — Tool implementations
- `crates/mcl-cli/src/main.rs` — Add `serve-mcp` subcommand

**Verify**: `echo '{"jsonrpc":"2.0","method":"tools/list","id":1}' | cargo run -- serve-mcp --contexts examples/crypt/` returns tool list.

---

## Task 2.2: Multi-Persona Graph Demo

**Why**: Two LLM nodes in one graph — the most unique MCL differentiator. No other framework does this with deterministic turn-taking.

**TDD:**
```rust
#[tokio::test]
async fn two_llm_nodes_produce_different_voices() {
    // narrator (gothic) and advisor (tactical) both respond to player input
    // narrator goes first (priority 1), advisor responds to narrator (priority 2)
    let def = TomlGraphDefParser.parse_file("examples/multi_persona/gothic_advisor.graph.toml").unwrap();
    assert!(def.nodes.contains_key("narrator"));
    assert!(def.nodes.contains_key("advisor"));
    assert!(def.nodes["narrator"].priority < def.nodes["advisor"].priority);
}
```

**Create:**
- `examples/multi_persona/gothic_advisor.graph.toml` — Narrator (gothic tone) + Advisor (tactical tips)
- `examples/multi_persona/room.mcl` — Simple room with both personas configured
- The graph routes: `user_input → narrator → game_engine → display` AND `user_input → advisor → display`
- Priority ensures narrator speaks first, advisor second

**Verify**: Run with `cargo run -- graph examples/multi_persona/gothic_advisor.graph.toml`. Both voices appear in output.

---

## Task 2.3: Token-Level Constraint Enforcement via Structured Output

**Why**: Inline `MCL::*` extraction is MCL's biggest reliability risk. JSON structured output mode with provider-native schema enforcement can guarantee 100% signal validity.

**TDD:**
```rust
#[tokio::test]
async fn structured_output_with_schema_produces_valid_signals() {
    // Test that when using OpenAI's response_format with JSON schema,
    // every response is valid JSON with correct signal structure
    let backend = OpenAiBackend::new("gpt-4o-mini", &api_key)
        .with_url("https://openrouter.ai/api/v1")
        .with_response_format(json!({
            "type": "json_schema",
            "json_schema": {
                "name": "mcl_response",
                "schema": MCL_RESPONSE_SCHEMA
            }
        }));
    // ... run scenarios, verify 100% parse rate
}
```

**Implementation:**
- Add `response_format` field to `OpenAiBackend` and `AnthropicBackend`
- When set, append to API request body
- Define `MCL_RESPONSE_SCHEMA` constant matching the structured output format
- Add `--structured` flag to `eval` and `bench` CLI commands
- Run comparative eval: inline vs structured

**Files**: `crates/mcl-llm/src/openai.rs`, `crates/mcl-llm/src/anthropic.rs`, `crates/mcl-cli/src/main.rs`
**Verify**: Eval with `--structured` shows higher accuracy than inline mode.

---

## Task 2.4: Bevy Game Engine Plugin

**Why**: MCL handles narrative + game logic; Bevy handles rendering. Together they make a real game.

**TDD:**
```rust
// crates/mcl-bevy/tests/plugin_test.rs
#[test]
fn mcl_plugin_adds_systems() {
    let mut app = App::new();
    app.add_plugins(MclPlugin::new("examples/crypt/crypt_antechamber.mcl"));
    // Verify MCL systems are registered
    assert!(app.world.contains_resource::<MclSession>());
}

#[test]
fn mcl_event_triggers_turn() {
    let mut app = App::new();
    app.add_plugins(MclPlugin::new("examples/crypt/crypt_antechamber.mcl"));
    app.world.send_event(MclPlayerInput("look around".to_string()));
    app.update();
    let events: Vec<&MclNarration> = app.world.resource::<Events<MclNarration>>().iter().collect();
    assert!(!events.is_empty());
}
```

**Create crate**: `crates/mcl-bevy/`
- `MclPlugin` — Bevy plugin that creates an MCL session
- `MclPlayerInput` event — trigger a turn
- `MclNarration` event — receive narrator output
- `MclStateChanged` event — notify when state mutates
- `MclSession` resource — holds the runtime session

**Verify**: `cargo run --example bevy_crypt` opens a window with text-based crypt demo.

---

## Task 2.5: Content Creator Validation

**Why**: The `.mcl` format's usability by non-programmers is the key moat. If writers can't use it, the platform doesn't work.

**Steps:**
1. Recruit 3-5 writers/game designers (not Rust developers)
2. Give them: language reference doc + tutorial + 1 example file
3. Ask them to create a 2-room scenario from scratch
4. Time them. Record questions. Note where they get stuck.
5. Iterate on syntax based on feedback.

**Success criteria:**
- Average time to working 2-room game: < 45 minutes
- Zero questions about Rust-specific concepts
- Feedback: "I could teach this to someone else"

**Deliverable**: User testing report in `docs/content-creator-testing.md`

---

## Task 2.6: Hosted Demo

**Why**: "Try MCL without installing Rust" is essential for adoption.

**Implementation:**
- Deploy `mcl serve` on a VPS (or fly.io)
- Serve the existing `static/index.html` web frontend
- Configure with Llama 4 Scout via OpenRouter
- Add rate limiting (max 50 turns per session, max 100 sessions/day)

**Verify**: Navigate to `https://mcl-demo.example.com`, play the crypt adventure in browser.

---

## Task 2.7: `.mcl` Web Editor

**Why**: Content creators need a better authoring experience than a text editor.

**Implementation:**
- Extend VS Code extension with diagnostics (parse errors with line numbers)
- OR: Build a web-based Monaco editor with MCL TextMate grammar + live preview
- Live preview: parse on keystroke, show current state/actions/entities in sidebar

**Verify**: Syntax errors show red squiggles. Live preview updates as you type.

---

## Phase 2 Exit Criteria

- [ ] MCP server ships, `mcl serve-mcp` works
- [ ] Multi-persona graph demo runs with 2 LLM voices
- [ ] Structured output eval shows accuracy improvement
- [ ] Bevy plugin exists with working example
- [ ] 3+ non-programmer content creators successfully built `.mcl` content
- [ ] Hosted demo running and accessible
- [ ] All quality gates pass
