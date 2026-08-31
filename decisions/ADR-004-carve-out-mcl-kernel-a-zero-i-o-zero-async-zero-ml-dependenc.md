---
adr: 4
title: "Carve out mcl-kernel: a zero-I/O, zero-async, zero-ML dependency firewall around the pure core"
status: Proposed
implementation: Design-only
date: 2026-07-07
last_reviewed: 2026-07-07
amends: []
supersedes: null
superseded_by: null
tags: [kernel, architecture, embedding, crates]
decision: "The pure deterministic core (spec, lang, signal, registry arbiter+log, policy) moves behind a dependency firewall as mcl-kernel; graph/memory/llm/server become clients of it. mcl-validate is the standalone CLI/product surface built on the kernel (subsumes roadmap M12.1). Gate: a cargo tree assertion that the kernel pulls no tokio/reqwest/fastembed/usearch."
---
# ADR-004 — Carve out mcl-kernel: a zero-I/O, zero-async, zero-ML dependency firewall around the pure core

> **Status:** Proposed · **Implementation:** Design-only · **Reviewed:** 2026-07-07

## Context

The postmortem's highest-leverage finding (friction #2): binding the arbiter — the
highest-value surface — through PyO3 drags in fastembed, usearch, reqwest, and tokio,
because `mcl-registry` path-deps `mcl-graph → mcl-memory` and `mcl-llm`. The outside
engineer could cleanly bind only `mcl-spec`/`mcl-parse`/`mcl-signal`. His one-thing-to-tell-
the-author: split the pure deterministic core into a dependency-free kernel, because "the
moment MCL is a two-line dependency in Python, TypeScript, and Go, 'the engine enforces'
stops being your framework's feature and becomes the industry's default."

The Stage 3 roadmap already points here — M12.1 calls for an `mcl-validate` crate with
"zero dependency on mcl-graph's Pregel executor or any RPG type" — but names the product
surface without naming the crate-graph surgery that makes it possible. The moonshot skeleton
(Phase 0) names the surgery: a kernel firewall. These are one decision.

## Decision

- **Create `mcl-kernel`:** a zero-I/O, zero-async, zero-ML crate re-exporting the pure core —
  `mcl-spec`, the canonical parser (ADR-003), `mcl-signal`, the registry's arbiter chain +
  content-addressed log, and `mcl-policy`. Everything effectful (`mcl-graph`, `mcl-memory`,
  `mcl-llm`, `mcl-server`, `mcl-nodes`, `mcl-domains`) becomes a *client* of the kernel,
  never a dependency of it.
- **Registry surgery:** the arbiter chain and signal log split from the vector-index/
  causality-graph halves of `mcl-registry` (which legitimately need usearch/fastembed), so
  the verdict path is pure. Feature flags are acceptable as a transition; separate crates are
  the end state if flags can't keep the tree clean.
- **`mcl-validate`** (roadmap M12.1) is the standalone CLI/product surface **on top of** the
  kernel: schema + transcript in, accept/reject/quarantine verdicts + content-addressed log
  out. It is the proof the firewall holds, not a parallel extraction.
- **The gate is mechanical:** a CI assertion (`cargo tree`-based) that `mcl-kernel` resolves
  no tokio, reqwest, fastembed, usearch, or rand-seeded-from-entropy dependency. The
  firewall is a test, not a convention.

## Consequences

- Unblocks ADR-005 (grammar compiler consumes the kernel's world model), ADR-007 (bindings
  bind the kernel, not five crates), ADR-011 (the checker analyses kernel types), and the
  framework adapters (roadmap M15).
- WASM builds become trivial rather than aspirational — the kernel is `no_std`-adjacent by
  construction even if not literally `no_std`.
- Cost: a real refactor across the workspace's dependency graph, and `mcl-registry`'s
  public API splits. This is Stage 3's critical-path engineering item.

## Roads not taken

- **Feature-flag the existing crates instead of a kernel crate.** Considered as a
  transition mechanism only: flags don't give downstream users a single obvious artifact to
  depend on, and flag-matrix rot is a standing tax.
- **Extract mcl-validate directly without the kernel boundary** (M12.1 as written).
  Rejected: it would duplicate the pure core's surface inside a second crate and leave the
  PyO3/WASM bindings still blocked on the dirty tree.
