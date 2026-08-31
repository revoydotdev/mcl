# 01 — Stage 3 execution plan: kernel & constraint

*Executes the ROADMAP.md Stage 3 mandate (M12–M16), reconciled with the external-review
moonshot skeleton (Phases 0–2). ADRs: 003, 004, 005, 006, 007, 008, plus 002 as the frame.*

---

## 1. Reconciliation: mandate × external review

The Stage 3 mandate (2026-07-07) and the external roadmap skeleton were written days apart
by different eyes and overlap heavily but not exactly. This plan merges them; nothing from
either is silently dropped.

| Concern | Mandate (ROADMAP.md) | External skeleton | This plan |
|---------|---------------------|-------------------|-----------|
| One canonical parser | — *(absent)* | Phase 0 "bless one parser" | **S3.1** (ADR-003) — adopted; it was the missing precondition |
| Pure-core extraction | M12.1 `mcl-validate` crate | Phase 0 `mcl-kernel` firewall | **S3.2** merges both: kernel = the boundary, validate = the surface (ADR-004) |
| Spec parity / R2 drift | — | Phase 0 spec-parity test | **S3.1** (ADR-003) |
| Generic tool trait | M12.2 `ToolResolver` | — | **S3.3** (ADR-006) |
| Structured-output promotion | M13.1 | Phase 1 closed-model path | **S3.4** (ADR-005) |
| Grammar compilation | M13.2 GBNF/XGrammar | Phase 1 flagship + `mcl grammar` | **S3.4** (ADR-005) |
| Retry/few-shot hardening | M13.3 | — | **S3.4** tail (ADR-005) |
| PyO3 path primary | M12.3 | Phase 2 `mcl-py` + maturin | **S3.5** (ADR-007) |
| npm/wasm package | — | Phase 2 `mcl-js` | **S3.5**, second (ADR-007) |
| Cross-runtime conformance corpus | — | Phase 2 | **S3.5** (ADR-007) — also the S3.1 migration regression net |
| Non-RPG benchmark corpus | M14.1 | — | **S3.6** (ADR-008) |
| Adversarial battery | M14.2 | — | **S3.6** (ADR-008) |
| Re-run cadence + leaderboard | M14.3 + M16.2 | — | **S3.6** (ADR-008) |
| LangGraph adapter / SDK wrapper | M15.1–15.2 | — | **S3.7** |
| revoy flagship integration | M15.3 | *(noted as downstream)* | **S3.7** (with revoy ADR-053) |
| Publish + reposition | M16.1, M16.3 | — | **S3.8** (ADR-002) |
| Signal-degrade feedback loop | — | Postmortem friction #4 | **S3.4** tail — rejected/degraded signals feed back to the model next turn |
| Effects / verify / ledger / composition | — | Phases 3–6 | **Stage 4–5 docs** (ADR-009…012) |

## 2. Sequencing and why

```
S3.1 parser+spec ─► S3.2 kernel+validate ─┬─► S3.4 grammar/constraint ─┐
                                          ├─► S3.5 bindings ───────────┼─► S3.7 adapters + revoy
S3.6 battery corpora (start immediately) ─┴─► S3.6 battery modes ──────┘
                                                        S3.8 publish/reposition (rolling)
```

- **S3.1 before S3.2:** the kernel must wrap *one* parser; carving it around two would
  fossilize the ambiguity.
- **S3.6 corpus-building starts immediately** — scenario authoring needs no kernel; only
  the *mode axis* runs wait on S3.4. This also closes the standing RunPod deferral early.
- **S3.4 and S3.5 parallelize** after the kernel; they share nothing but its API.
- **S3.7 adapters come last** — they are thin precisely because everything under them is
  finished; building them early would mean building them twice.
- Monitors (`mcl-decoding-sota`, `mcl-agent-tooling-intel`) register at stage start;
  their findings feed S3.4/S3.6 assumptions continuously.

## 3. Phases (revoy-conformant: difficulty 1–100, priority VLOW…VHIGH)

### S3.1 — One grammar, one source of truth *(ADR-003)*
Bless `mcl-lang`; migrate `mcl-cli`/`mcl-wasm`; freeze `mcl-parse`; spec-parity gate;
document R2 (label lattice, degradation, `policy {}`); resolve positional-arg coercion.
**Exit:** all first-party callers on `mcl-lang`, fixtures byte-identical, spec-parity test
red-green demonstrated, R2 documented. *(diff 40, VHIGH)*

### S3.2 — Kernel firewall + `mcl-validate` *(ADR-004; subsumes ledgered M12.1)*
`mcl-kernel` crate; registry arbiter/log split from vector/causality halves; `cargo tree`
CI gate; `mcl-validate` CLI (schema + transcript in, verdicts + content-addressed log out).
**Exit:** gate green in CI; `mcl validate --schema tools.json --input transcript.jsonl`
runs with zero game-world concepts in the call path. *(diff 60, VHIGH)*

### S3.3 — `ToolResolver` *(ADR-006; ledgered as M12.2)*
Schema-shaped trait + MCP/OpenAI/Anthropic tool-list loaders; RPG resolvers reimplemented
atop it; `ActionResolver` deprecated then removed.
**Exit:** an MCP tool list loads directly as an action vocabulary; crypt demo plays
unchanged on the new trait. *(diff 45, HIGH)*

### S3.4 — Generation-time constraint *(ADR-005; subsumes ledgered M13.1)*
`mcl grammar <world.mcl>` (GBNF + XGrammar/llguidance + JSON-schema views); constrained
decoding wired behind `mcl-llm` for open models; `signal_mode="structured"` first-class for
closed APIs; bottom-four-model re-run testing the ≥20-point-jump hypothesis; verified
retry-on-missing-signal; few-shot tightening from sweep transcripts; degraded/rejected
signals fed back to the model next turn (postmortem #4).
**Exit:** a world with no `teleport` cannot sample `MCL::action(teleport)` on a
grammar-constrained backend (test); structured-mode battery results published. *(diff 60, VHIGH)*

### S3.5 — Bindings *(ADR-007; subsumes ledgered M12.3)*
`mcl-py` (maturin/PyO3 over the kernel) to PyPI; revoymcl flips to compiled-primary with
`CONTRACT.md` committed; `mcl-js` npm package; one conformance corpus green on all three
runtimes in CI.
**Exit:** `pip install` + 5-line arbiter example works; revoy ADR-053 item 9 calls the PyO3
path by default; corpus gate in CI. *(diff 45, HIGH)*

### S3.6 — Battery 2.0 *(ADR-008; subsumes ledgered M14.1)*
Three non-RPG vocabularies (filesystem ops, REST API calls, financial approvals) at the
crypt sweep's structure; adversarial suite measuring catch-rate; mode axis (inline ×
structured × grammar); RunPod/OpenRouter execution; recurring cadence; `MODELS.md` becomes
generated leaderboard (M16.2).
**Exit:** dated, diffable results for ≥3 domains × 9 models × modes; catch-rate published;
open HIGH deferral resolved via `revoy resolve`. *(diff 55, VHIGH)*

### S3.7 — Adapters & the revoy flagship *(M15)*
LangGraph callback/checkpointer validating tool nodes through the kernel; raw-SDK wrapper
at the Instructor onboarding bar; revoy integration closes ADR-053's build order
(generation-time constraint via MCP schemas, PreToolUse restriction, provenance-as-type,
capability tokens).
**Exit:** a LangGraph example rejects an undeclared tool call end-to-end; revoy runs a
governed pass through MCL by default. *(diff 65, HIGH)*

### S3.8 — Publish, prove, promote *(M16; rolling)*
crates.io for kernel/validate + deps; PyPI/npm; README badges + release tag + generated
changelog (clears the three standing `revoy audit` GAPs); comparison-doc repositioning per
ADR-002 (M16.3).
**Exit:** `revoy audit MCL` clean; `revoy audit MCL --hygiene` green; installable from all
three registries. *(diff 25, HIGH)*

## 4. Mandate criteria advanced

Domain-agnostic (S3.2/S3.3/S3.6) · provider-agnostic (S3.4/S3.5) · framework-agnostic
(S3.5/S3.7) · reliability-measured (S3.6) · adversarially tested (S3.6) ·
generation-time-hardened (S3.4). **Stage 3 alone can earn all six** — Stages 4–5 deepen the
moat rather than complete the definition.
