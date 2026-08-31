---
adr: 9
title: "Honor the effect system: memoize Pure, content-address Cacheable, quarantine ReplayUnsafe"
status: Proposed
implementation: Design-only
date: 2026-07-07
last_reviewed: 2026-07-07
amends: []
supersedes: null
superseded_by: null
tags: [effects, replay, caching, determinism]
decision: "The runtime consults Effect per action: Pure results are memoized, Cacheable external calls are content-addressed and never re-paid, ReplayUnsafe is quarantined, IdempotencyKey dedups; the signal log gains rewind/fork-at-turn/diff-two-runs. Staged after the kernel carve-out (Stage 4)."
---
# ADR-009 — Honor the effect system: memoize Pure, content-address Cacheable, quarantine ReplayUnsafe

> **Status:** Proposed · **Implementation:** Design-only · **Reviewed:** 2026-07-07

## Context

`Effect::{Pure, StateMutating, External, Cacheable, ReplaySafe, IdempotencyKey}` exists in
the codebase and is barely consulted — the postmortem calls it "quietly one of the most
forward-looking things in the codebase." Meanwhile the blake3 signal log already gives
deterministic replay under `MockLlmBackend`, proven by tests. The gap between the two is a
missing runtime contract: effects are *declared* but not *honored*.

Honoring them yields three things no mainstream agent framework has together: time-travel
debugging (rewind/fork/replay a session bit-for-bit), a cost model (never pay twice for a
pure or cacheable result — material for long-horizon agents), and a principled quarantine
for the genuinely unreplayable.

## Decision

The runtime consults `Effect` on every resolved action:

- **`Pure`** results are memoized (keyed on content-addressed inputs).
- **`Cacheable`** external calls are content-addressed and never re-executed for identical
  inputs; cache hits are recorded in the signal log as hits (provenance preserved).
- **`ReplayUnsafe`** actions are quarantined at replay time: a replayed session either
  substitutes the logged result or halts with a typed error — never silently re-executes.
- **`IdempotencyKey`** dedups retries/double-submissions at the arbiter boundary.
- **Session time-travel:** `mcl registry replay` grows `--at <turn> --fork` (fork a new
  campaign from any logged turn) and a two-run diff; reconstruction must be bit-perfect for
  replay-safe sessions.
- A per-session **cost ledger** (calls made vs cache-served) is surfaced in `registry
  inspect`.

Staged as Stage 4 — after the kernel (ADR-004), because the effect contract belongs in the
kernel's type surface, and adapters (M15) should arrive already effect-aware.

## Consequences

- The resolver contract (ADR-006) carries effect annotations; tool loaders must map or
  default them conservatively (`External` + non-cacheable when unknown).
- Cache correctness bugs become state-integrity bugs; the battery gains replay-consistency
  scenarios.
- Storage growth in the registry (cached payloads) needs an eviction/GC policy.

## Roads not taken

- **Drop the Effect enum as speculative.** Rejected: replay/fork/cost is a differentiator
  directly downstream of the existing log; deleting it forfeits the moat adjacent to what's
  already built.
- **Build caching outside the registry** (a separate memo layer). Rejected: provenance must
  show cache hits, or the audit story (ADR-010) develops holes.
