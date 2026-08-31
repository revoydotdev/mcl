# 02 — Stage 4: proof & provenance

*Direction-setting for the stage after Stage 3. Executes external-review moonshot Phases
3–5. ADRs: 009 (effects), 010 (verifiable ledger), 011 (`mcl verify`). Task-level detail is
deliberately thin — it gets cut when Stage 3's exit criteria are real.*

---

## 1. Why this stage exists

Stage 3 makes the layer *installable everywhere* and *unlie-able-to at generation time*.
Stage 4 makes it *provable after the fact and before the fact* — the artifacts that let an
agent into a hospital, a bank, or an audit: a session you can rewind and fork bit-for-bit,
a transcript a third party can verify without trusting your machine, and a world that
carries a proof about what can never happen.

This is where MCL stops competing with anything on the market, because nothing on the
market produces these artifacts. It is also premature to build before Stage 3: all three
pillars want the kernel (the verifier and checker must build anywhere), the blessed parser
(proofs about *which* grammar?), and the battery (claims discipline).

## 2. The three pillars

### S4.1 — Honor the effect system *(ADR-009; moonshot Phase 3)*
Runtime consults `Effect` per action: memoize `Pure`, content-address `Cacheable` (cache
hits recorded in the log — provenance preserved), quarantine `ReplayUnsafe` at replay,
dedup on `IdempotencyKey`. `mcl registry replay --at <turn> --fork` + two-run diff;
per-session cost ledger in `registry inspect`.
**First commit** (per the skeleton): fork-at-turn reconstructing state bit-for-bit.
**Exit:** replay-consistency scenarios in the battery; a long session demonstrably never
re-pays for a pure/cacheable result. *(diff 55, HIGH once scheduled)*

### S4.2 — The verifiable ledger *(ADR-010; moonshot Phase 5)*
Merkle-chain the accepted-signal log (chain tip per campaign, optional signing);
`mcl audit export` producing a portable transcript (world hash + policies + chained log +
verdicts); an independent kernel-only verifier CLI that checks a transcript with no access
to the original runtime. Mirror the chain-tip pattern revoy already runs (`revoychain`).
**First commit:** chain-tip + verify over `signals.jsonl`.
**Exit:** a transcript tampered at any record fails verification; an untouched one verifies
on a machine that has never run MCL. *(diff 45, HIGH)*

### S4.3 — `mcl verify` and certified worlds *(ADR-011; moonshot Phase 4)*
Bounded model checker over the declared action/transition system; invariant + reachability
+ deadlock queries; counterexample traces replayable in the runtime; certificates (world
hash → invariants → result → bound) embeddable in S4.2's export. First invariants from the
existing examples: PHI-export gate, budget non-negativity, no-deadlock.
**First commit:** bounded checker for one invariant + a counterexample printer.
**Exit:** `mcl verify` returns a machine-checkable verdict on ≥3 real example worlds; a
seeded violation yields a concrete replayable trace. *(diff 75, HIGH — the hardest single
item in the whole roadmap)*

## 3. Sequencing

S4.2 first (three commits from existing material, immediate demo value), S4.1 second
(kernel-type surface work), S4.3 last and longest. All three can interleave with late
Stage 3 marketing/publish work; none may start before the kernel gate (S3.2) is green.

The intel monitor (`mcl-decoding-sota`) doubles as the research feed for S4.3 — verifier-
guided generation and agent-verification literature moves fast; the checker's method
should be chosen against 2027 SOTA, not 2026 assumptions.

## 4. Exit picture

A user can: run an agent under MCL, rewind and fork any turn, export the session, hand it
to an auditor who verifies it independently, and attach a certificate that the world the
agent ran in cannot reach a forbidden state. When that loop closes, "governance-grade" is a
demo, not a slide.
