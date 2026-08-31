# MCL TODOs — Bespoke LLM-in-Gaming Library

Exhaustive, unprioritized inventory of work items to turn `mcl` into a production library for building LLM-driven games. Cherry-pick into `ROADMAP.md` or issue tracker as needed. Grouped by concern, not by phase.

---

## 1. Signal Protocol & LLM Reliability (`crates/mcl-signal`, `crates/mcl-llm`)

- [ ] Build real-LLM reliability matrix: extract rate × model × prompt style × temperature across Llama 3.1/3.3 (8B/70B), Gemma 2/3, Qwen 2.5, Mistral, Phi-4, GPT-4o/4o-mini, Claude Haiku/Sonnet/Opus.
- [ ] Commit a reliability SLO per model tier (e.g. cloud ≥99%, local-7B ≥92%) and fail CI if regression.
- [ ] GBNF grammar for `MCL::action(...)` / `MCL::mutate(...)` and wire into llama.cpp + Ollama backends.
- [ ] OpenAI structured outputs (JSON schema) path as fallback when signal missing.
- [ ] Anthropic tool-use path: emit signals as tool calls, translate back to `MCL::…` internally.
- [ ] Signal repair pass: re-prompt with "previous response missing signal, emit only the signal" on miss.
- [ ] Confidence scoring per extracted signal (logprob-based where available, heuristic otherwise).
- [ ] Fuzz the extractor against adversarial outputs (nested calls, embedded code fences, unicode homoglyphs).
- [ ] Multi-signal-per-turn support + deterministic ordering guarantees.
- [ ] Signal dry-run mode: parse but do not apply, for safe probing.
- [ ] Golden-file regression suite for extractor edge cases.

## 2. Language & Parser (`crates/mcl-lang`, `crates/mcl-spec`, `crates/mcl-parse`)

- [ ] LSP server crate (`mcl-lsp`) with diagnostics, hover, go-to-def, rename, find-references, completions.
- [ ] Incremental reparse (rope + tree-sitter-style or Salsa).
- [ ] Error-recovery parser so the LSP shows partial diagnostics on broken files.
- [ ] Formal grammar spec (EBNF) in `docs/` matching the implementation.
- [ ] `mcl fmt` formatter with stable output.
- [ ] `mcl lint` with per-rule severities (unused-tag, shadowed-entity, unreachable-room, missing-transition).
- [ ] `import`/`include` resolution with cycle detection and caching.
- [ ] Macros / templates for repeated entity patterns.
- [ ] Static type system for entity/item/action schemas; type-check signals against declared actions.
- [ ] Schema versioning + migration tool (`mcl migrate v1 → v2`).
- [ ] `mcl doc` generator (HTML + Markdown) for a game's entities/actions/rooms.
- [ ] Tree-sitter grammar for editor ecosystem reuse.
- [ ] Source maps from compiled prompt → original `.mcl` spans for error attribution.

## 3. Runtime & Graph (`crates/mcl-runtime`, `crates/mcl-graph`, `crates/mcl-nodes`)

- [ ] End-to-end token streaming: backend → node → session → websocket → browser.
- [ ] Turn-order / priority scheduler for multi-agent super-steps (milestone 8.3 gap).
- [ ] Pause / resume mid-super-step with durable checkpoint.
- [ ] Deterministic replay CLI: feed JSONL audit log, reproduce identical world state.
- [ ] Time-travel debugger: step back N turns, branch timeline.
- [ ] Hot-reload of graph / context files while session is live.
- [ ] Parallel node execution within a super-step (independent subgraphs).
- [ ] Backpressure and cancellation across async node boundaries.
- [ ] Graceful backend failover (primary → fallback model).
- [ ] Per-node timeouts + circuit breakers.
- [ ] Structured error taxonomy surfaced through `MclError` with codes.
- [ ] Snapshot diff API (state_before → state_after per turn).
- [ ] Node authoring API stabilized + `#[mcl_node]` macro.

## 4. Gameplay Domains (`crates/mcl-domains`)

- [ ] Dialogue / conversation domain with NPC memory and relationship tracking.
- [ ] Skill check system: d20-with-advantage, PbtA 2d6, FitD dice pool, BitD position/effect.
- [ ] Spell / ability system with costs, cooldowns, targeting.
- [ ] Faction + reputation resolver.
- [ ] Crafting / recipe resolver with ingredient consumption.
- [ ] Economy (currency, shops, prices, haggling).
- [ ] Procedural generation hooks (rooms, loot, NPCs) w/ seeded RNG.
- [ ] GOAP and utility-AI adapters for scripted NPCs.
- [ ] Quest state machine (offered → active → failed/complete) with branching.
- [ ] World clock + scheduled events / cron within the game world.
- [ ] Physical-space resolver: coordinates, line-of-sight, movement costs.
- [ ] Status effects (poison, stun, buffs) with duration and stacking rules.
- [ ] Pluggable domain registration (third-party resolvers without forking).
- [ ] Domain test-kit crate for authors writing their own resolvers.

## 5. Memory & Retrieval (`crates/mcl-memory`)

- [ ] Hybrid retrieval: BM25 + vector with rank fusion.
- [ ] Hierarchical summarization (turn → scene → chapter → session).
- [ ] Entity-scoped memory (per NPC, per room) with scoping queries.
- [ ] Forgetting curve / relevance decay.
- [ ] Dedup and canonicalization across sessions.
- [ ] Hot/cold tiering (in-memory → mmap → object store).
- [ ] Pluggable embedder trait: fastembed, OpenAI, Cohere, Voyage, local gguf.
- [ ] Cross-session memory store with player-scoped keys.
- [ ] Redaction API (forget this entity, GDPR delete).
- [ ] Retrieval evals with MRR / NDCG against seeded corpora.

## 6. LLM Backends (`crates/mcl-llm`)

- [ ] Streaming implementations for Ollama, OpenAI, Anthropic (trait already defined).
- [ ] Token accounting (input/output/cached) surfaced per turn.
- [ ] Logprobs / top-k sampling where providers support.
- [ ] vLLM, TGI, LM Studio, Groq, Together, Mistral, Fireworks, DeepInfra, Cerebras, SambaNova backends.
- [ ] Azure OpenAI + Bedrock + Vertex passthrough.
- [ ] Gemini backend.
- [ ] Speculative decoding for local backends.
- [ ] Batched inference for multi-NPC super-steps.
- [ ] Per-node model routing (narrator = big, picker = small).
- [ ] Cost-aware router with budget ceilings.
- [ ] Offline mode (fail closed if no local backend reachable).
- [ ] Anthropic + OpenAI prompt caching integration with cache-hit telemetry.
- [ ] Request deduping + idempotency keys.
- [ ] Provider health probes on startup.
- [ ] Fix outstanding clippy warning in `crates/mcl-llm/src/normalize.rs` (per `final-audit.md`).

## 7. Evaluations & QA

- [ ] Grow eval corpus to 1k+ cases covering signals, knowledge filtering, tone adherence, safety.
- [ ] `mcl eval leaderboard` CLI emitting Markdown + HTML.
- [ ] CI regression gate: block merge if signal extraction drops vs baseline.
- [ ] Nightly eval workflow using self-hosted Ollama runner (`arch-linux`).
- [ ] Adversarial / jailbreak test set.
- [ ] Content-safety eval (violence, sexual content, self-harm flags) w/ configurable thresholds.
- [ ] LLM-as-judge narrative quality eval with rubric.
- [ ] Playability eval: can a bot player reach a goal state in ≤N turns?
- [ ] Reproducibility suite: identical seed → identical transcript.
- [ ] Eval result diffing UI.
- [ ] Hold-out eval set frozen from training/optimization loops.

## 8. Playtesting Infrastructure

- [ ] Keep & harden scripted-agent harness (already produces `playtest-scripted.jsonl`).
- [ ] Bot-player personas: speedrunner, explorer, chaos monkey, newbie, rules-lawyer.
- [ ] Fuzz players: random-walk + targeted mutators.
- [ ] Property-based invariants (HP ≥ 0 unless dead, inventory mass ≤ capacity, no orphaned rooms).
- [ ] Crash / stall detector with automatic bug-minimization (delta-debugging on transcripts).
- [ ] Session replayer with side-by-side diff view of two runs.
- [ ] Playtest report generator (markdown summary per build).
- [ ] Heatmaps: most-visited rooms, most-used actions, deadlock spots.

## 9. Web Frontend (`crates/mcl-server/static`, new `crates/mcl-web`)

- [ ] Scaffold a real SPA (SvelteKit or React+Vite) in `crates/mcl-web`, served by `mcl-server`.
- [ ] Streaming narration UI (token-by-token).
- [ ] Inventory, HP/AC, status-effect panels.
- [ ] Action menu w/ valid-action filtering from runtime.
- [ ] Map / room view from graph topology.
- [ ] Session browser + save-slot UI backed by persistence layer.
- [ ] Developer HUD: live signal trace, state diff, prompt inspector, token cost.
- [ ] Author mode: edit `.mcl` in-browser with LSP diagnostics via websocket.
- [ ] Multiplayer lobby + spectator mode.
- [ ] Accessibility pass (ARIA, keyboard nav, screen-reader transcript mode).
- [ ] Mobile-responsive layout.
- [ ] Theming per-game (game provides CSS tokens).

## 10. Editors & Author Tooling (`editors/`)

- [ ] VS Code extension feature parity with LSP (currently TextMate-only).
- [ ] Snippets pack for common MCL patterns.
- [ ] JetBrains plugin.
- [ ] Zed extension.
- [ ] Neovim plugin using LSP.
- [ ] Visual graph editor (browser) with drag-drop nodes, round-trips to `.graph.toml`.
- [ ] Context-file WYSIWYG editor.
- [ ] Constraint-tag linter with quick-fixes.
- [ ] Asset pipeline: portraits, ambient audio, sfx bound to entities/rooms.
- [ ] `mcl new <template>` scaffolder.
- [ ] `mcl playground` for quick prompt iteration without full game.

## 11. SDKs & Engine Bindings

- [ ] Python SDK via PyO3 (`crates/mcl-py`).
- [ ] JS/TS SDK: expand `crates/mcl-wasm` beyond parser to runtime + signal extraction.
- [ ] Node-native bindings via napi-rs for server-side JS.
- [ ] C ABI (`crates/mcl-capi`) for language bindings.
- [ ] C# / .NET package (for Unity & Godot C# users).
- [ ] Godot GDExtension.
- [ ] Unity package (via C ABI or native plugin).
- [ ] Unreal plugin (C++ wrapper).
- [ ] Bevy integration crate.
- [ ] SDK parity test-suite (same transcripts across all bindings).

## 12. Persistence & Data

- [ ] `Store` trait abstracting session snapshots, profiles, transcripts.
- [ ] SQLite default implementation.
- [ ] Postgres implementation for production.
- [ ] Object-store implementation (S3/R2/GCS) for transcripts + snapshots.
- [ ] Content-addressable saves (dedup by hash).
- [ ] Player profile service (saves, stats, preferences).
- [ ] Telemetry schema (event types, retention).
- [ ] Export / import tool for full sessions.
- [ ] GDPR delete API + audit record.
- [ ] Encryption at rest for save data.

## 13. Multiplayer, Server, MCP (`crates/mcl-server`, `crates/mcl-mcp`)

- [ ] Authoritative server mode (clients never mutate state locally).
- [ ] Spectator sockets + fan-out broadcast.
- [ ] Matchmaking / lobby service.
- [ ] Room lifecycle (create/pause/resume/archive).
- [ ] Auth: JWT or OIDC, per-game API keys.
- [ ] Rate limiting per client + per model cost.
- [ ] MCP tool schema hardening + version negotiation.
- [ ] MCP resource endpoints for game state introspection.
- [ ] Host-your-own-model reverse proxy (users bring Ollama URL).
- [ ] Horizontal scale: session affinity + Redis-backed state.
- [ ] WebRTC data channels for low-latency turns (experimental).

## 14. Security & Safety

- [ ] Input sanitization of player text before prompt assembly.
- [ ] Output scanner for prompt-injection artifacts ("ignore previous…").
- [ ] State-level knowledge filtering (not just prompt-level) — currently fragile per audits.
- [ ] PII scrubber with configurable rules.
- [ ] Content-moderation hook (pluggable: OpenAI Mod, Azure, local classifier).
- [ ] Sandboxed resolver plugins via WASM (wasmtime).
- [ ] Audit-log integrity: hash-chain entries so tampering is detectable.
- [ ] Secrets: never log API keys; redact in traces.
- [ ] Dependency audit (`cargo audit`) in CI.
- [ ] Threat model doc in `docs/SECURITY.md`.
- [ ] Signed release artifacts (sigstore / cosign).

## 15. Performance

- [ ] Cold-start budget < 200ms for CLI `play`.
- [ ] Per-turn P50 / P99 SLOs defined and tracked.
- [ ] Criterion benches in CI with regression thresholds.
- [ ] Heap + allocation profiling (dhat, heaptrack) reports published.
- [ ] WASM bundle-size budget + tree-shaking audit.
- [ ] SIMD for embedding hot paths.
- [ ] Async runtime tuning (tokio worker count, blocking-pool size).
- [ ] Prompt-assembly benchmark against context-window growth.

## 16. Observability

- [ ] OpenTelemetry traces per turn (spans: prompt-build, backend-call, signal-extract, resolver).
- [ ] Structured JSON logs with correlation IDs.
- [ ] Prometheus metrics endpoint.
- [ ] Grafana dashboard templates in `docs/observability/`.
- [ ] Per-session cost tracker (tokens × $/token).
- [ ] Error taxonomy exported as machine-readable.
- [ ] Sentry / error-reporter integration.
- [ ] Sampling: full traces on errors, 1% on successes.

## 17. Docs & Learning

- [ ] mdBook site `docs/book/` hosted at a project URL.
- [ ] Hosted `cargo doc` for all public crates.
- [ ] Tutorial series: 5 progressively complex games, each a chapter.
- [ ] Architecture deep-dives (Pregel exec, signal protocol, knowledge boundaries).
- [ ] Migration guides (per breaking release).
- [ ] `CONTRIBUTING.md` with PR checklist.
- [ ] Video walkthrough scripts (record later).
- [ ] Glossary of terms (signal, resolver, context, super-step, etc.).
- [ ] FAQ / troubleshooting.

## 18. Examples & Templates

- [ ] Keep `examples/crypt/`, expand to a full 10-room dungeon crawler.
- [ ] Visual novel template.
- [ ] Detective / mystery template (hidden knowledge is the gimmick).
- [ ] Survival / resource management.
- [ ] Tycoon / management sim.
- [ ] Dating sim (relationship resolver).
- [ ] Educational tutor (Socratic).
- [ ] Corporate training / compliance drill.
- [ ] Safety drill / emergency response.
- [ ] Chatbot NPC drop-in for existing games.
- [ ] `mcl new <template>` scaffolder reads from a template registry.

## 19. Release & Distribution

- [ ] Crates.io publish plan per crate (publish order respects DAG).
- [ ] Semver policy doc.
- [ ] `CHANGELOG.md` generation from conventional commits.
- [ ] Homebrew tap formula.
- [ ] AUR package.
- [ ] Docker images (server + CLI).
- [ ] Nix flake.
- [ ] Signed releases (sigstore).
- [ ] Deprecation policy + LTS branch strategy.
- [ ] Pre-release channels (nightly, beta, stable).

## 20. CI / CD

- [ ] Matrix across self-hosted runners (`arch-linux`, `macos-sonoma`, `win11-ci`).
- [ ] PR checks: `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`, `cargo deny`.
- [ ] Nightly real-LLM eval against local Ollama on `arch-linux`.
- [ ] Release workflow with artifact signing.
- [ ] Coverage reporting (`cargo-llvm-cov`) uploaded as artifact.
- [ ] Cache `target/` across jobs.
- [ ] Pre-commit hooks (fmt, clippy-fast, typos).
- [ ] `cargo bloat` report on releases.

## 21. Governance & Community

- [ ] License header audit across all source files.
- [ ] DCO or CLA decision + enforcement bot.
- [ ] `CODE_OF_CONDUCT.md`.
- [ ] `SECURITY.md` with disclosure process.
- [ ] Issue + PR templates.
- [ ] RFC process (`rfcs/` directory with numbered proposals).
- [ ] Discord or Matrix server + invite flow.
- [ ] Plugin registry (index of third-party domains, backends, templates).
- [ ] Governance doc (maintainers, decision-making).

## 22. Research & Exploration

- [ ] DSPy-style automatic prompt optimization against eval harness.
- [ ] RL / preference learning from playtest signal success.
- [ ] Fine-tune small model (3–8B) specialized for MCL signal emission — reduce cost/latency.
- [ ] World-model caching: cache stable prompt prefixes cross-turn.
- [ ] Planner + LLM hybrid for long-horizon procedural narrative.
- [ ] Multi-modal: image/audio generation bound to entities, narration TTS.
- [ ] Agent-vs-agent play for balance testing.
- [ ] Self-play data generation for fine-tuning.
- [ ] Formal verification of safety invariants (TLA+ sketch on knowledge filter).

---

_Last updated: 2026-04-19. Items are unprioritized; see `ROADMAP.md` for sequencing._
