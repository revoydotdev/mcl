---
adr: 2
title: "MCL is a signal-validation layer, not an agent framework or a game engine"
status: Proposed
implementation: Design-only
date: 2026-07-07
last_reviewed: 2026-07-07
amends: []
supersedes: null
superseded_by: null
tags: [identity, strategy, positioning]
decision: "Ratify the Stage 3 mandate as the project identity: the registry/arbiter/signal core is the product — a domain-, provider-, and framework-agnostic validation layer installable under anything — and interactive fiction is a demo domain, not the market. Do not compete on less-abstraction-than-a-raw-SDK; compete on deterministic replay, knowledge-boundary scoping, and content-addressed provenance."
---
# ADR-002 — MCL is a signal-validation layer, not an agent framework or a game engine

> **Status:** Proposed · **Implementation:** Design-only · **Reviewed:** 2026-07-07

## Context

Three independent evaluations converge on the same diagnosis:

1. The 2026-04-12 outside evaluation: the interactive-fiction market is too small; the
   moat is the design decisions (signal protocol, knowledge boundaries, deterministic
   resolution), not the code; a senior engineer could replicate 70–80% on LangGraph in weeks.
2. The 2026-07-07 assessment behind the Stage 3 mandate: MCL cannot win "less abstraction
   than a raw SDK," but its registry/arbiter/signal-log core — append-only, blake3
   content-addressed, deterministic-replay, knowledge-boundary-scoped — is a stronger
   observability/provenance primitive than most agent frameworks bolt on after the fact.
3. The external-review postmortem (an outside engineer who built `revoymcl` against MCL
   0.1.0): "You have built the SQL of LLM behavior constraints. The `HallucinationArbiter` +
   policy DSL + content-addressed log is a *category*, not a game engine."

Meanwhile `docs/comparison.md` still frames MCL per-competitor ("use LangGraph for general
agent workflows…"), and the README leads with a text-adventure quick start. The first proof
that the core generalizes already exists: revoy ADR-053 adopts MCL's arbiter for coding-agent
governance via the `revoymcl` bridge — a non-game, non-fiction consumer.

## Decision

Ratify the Stage 3 mandate as MCL's identity, stated positively:

> **MCL is a signal-validation layer: it proves that an LLM's emitted actions are declared,
> typed, policy-clean, and replayable — under any framework, provider, or host language.**

Concretely:

- The registry/arbiter/signal core is the product. `mcl play`, the crypt demo, and the RPG
  domain are the reference application and test domain — kept, polished, but never the pitch.
- MCL does not compete on abstraction-minimalism with raw SDKs, nor on workflow breadth with
  LangGraph. It competes on what nobody else has end-to-end: deterministic replay,
  knowledge-boundary scoping, content-addressed provenance, and (per ADR-005) generation-time
  constraint derived from a declared world.
- Positioning language across README/comparison/docs converges on: "MCL is not your agent
  framework; MCL is what proves your agent framework's tool calls are real."
- The flagship integration is revoy's own agent governance (ADR-053's build order) — prove
  the layer on a real coding-agent problem before pitching anyone else.

## Consequences

- Roadmap gravity shifts from game-feature completeness to kernel extraction (ADR-004),
  decoding constraint (ADR-005), bindings (ADR-007), and measured reliability (ADR-008).
- `docs/comparison.md`, README framing, and crates.io metadata need a repositioning pass
  (roadmap M16.3) — honest: the "use LangGraph for X" verdicts stay, joined by the
  validation-layer claim.
- The EdTech/compliance-simulation pivot from the April evaluation is subsumed: those are
  *customers of the layer*, not the product identity.

## Roads not taken

- **Double down on interactive fiction / EdTech verticals.** Rejected: market too small
  (April evaluation), and vertical polish doesn't compound — kernel leverage does.
- **Compete as a lighter agent framework.** Rejected: Rust + a bespoke DSL never out-simples
  `pip install`; the framing loses on contact.
- **Rebrand/rename the project.** Rejected for now: `mcl` (Model Control Language) already
  reads correctly for the validation-layer identity; churn without leverage.
