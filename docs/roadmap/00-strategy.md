# 00 — Strategy: what MCL is, where it stands, how it wins

*2026-07-07. Evidence-first: every claim below traces to a repo artifact, a measured
result, or a named external review.*

---

## 1. Where MCL actually stands

**Built and verified.** 16 crates, ~30k LOC Rust. Stage 1 (foundation, language, graph,
memory) closed out; Stage 2 (registry, arbiter chain, cross-session persistence, governing
nodes, entity-scoped memory) landed through M2.10 — M2.11 (docs/dogfooding) in progress,
M2.12 (publish) open. Determinism is tested, not claimed (`determinism.rs`, 100×
byte-identical). The signal log is blake3 content-addressed and append-only. The policy DSL
type-checks and ships non-game examples (`phi_export_gate`, `budget_policy`, `rag_defense`,
`confidence_routing`).

**Measured.** The 2026-04-22 sweep (9 models, 540 turns, one RPG fixture): accept-rate
41.7%–98.3%; **zero malformed** signals; 2/540 hallucinated; dominant failure is
*under-emission*; parameter size does not predict reliability — instruction-following
literalism does. Read both ways: the existential "can LLMs do this at all" is answered
(DeepSeek v3: 98.3% untuned), and prompt-only inline signaling has a ceiling that will not
close by prompting harder.

**Externally validated.** An outside engineer (external-review/) re-derived the signal
grammar and arbiter in one session, bound the real crates via PyO3, and cross-checked with
**zero differences** on real transcripts — then wrote: *"You have built the SQL of LLM
behavior constraints… a category, not a game engine."* The same session produced the
sharpest friction list MCL has (two parsers; the arbiter unbindable without dragging
fastembed/tokio; R2 spec drift; silent degrade with no feedback loop; a coercion
inconsistency).

**Adopted once.** revoy ADR-053: revoy adopts MCL's model for coding-agent governance via
the `revoymcl` bridge. One real, non-fiction consumer — the wedge.

**Not done.** Nothing published (0.1.0 everywhere); README badges/release tags/changelog
are open `revoy audit` gaps; the reliability battery has run once, on one domain, in one
mode; the RunPod battery deferral (HIGH, 2026-06-27) is still open.

## 2. The identity decision (ADR-002)

MCL is a **signal-validation layer**: it proves that an LLM's emitted actions are declared,
typed, policy-clean, and replayable — under any framework, any provider, any host language.
Interactive fiction is the reference application and test domain, never the pitch.

Three fights MCL deliberately does not pick:

- **Not** "less abstraction than a raw SDK" — Rust + a DSL never out-simples `pip install`.
- **Not** "a better LangGraph" — workflow breadth is their game; the honest comparison-doc
  verdicts stay.
- **Not** a vertical (IF/EdTech/compliance-training product) — verticals are *customers*
  of the layer.

The fight it does pick, because nobody else holds all four cards at once: **deterministic
replay + knowledge-boundary scoping + content-addressed provenance + a declared world that
compiles to a decoding constraint.** Guardrails/Instructor validate shapes after the fact;
Outlines/XGrammar constrain shape without world semantics, provenance, or replay; LangFuse
observes without enforcing. MCL's registry/arbiter core already does what they bolt on.

## 3. The strategy, in one sequence

**Kernel → constraint → bindings → proof → composition.** Each step multiplies the next:

1. **The kernel firewall (ADR-004)** makes the pure core (spec, parser, signal, arbiter+log,
   policy) a dependency-free artifact. It fixes the #1 external friction, unblocks every
   binding and adapter, and is the precondition for everything below. *Highest-leverage
   refactor in the repo.*
2. **Generation-time constraint (ADR-005)** converts MCL's unique asset — the world file IS
   the grammar — into the flagship feature: the lie becomes unsamplable, not merely caught.
   It also converts the sweep's 56-point model spread from a product risk into a
   compiler-correctness problem testable in CI.
3. **Bindings (ADR-007)** make "the engine enforces" a two-line import where agents actually
   live (Python/TS). The revoymcl prototype proves the cost is an afternoon; productizing it
   is packaging, not research.
4. **Measured reliability (ADR-008)** earns every public claim: non-RPG corpora, adversarial
   catch-rate, recurring cadence, generated leaderboard. "Ultimate" is a measurement regime,
   not an adjective.
5. **Proof & provenance (ADR-009/010/011)** — replay/fork/cost, the Merkle-chained portable
   audit, `mcl verify` — build the artifacts regulated adoption will demand and no
   competitor produces.
6. **Composition (ADR-012)** turns governance into a standard library. Endgame; sequenced
   last on purpose.

The flagship *proof point* throughout is revoy itself (M15.3): a real coding-agent
governance problem, in-family, with its own ADR trail. Dogfood before pitch.

## 4. Risks, named

| Risk | Reality check | Mitigation |
|------|--------------|------------|
| Generalization is asserted, not proven | All reliability data is one RPG fixture | Battery 2.0 non-RPG corpora are VHIGH and early (ADR-008) |
| Kernel refactor stalls (it touches everything) | mcl-registry's split is genuinely invasive | It leads Stage 3; nothing later is allowed to start ahead of the firewall gate |
| Providers absorb the layer (structured outputs get "good enough") | OpenAI/Anthropic keep hardening schemas | Provider schemas prove *shape*; MCL proves *declared-world semantics + provenance + replay* — keep the comparison doc honest on exactly this line; the intel monitor watches for erosion |
| A LangGraph-native clone appears (April eval: 70–80% in weeks) | The code is replicable; the decisions compound | Speed on kernel+bindings (the part that takes months to get right), plus the proof artifacts (ADR-010/011) nobody clones in weeks |
| Single-author bus factor & attention | One person, many repos | revoy ledger discipline; small, exit-criteria'd phases; publish early (M16.1) so the project survives attention gaps |

## 5. What "ultimate" means, measurably

The Stage 3 mandate's six criteria stand as the acceptance definition: domain-agnostic,
provider-agnostic, framework-agnostic, reliability-measured (recurring, not snapshot),
adversarially tested, generation-time-hardened. Each stage doc closes with which of the six
it advances. When all six hold with published evidence, the word is earned.
