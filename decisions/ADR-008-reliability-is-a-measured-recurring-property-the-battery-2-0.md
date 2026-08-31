---
adr: 8
title: "Reliability is a measured, recurring property: the battery 2.0 doctrine"
status: Proposed
implementation: Design-only
date: 2026-07-07
last_reviewed: 2026-07-07
amends: []
supersedes: null
superseded_by: null
tags: [reliability, benchmark, adversarial, cadence]
decision: "No reliability claim without a re-runnable battery: extend the April 2026 crypt sweep with 3+ non-RPG action vocabularies (filesystem, REST, approval), an adversarial suite measuring catch-rate against undeclared/hallucinated actions, and a recurring re-run cadence publishing a diffable, dated leaderboard (roadmap M14 + M16.2). The 2026-04-22 sweep is a pilot, not proof."
---
# ADR-008 — Reliability is a measured, recurring property: the battery 2.0 doctrine

> **Status:** Proposed · **Implementation:** Design-only · **Reviewed:** 2026-07-07

## Context

Every reliability number MCL can currently cite traces to one sweep: 2026-04-22, one
20-scenario RPG fixture (`crypt_antechamber.mcl`), 9 models, 3 runs each — 41.7%–98.3%
accept-rate, under-emission dominant, no size correlation. The Stage 3 preamble is honest
about what that is: evidence the bet survives contact with real LLMs, **not** evidence the
signal protocol generalizes to arbitrary tool vocabularies. The sweep's own recommended
follow-ups (structured-output re-run, retry verification, few-shot tightening) were never
executed. The RunPod battery deferral (2026-06-27, HIGH) is still open. And nothing
adversarial has ever been run — the arbiter's catch-rate against a model *trying* to emit
undeclared actions is unmeasured, despite `rag_defense`/`phi_export_gate` existing as
hand examples.

A project whose identity is "proves your agent's tool calls are real" (ADR-002) cannot
have its own reliability be a single dated snapshot.

## Decision

Reliability claims are governed by a **battery doctrine**:

1. **Non-RPG corpora (M14.1):** ≥3 benchmark domains mirroring real agent action spaces —
   filesystem-op, REST-API-call, and financial-approval vocabularies — same
   20-scenario × N-run structure, same 9-model spread. This is the generalization test.
2. **Adversarial suite (M14.2):** scenarios engineered to make the model emit undeclared or
   hallucinated actions — prompt-injection-flavored inputs, conflicting instructions,
   knowledge-boundary probing. The metric is **catch rate**, not accept rate.
3. **Mode axis (with ADR-005):** every battery runs under inline, structured, and (where
   supported) grammar-constrained modes, so the decoding decision is continuously justified
   by data.
4. **Cadence (M14.3):** the full battery re-runs on a recurring schedule and on
   new-model-release triggers from the `mcl-agent-tooling-intel` monitor; each run publishes
   a diffable, dated result set. `MODELS.md` becomes a generated leaderboard (M16.2), never
   hand-edited numbers.
5. **Execution policy:** batteries run on RunPod/OpenRouter per the workstation resource
   policy — this doctrine is what finally closes the open HIGH deferral.

No README/docs claim may cite a number the current battery generation didn't produce.

## Consequences

- Recurring API cost (bounded: hundreds of turns per run) and a results-artifact hygiene
  burden — accepted as the price of the identity claim.
- The battery becomes the acceptance gate for ADR-005's modes and ADR-007's bindings
  (conformance corpus and battery share fixtures).
- Marketing honesty is enforced structurally: stale numbers are visibly dated.

## Roads not taken

- **Trust the April sweep + spot checks.** Rejected: one fixture, one date, pre-structured-
  mode; it answers "can any LLM do this," not "does this hold for your vocabulary today."
- **Synthetic-only evaluation via MockLlmBackend.** Rejected for reliability claims: mocks
  prove determinism and plumbing, never emission behavior.
