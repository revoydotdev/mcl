---
adr: 5
title: "Generation-time constraint is first-class: compile the declared world to a decoding grammar"
status: Proposed
implementation: Design-only
date: 2026-07-07
last_reviewed: 2026-07-07
amends: []
supersedes: null
superseded_by: null
tags: [constrained-decoding, grammar, structured-output, reliability]
decision: "The world file compiles to a decoding constraint — GBNF/XGrammar-class grammars for open models, structured-outputs/forced tool-use for closed APIs — so an undeclared signal is unsamplable rather than merely detected. This supersedes the Stage-1 posture that structured output is a Plan-B fallback; the arbiter chain is demoted to safety net and drift detector."
---
# ADR-005 — Generation-time constraint is first-class: compile the declared world to a decoding grammar

> **Status:** Proposed · **Implementation:** Design-only · **Reviewed:** 2026-07-07

## Context

The 2026-04-22 sweep (540 turns, 9 models, one RPG fixture) settled the existential
question and exposed the ceiling in the same table: zero malformed signals, 2/540
hallucinated — but accuracy ranged 41.7%–98.3% with **under-emission** (narrating an action
without the `MCL::` line) as the dominant failure, and no clean parameter-size correlation.
Prompt-engineering alone cannot close a 56-point spread that tracks instruction-following
literalism rather than capability.

Stage 1 treated structured output as "Plan B if inline reliability stays <95%" (Phase 5.3).
Every serious competitor in constrained output (Guidance, LMQL, Outlines, XGrammar) solves
this at the token level; `docs/comparison.md` concedes the weakness. Meanwhile MCL holds an
asset none of them have: **the grammar already exists** — the declared world file *is* the
action schema. The moonshot names this the flagship horizon: move from "catches the lie" to
"the lie is unsayable." revoy ADR-053 independently reached the same conclusion for its own
governance (its item 1: the schema IS a grammar).

## Decision

Constraint at generation time becomes a first-class, benchmarked mode, derived from the
world file:

- **World → grammar compiler:** `mcl grammar <world.mcl>` emits (a) a GBNF grammar
  (llama.cpp/Ollama), (b) an XGrammar/llguidance-compatible grammar (vLLM/SGLang), and
  (c) the equivalent JSON-Schema / tool-schema view — all from the declared signal + action
  set of the canonical parse (ADR-003), via the kernel (ADR-004).
- **Open-model path:** the compiled grammar is wired into constrained-decoding backends
  behind `mcl-llm`, so undeclared signals are unsamplable.
- **Closed-model path:** `signal_mode = "structured"` (OpenAI `response_format`/strict
  tools, Anthropic forced tool-use) is promoted from fallback to fully supported peer of
  inline signals — documented, configurable, benchmarked (roadmap M13.1: re-run the
  bottom-four models under it and test the sweep's own ≥20-point-jump hypothesis).
- **The arbiter chain is re-scoped, not retired:** with the floor enforced at decoding, the
  arbiter is the safety net for unconstrained backends, the drift detector (a reject under
  constrained decoding indicates a compiler/config bug), and the policy layer that grammars
  cannot express.
- The unactioned April recommendations land with this (roadmap M13.3): verified
  retry-on-missing-signal in the live `LlmNode`, few-shot tightening from the per-model
  missing-signal transcripts.

This supersedes the Stage-1 "structured output is Plan B" posture. (Nothing to stamp:
that posture predates this decision log.)

## Consequences

- Reliability stops being a per-model lottery: under constrained decoding the accept-rate
  question becomes "did the compiler emit the right grammar," which is testable in CI
  without LLM spend.
- `mcl grammar` is a standalone-valuable artifact (usable by people who never run MCL's
  runtime) — a distribution wedge.
- Two decoding paths (open/closed) must be kept semantically aligned; the battery
  (ADR-008) gains a mode axis and must measure inline vs structured vs grammar-constrained.
- Narration-quality tradeoff of structured modes is acknowledged and measured, not assumed.

## Roads not taken

- **Stay inline-only and iterate on prompts/retries.** Rejected: the sweep's spread and
  failure mode show this optimizes a ceiling, not the floor.
- **Adopt an existing constrained-output framework wholesale** (Outlines/Guidance as the
  runtime). Rejected: they constrain *output shape*; MCL constrains *declared world
  semantics* and keeps provenance/replay — the grammar is an export surface, not the product.
