# MCL roadmap series

Planning, reasoning, and strategy for MCL from Stage 3 forward. Written 2026-07-07 from a
full-project evaluation; each document cites the evidence it plans from and the ADR that
ratifies each load-bearing choice.

## The documents

| Doc | Scope | Governs |
|-----|-------|---------|
| [00-strategy.md](00-strategy.md) | Where MCL actually is, what it is, and how it wins | Everything below |
| [01-stage-3-kernel-and-constraint.md](01-stage-3-kernel-and-constraint.md) | Stage 3 execution: kernel firewall, generation-time constraint, bindings, battery 2.0, adapters, publish | Current ledger phase |
| [02-stage-4-proof-and-provenance.md](02-stage-4-proof-and-provenance.md) | Stage 4: effects/replay, `mcl verify`, verifiable ledger | Next phase after Stage 3 |
| [03-stage-5-composition.md](03-stage-5-composition.md) | Stage 5: composable worlds, overlay stdlib, federated arbiter | Endgame |
| [04-ledger-proposals.md](04-ledger-proposals.md) | Proposed revoy ledger mutations, as runnable `revoy` commands | Awaiting approval |

## How this series relates to everything else

- **[`ROADMAP.md`](../../ROADMAP.md)** stays the repo's plan of record and carries the revoy
  phase block (the machine-read ledger). Its Stage 3 mandate section is the *mandate*; this
  series is the *plan* that executes it — sequencing, reasoning, exit criteria. Milestones
  1–11 and [`STAGE-2-TODOS.md`](../../STAGE-2-TODOS.md) remain historical record.
- **[`decisions/`](../../decisions/)** holds the ADR series (ADR-001…012, all **Proposed**)
  created alongside this roadmap via `revoy adr new`. Each stage doc references the ADRs it
  executes; accept or reject them individually with `revoy adr` before treating the plan as
  ratified.
- **[`external-review/`](../../external-review/)** (postmortem, moonshot, roadmap skeleton)
  is the outside input this series reconciles with the existing Stage 3 mandate. The
  reconciliation table lives in [01-stage-3-kernel-and-constraint.md](01-stage-3-kernel-and-constraint.md).
- **revoy**: MCL is enrolled as `MCL`; ledger mutations happen only through `revoy todo` /
  `revoy defer` / phase-block edits, per the protocol. [04-ledger-proposals.md](04-ledger-proposals.md)
  contains exact commands, deliberately *not yet run*.

## Reading order

Strategy first (00), then the stage matching the current ledger phase (01). Stages 4 and 5
are direction-setting, intentionally lighter on task detail — their detail gets cut when
their preconditions are real.
