---
adr: 7
title: "Bindings strategy: mcl-py (PyO3/maturin) is the flagship embedding; one conformance corpus for every runtime"
status: Proposed
implementation: Design-only
date: 2026-07-07
last_reviewed: 2026-07-07
amends: []
supersedes: null
superseded_by: null
tags: [bindings, pyo3, wasm, python, conformance]
decision: "Productize the proven revoymcl binding as mcl-py (pip install mcl, maturin-built), promote the PyO3 path to primary with pure-Python as offline fallback and differential oracle (roadmap M12.3); ship mcl-wasm as an npm package second; one corpus of worlds+transcripts+expected-verdicts must pass identically on every binding."
---
# ADR-007 — Bindings strategy: mcl-py (PyO3/maturin) is the flagship embedding; one conformance corpus for every runtime

> **Status:** Proposed · **Implementation:** Design-only · **Reviewed:** 2026-07-07

## Context

The strongest external validation MCL has: an outside engineer mirrored the parser, signal
grammar, and arbiter chain in Python in one working session, bound the real crates via
PyO3, and the cross-check passed with **zero differences** on real transcripts. That
prototype (`revoymcl`, now living in revoy-spec and consumed by revoy ADR-053 item 9) is
simultaneously proof the binding is cheap and an unproductized dead end: no
pyproject/maturin backend (built by `cargo build` + manual `.so` copy), a pyo3 version pin
that rejected Python 3.14 until bumped, and the pure-Python mirror still the default path.

Roadmap M12.3 wants the PyO3 path primary. The moonshot (horizon 3) wants further: one
kernel, every language. `mcl-wasm` compiles but is a sketch. The kernel firewall (ADR-004)
is what makes any of this clean.

## Decision

- **`mcl-py` is the flagship binding:** a maturin-built PyO3 package over `mcl-kernel`,
  published to PyPI as `pip install mcl` (name availability permitting), exposing parse /
  extract / arbitrate / policy-eval over the serde surface.
- **Path promotion (M12.3):** in `revoymcl` and any Python consumer, the compiled extension
  is the default execution path; the pure-Python mirror is demoted to (a) offline fallback
  and (b) differential test oracle. `revoymcl/CONTRACT.md` is extended to a committed
  interface.
- **`mcl-js` second:** the existing `mcl-wasm` surface ships as an npm package once the
  kernel exists; no new capability, just packaging of the kernel's.
- **One conformance corpus:** a single set of `.mcl` worlds + transcripts + expected
  verdicts, run against Rust, Python, and WASM in CI; a binding that diverges from the
  reference verdicts is a release blocker. The corpus is also the regression net for the
  ADR-003 parser migration.
- Go/JVM bindings are explicitly deferred until an actual consumer exists.

## Consequences

- "The engine enforces" becomes a two-line import in the two ecosystems where agent
  frameworks actually live (Python, TypeScript) — the precondition for every M15 adapter.
- Toolchain surface grows (maturin, npm publishing, abi3 wheels); CI must build wheels.
- The differential-oracle arrangement means the Python mirror must be maintained *enough*
  to stay a valid oracle — bounded by the conformance corpus, not by feature parity.

## Roads not taken

- **Pure-Python reimplementation as the product** (skip the binding). Rejected: two
  implementations drift; the zero-difference cross-check is valuable as a *test*, ruinous
  as a *product surface*.
- **gRPC/HTTP sidecar instead of bindings.** Rejected: a validation layer that adds a
  network hop and a daemon to every tool call prices itself out of the "same abstraction,
  now proven" pitch (raw-SDK wrapper, M15.2).
