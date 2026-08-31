---
adr: 11
title: "mcl verify: bounded model checking over declared worlds, and certified worlds"
status: Proposed
implementation: Design-only
date: 2026-07-07
last_reviewed: 2026-07-07
amends: []
supersedes: null
superseded_by: null
tags: [verification, model-checking, policy, certification]
decision: "Grow mcl-policy into a checker that answers reachability/invariant questions statically (no PHI export to non-auditors on any path, budget stays non-negative, no deadlock); mcl verify returns a proof or a counterexample trace; a certified world is a signed artifact attesting no-undeclared-action + no-policy-violation on any path (Stage 4, after the kernel)."
---
# ADR-011 — mcl verify: bounded model checking over declared worlds, and certified worlds

> **Status:** Proposed · **Implementation:** Design-only · **Reviewed:** 2026-07-07

## Context

`mcl-policy` is a small expression language with a type-checker and a tree-walking
evaluator, with real non-game examples (`phi_export_gate`, `budget_policy`, `rag_defense`,
`confidence_routing`) — the part of MCL that made the external reviewer "stop thinking
game engine and start thinking governance runtime." Today policies are enforced
*dynamically*, per signal. The declared world is also a closed transition system: typed
state, enumerated actions with preconditions and mutations, declared transitions.

That combination is model-checkable: "can any sequence of declared actions reach a state
where PHI is exported to a non-auditor?" is a reachability query, answerable statically,
before any model runs. The moonshot (horizon 2) names the endgame: worlds that carry a
proof. Agents entering regulated domains (health, finance, aviation) will need exactly
this artifact, and no agent framework produces one.

## Decision

- **Bounded model checking over the declared world:** grow `mcl-policy` + the kernel's
  world model into a checker that explores the action/transition system for invariant
  violations, reachability of forbidden states, and deadlock — bounded depth first,
  honest about the bound.
- **`mcl verify <world.mcl>`** returns either *holds (within bound)* with the explored
  envelope, or a **counterexample trace** — the concrete action sequence that violates the
  invariant, replayable in the runtime.
- **Certified worlds:** a signed artifact binding (world content hash → invariants checked
  → result → bound/method), embeddable in the ADR-010 audit export.
- First target invariants come from the existing examples: no-PHI-export-to-non-auditor,
  budget non-negativity, no-deadlock.
- Stage 4 endgame; starts as a bounded checker for a single invariant + a counterexample
  printer (the moonshot skeleton's first commit).

## Consequences

- Language design pressure: constructs that resist checking (unbounded strings in
  conditions, open-world escape hatches) become visible and either get semantics or get
  flagged as unverifiable — that tension is a feature.
- State-space explosion is real; bounded checking with explicit bounds is the honest
  contract, upgradeable later (SMT backends) behind the same CLI.
- "Certified" must never overclaim: the certificate names exactly which invariants, what
  bound, which checker version.

## Roads not taken

- **Translate worlds to an existing checker (TLA+/NuSMV/Alloy) instead of building one.**
  Seriously considered; rejected as the *product* path because the translation gap (MCL
  semantics → foreign formalism) becomes the trust boundary and the UX is unshippable to
  MCL's users. Revisit as an internal cross-check oracle for the checker itself.
- **Runtime-only enforcement forever.** Rejected: dynamic enforcement proves what *did*
  happen; regulated adoption needs what *cannot* happen.
