---
adr: 6
title: "Generalize ActionResolver to a schema-shaped ToolResolver trait"
status: Proposed
implementation: Design-only
date: 2026-07-07
last_reviewed: 2026-07-07
amends: []
supersedes: null
superseded_by: null
tags: [api, tools, mcp, resolver]
decision: "Replace the RPG-shaped ActionResolver with a ToolResolver trait that maps 1:1 onto JSON-schema tool/function definitions (name, typed args, return contract), so an MCP tool list or an OpenAI/Anthropic tool schema loads directly; combat/inventory become just two implementations (roadmap M12.2)."
---
# ADR-006 — Generalize ActionResolver to a schema-shaped ToolResolver trait

> **Status:** Proposed · **Implementation:** Design-only · **Reviewed:** 2026-07-07

## Context

The `ActionResolver` trait is shaped by its first two implementations — combat and
inventory. Its vocabulary (targets, outcomes modeled on RPG resolution) leaks the demo
domain into the layer that ADR-002 declares domain-agnostic. Roadmap M12.2 already calls
for the generalization; the industry has meanwhile standardized the shape it should
generalize *to*: the JSON-schema tool/function definition (name, typed args, return
contract) used by MCP tool lists and OpenAI/Anthropic tool-use APIs.

If MCL's resolver trait maps 1:1 onto that shape, an MCP server's tool list — or a
framework's tool schema — loads directly as a declared action vocabulary, which is the
on-ramp every adapter in roadmap M15 needs.

## Decision

- Introduce a **`ToolResolver`** trait whose contract mirrors the JSON-schema tool
  definition: declared name, typed argument schema, return contract, and an `Effect`
  annotation (feeding ADR-009).
- **Loaders** accept an MCP tool list or OpenAI/Anthropic tool-use schema directly as a
  world's action vocabulary — no `.mcl` rewrite required to validate an existing agent's
  tools.
- The RPG resolvers (`mcl-domains`) are reimplemented as `ToolResolver` implementations —
  the demo becomes proof the trait is sufficient, not the trait's designer.
- `ActionResolver` is deprecated with a migration window, then removed; one trait, not two.

## Consequences

- The LangGraph adapter (M15.1) and raw-SDK wrapper (M15.2) become thin: they hand MCL the
  tool schema they already have.
- Argument typing gets stricter than the current RPG-string-flavored resolution — this
  intersects the positional-arg coercion fix in ADR-003.
- Breaking change to `mcl-domains` internals; acceptable pre-1.0, invisible to `.mcl`
  authors.

## Roads not taken

- **Keep ActionResolver and add an adapter shim per framework.** Rejected: N shims around a
  domain-shaped core, versus one core shaped like the standard everyone already emits.
- **Invent an MCL-native tool ontology richer than JSON-schema.** Rejected: the extra
  expressiveness lives in `.mcl` policies and knowledge boundaries, not in the resolver
  contract; a bespoke ontology would tax every adapter.
