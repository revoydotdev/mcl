---
adr: 12
title: "Worlds compose: policy overlays and a federated arbiter"
status: Proposed
implementation: Design-only
date: 2026-07-07
last_reviewed: 2026-07-07
amends: []
supersedes: null
superseded_by: null
tags: [composition, overlays, federation, stdlib]
decision: "Formalize extends + policy-overlay composition so a signal must pass every overlay's policies; build toward a registry of vetted, versioned governance overlays (HIPAA, PCI, spend-confirmation) that are imported and composed like crates. The endgame stage — deliberately sequenced last (Stage 5)."
---
# ADR-012 — Worlds compose: policy overlays and a federated arbiter

> **Status:** Proposed · **Implementation:** Design-only · **Reviewed:** 2026-07-07

## Context

`extends` inheritance exists in the language (wired in `mcl-lang`), and the Pregel node
graph already composes resolvers as independent nodes on typed channels. What does not yet
exist is *governance* composition: a base "hospital" world, a "cardiology" extension, a
"night-shift" policy overlay — with the arbiter enforcing **all** applicable layers. Every
team currently hand-rolls its governance stack per deployment; the moonshot's sixth horizon
is governance as a standard library you `use`.

This is the endgame because its value compounds with everything before it: overlays are
only trustworthy if the parser is single-sourced (ADR-003), embeddable everywhere
(ADR-004/007), reliability-measured (ADR-008), and certifiable (ADR-011).

## Decision

- **Formalize composition semantics:** `extends` + policy overlays get defined merge/
  override rules (child overrides parent; policies are additive — an overlay can only
  *narrow*, never widen, what the composed world permits).
- **The arbiter federates:** a signal must pass every overlay's arbiter/policy chain;
  verdicts record *which* layer rejected, for auditability.
- **A standard library of overlays:** vetted, versioned, composable governance modules —
  starting from the existing examples grown into real overlays (PHI/HIPAA-shaped, budget/
  spend-confirmation, RAG source-trust) — importable via a `use <overlay>` mechanism with
  semver.
- Registry/marketplace tooling (`mcl add <overlay>`, conformance badges) only after ≥2
  overlays have real external users; distribution follows demand, not the reverse.
- Explicitly **Stage 5**: the first commit (two composable overlays + a test that a signal
  must satisfy both) is cheap, but the productization is gated on Stages 3–4 landing.

## Consequences

- Policy semantics need a monotonicity guarantee (narrowing-only) to make composition safe
  to reason about — this constrains future policy-language features.
- Version skew between composed overlays becomes a user-facing failure mode; semver +
  conformance tests are load-bearing.
- MCL's pitch gains its final form: import governance the way you import a crate.

## Roads not taken

- **Ship a marketplace early as a growth hack.** Rejected: a registry of unvetted policy
  modules is a liability in exactly the domains (health, finance) the layer targets.
- **Arbitrary overlay logic (overlays can widen permissions).** Rejected: without the
  narrowing guarantee, adding an overlay could *weaken* a world — composition must be
  monotone to be auditable.
