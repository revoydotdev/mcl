# 03 — Stage 5: composition & the governance standard library

*Endgame direction. Executes external-review moonshot Phase 6. ADR: 012. Thinnest of the
series by design — everything here is contingent on Stages 3–4 landing and on external
adoption signals that cannot be scheduled.*

---

## 1. The thesis

Once the layer is embeddable (Stage 3) and provable (Stage 4), the remaining question is
*who writes the worlds*. Today: each team, by hand, from scratch. Endgame: governance is a
standard library — a base "hospital" world, a "cardiology" extension, a "night-shift"
overlay, imported and composed like crates, with the arbiter enforcing every layer.

The pieces already hint at it: `extends` inheritance is wired (S3.1 makes it canonical),
policies are declarative and typed, resolvers are composable nodes. What's missing is the
semantics (safe composition) and the ecosystem (vetted overlays).

## 2. The work

### S5.1 — Composition semantics *(ADR-012)*
Formal merge/override rules for `extends` + policy overlays, with the **monotonicity
guarantee**: an overlay can only narrow, never widen, what the composed world permits. The
arbiter federates — a signal passes every overlay's chain; verdicts record which layer
rejected. **First commit:** two composable overlays + a test that a signal must satisfy
both. *(diff 50)*

### S5.2 — The overlay standard library
Grow the existing examples into real, versioned overlays: PHI/HIPAA-shaped export gating,
spend-confirmation/budget, RAG source-trust. Each ships with its invariants pre-verified
(`mcl verify`, S4.3) — a certified overlay is the unit of trust. *(diff 55)*

### S5.3 — Distribution (`mcl add <overlay>`)
Registry tooling, semver for policies, conformance badges. **Gated on ≥2 overlays having
real external users** — distribution follows demand. A marketplace of unvetted governance
modules would be a liability in exactly the regulated domains the layer targets. *(diff 60)*

## 3. Commercial posture (kept honest)

The April 2026 evaluation's market finding stands: the layer's buyers are teams deploying
agents into consequence-bearing domains. Stage 5 is where a commercial wrapper becomes
*possible* (certified overlays, verification-as-a-service, support) — but this roadmap
commits only to the open substrate. A monetization decision, when real, gets its own ADR
with actual market evidence; it is deliberately absent from ADR-001…012.

## 4. Exit picture

`.mcl` worlds and overlays are things you `use`, not things you write from scratch; a
compliance officer reads a certificate rather than a promise; and MCL is boring — which,
per the moonshot's own framing ("as boring, ubiquitous, and load-bearing as SQL"), is what
winning looks like.
