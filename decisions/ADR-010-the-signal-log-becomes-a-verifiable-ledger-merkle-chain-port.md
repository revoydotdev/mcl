---
adr: 10
title: "The signal log becomes a verifiable ledger: Merkle chain, portable export, independent verifier"
status: Proposed
implementation: Design-only
date: 2026-07-07
last_reviewed: 2026-07-07
amends: []
supersedes: null
superseded_by: null
tags: [audit, merkle, provenance, governance]
decision: "Merkle-chain the accepted-signal log with optional signing; mcl audit export produces a portable transcript verifiable by a third-party CLI against the world+policies with no access to the original runtime. Governance-grade audit is the moat the comparison doc says nobody else has (Stage 4)."
---
# ADR-010 — The signal log becomes a verifiable ledger: Merkle chain, portable export, independent verifier

> **Status:** Proposed · **Implementation:** Design-only · **Reviewed:** 2026-07-07

## Context

Every accepted signal is already blake3 content-addressed
(`origin‖turn‖channel‖signal`) and appended to a per-campaign JSONL log — a tamper-evident
audit trail that "falls out of the design rather than bolted on" (postmortem). But it is a
*local file*: per-record hashes without a chain tip, no signing, no portable export, and no
way for a third party to verify a transcript without trusting the machine that produced it.

The moonshot's fifth horizon and the direction of AI-governance requirements point the same
way: "prove to me what this agent did, and that every action was declared and every policy
held" as a cryptographic artifact. revoy's own ledgers already use chain-tip + verify over
JSONL (`revoychain`), so the pattern is proven in-family. MCL is, as the moonshot puts it,
"three commits from having it."

## Decision

- **Merkle-chain the accepted-signal log:** each entry commits to the previous chain tip;
  the campaign carries a head hash. Optional signing (age/minisign-class, pluggable) over
  the tip.
- **`mcl audit export`:** a portable, self-contained transcript artifact — world (or its
  content hash), policy set, chained signal log, and verdicts — designed to be handed to an
  auditor, a regulator, or the next agent in a pipeline.
- **Independent verifier:** a small CLI (kernel-only per ADR-004, so it builds anywhere)
  that checks an exported transcript against the world + policies with **no access to the
  original runtime** — recomputing the chain, re-running the arbiter verdicts, and
  reporting any divergence.
- Certification hooks: when ADR-011 lands, the exported artifact can embed the world's
  proof certificate, making "declared, policy-clean, and the world itself verified" one
  bundle.

Stage 4, sequenced after the kernel; the verifier is the kernel's first external showcase.

## Consequences

- Replay (ADR-009) and audit share the chain: a fork records its fork-point tip, keeping
  forked histories honest.
- Log format gains a versioned envelope — a migration for existing campaign logs.
- Key management for signing is deliberately out of scope (BYO keys); MCL attests
  integrity, not identity.

## Roads not taken

- **Anchor to an external/public ledger.** Rejected: operational and ideological baggage
  for zero added verification power in the target use cases; a signed Merkle tip already
  gives tamper-evidence.
- **Leave verification to the runtime** (`mcl registry verify` only). Rejected: an audit
  trail only checkable by the software that wrote it convinces nobody outside the team.
