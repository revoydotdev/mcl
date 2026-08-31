---
adr: 3
title: "One grammar, one source of truth: bless mcl-lang as the canonical parser"
status: Proposed
implementation: Design-only
date: 2026-07-07
last_reviewed: 2026-07-07
amends: []
supersedes: null
superseded_by: null
tags: [parser, language, kernel]
decision: "mcl-lang is the canonical .mcl parser/validator/emitter; mcl-parse is frozen as a legacy shim until its callers (mcl-cli, mcl-wasm) migrate, then deleted. A spec-parity conformance test fails the build when docs/language-reference.md and mcl-spec/document.rs diverge (R2 drift class)."
---
# ADR-003 — One grammar, one source of truth: bless mcl-lang as the canonical parser

> **Status:** Proposed · **Implementation:** Design-only · **Reviewed:** 2026-07-07

## Context

The repo carries two `.mcl` parsers. `mcl-parse` is the legacy, hand-rolled, line-oriented
parser that `mcl-cli` and `mcl-wasm` actually call today; `mcl-lang` is the newer
lexer/parser/validator/emitter where `extends` inheritance is really wired. The external
postmortem (friction #1) is blunt: an integrator had to read both to decide what to bind —
"a reader shouldn't have to reverse-engineer which of two parsers is the future."

The same postmortem found the sibling failure at the spec level (friction #3): the R2
additions (label lattice, degradation rules, `policy {}` blocks) exist in
`mcl-spec/document.rs` but not in `docs/language-reference.md` — the policy-block syntax was
learnable only from `examples/phi_export_gate/clinical_export.mcl`. And a smaller grammar
inconsistency (friction #5): bare positional action args bypass the value coercion that
`key=val` kwargs get.

Both are the same disease: **more than one source of truth for the language.** A validation
layer whose own grammar has two half-authoritative definitions cannot credibly sell
"one declared world, mechanically enforced."

## Decision

- **`mcl-lang` is the canonical parser/validator/emitter.** All first-party callers
  (`mcl-cli`, `mcl-wasm`, `mcl-server`, the kernel of ADR-004) migrate to it.
- **`mcl-parse` is frozen** — bugfix-only, no new syntax — and retained solely as a
  fixture-migration shim until callers are moved, then deleted. Deletion, not indirection.
- **A spec-parity conformance gate:** a test that walks the constructs `mcl-spec/document.rs`
  can represent and fails when `docs/language-reference.md` lacks a section for one (and
  vice versa for documented-but-unparseable syntax). The R2 features get documented as part
  of landing the gate.
- **The positional-arg coercion divergence is resolved** — bare positional args coerce
  through the same value pipeline as kwargs, or the divergence is documented as intentional
  with a lint. Two spellings of one intent must not produce differently-typed values.

## Consequences

- Every downstream binding (ADR-007) and the grammar compiler (ADR-005) build against one
  grammar; the cross-runtime conformance corpus has a single reference semantics.
- Migration risk on `mcl-cli`/`mcl-wasm` behavior: the corpus of example worlds + existing
  fixtures is the regression net and must pass byte-identically under `mcl-lang` before the
  switch.
- Short-term: carrying the frozen crate until migration completes.

## Roads not taken

- **Bless `mcl-parse` (it's what production calls).** Rejected: it lacks `extends`, has no
  validator/emitter architecture, and its line-grammar subtleties (decorated-vs-prose section
  headers) are exactly the kind of thing the newer lexer disambiguates properly.
- **Keep both indefinitely** ("legacy for stability, lang for features"). Rejected: that is
  the current state, and it already cost an integrator real time; duplicated knowledge, not
  duplicated text.
