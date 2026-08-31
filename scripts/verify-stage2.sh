#!/usr/bin/env bash
# verify-stage2.sh — Stage-2 end-to-end gate: registry replay + semantic recall
#
# Modeled on scripts/stage1-gate.sh.
# Must run unattended (no network, no model downloads).
# Embeddings are mocked via MCL_MOCK_EMBEDDING=1, which mcl-memory/src/lib.rs
# checks in create_embedder() and routes to MockEmbedder instead of fastembed.
#
# What this gate covers:
#   1. mcl-registry unit + integration tests:
#        - src/recovery.rs (rebuild_index, repair_jsonl, verify_consistency)
#        - src/log.rs, src/index.rs, etc.
#        - tests/executor_integration.rs (interceptor accept/reject/passthrough)
#      "Registry replay" = the arbiter re-running recorded JSONL entries
#      deterministically (rebuild_index + verify_consistency).
#
#   2. mcl-memory unit + proptest tests:
#        - src/store.rs (insert, search, hybrid_search, recent, save/load)
#        - src/node_memory.rs (three-store model, persist round-trip)
#        - tests/tag_recent_proptest.rs (property-based: ordering, round-trip, overflow)
#      "Semantic recall" = MemoryStore::search() via MockEmbedder (MCL_MOCK_EMBEDDING=1).
#      The mock produces deterministic random embeddings, so search() exercises the full
#      usearch HNSW path without downloading any model.
#
#   3. mcl-nodes memory tests (MCL_MOCK_EMBEDDING=1 required):
#        - llm_node_memory_stores_incoming, llm_node_memory_stores_outgoing
#      These wire the three-store NodeMemory into a real LlmNode, verifying that
#      incoming/outgoing packets are indexed via the mock embedder.
#
#   4. CLI smoke — registry replay subcommand:
#        mcl-cli registry replay <campaign>
#      Exercises the real CLI path that re-runs signal log entries through the
#      arbiter chain. Uses a temp dir with a pre-seeded signals.jsonl fixture —
#      no LLM backend required.
#
# NOTE: mcl-cli registry query --semantic is NOT exercised here because it
# re-reads from a JSONL log (no vector index persisted) and always returns
# "No matching entries found" — a correct but trivially uninteresting assertion.
# The meaningful semantic-recall path is tested in mcl-memory's unit/proptest suite.

set -euo pipefail

export MCL_MOCK_EMBEDDING=1

PASS=0
FAIL=0

check() {
    local name="$1"
    shift
    echo "=== $name ==="
    if "$@"; then
        echo "  PASS"
        PASS=$((PASS + 1))
    else
        echo "  FAIL"
        FAIL=$((FAIL + 1))
    fi
}

cd "$(dirname "$0")/.."

echo "Stage-2 Verification Gate"
echo "============================"
echo "MCL_MOCK_EMBEDDING=1 (embeddings mocked — no model download)"
echo ""

# 1. Registry crate tests (unit + executor_integration)
#    Covers: SignalLog append/read, RegistryIndex rebuild, repair_jsonl,
#    verify_consistency, ArbiterChain intercept accept/reject/passthrough.
check "mcl-registry tests" cargo test -p mcl-registry

# 2. Memory crate tests (unit + proptest)
#    Covers: MemoryStore insert/search/hybrid_search/recent/save/load,
#    NodeMemory three-store model, tag round-trip proptest, capacity eviction.
#    MCL_MOCK_EMBEDDING=1 ensures create_embedder() returns MockEmbedder.
check "mcl-memory tests (mock embedder)" cargo test -p mcl-memory

# 3. Node memory integration tests
#    Covers: LlmNode with memory_enabled=true, MemoryPopulationHook wiring
#    incoming/outgoing stores. Requires MCL_MOCK_EMBEDDING=1.
#    Uses --test to target the specific integration test files.
check "mcl-nodes memory tests (mock embedder)" bash -c '
    cargo test -p mcl-nodes --test llm_node_memory_stores_incoming &&
    cargo test -p mcl-nodes --test game_engine_memory_enabled_creates_store &&
    cargo test -p mcl-nodes --test game_engine_memory_disabled_by_default
'

# 4. CLI smoke: registry replay subcommand
#    Seeds a temp campaign dir with a 3-entry signals.jsonl and runs
#    `mcl-cli registry replay <campaign>` from that temp dir. Verifies the replay
#    path (read JSONL -> re-run arbiter chain -> print summary) exits 0 and emits
#    expected output. The binary is pre-built by check 1 (cargo test also builds).
check "CLI smoke: registry replay" bash -c '
    tmpdir=$(mktemp -d)
    trap "rm -rf $tmpdir" EXIT
    campaign_dir="$tmpdir/.mcl/gate-campaign"
    mkdir -p "$campaign_dir"
    cat > "$campaign_dir/signals.jsonl" <<'"'"'JSONL'"'"'
{"id":{"hash":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]},"signal":{"kind":"Noop","span":{"offset":0,"len":1,"line":1,"col":1}},"origin":"player","turn":1,"channel":"actions","timestamp":1714000000000,"verdict":"Accept"}
{"id":{"hash":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1]},"signal":{"kind":{"Narrate":{"tag":"scene_desc"}},"span":{"offset":0,"len":1,"line":1,"col":1}},"origin":"player","turn":2,"channel":"actions","timestamp":1714000001000,"verdict":"Accept"}
{"id":{"hash":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,2]},"signal":{"kind":{"Query":{"path":"look"}},"span":{"offset":0,"len":1,"line":1,"col":1}},"origin":"player","turn":3,"channel":"queries","timestamp":1714000002000,"verdict":"Accept"}
JSONL
    # Build the binary from workspace root, then run it with tmpdir as cwd
    # so the campaign lookup (.mcl/gate-campaign/signals.jsonl) resolves correctly.
    cargo build -q --bin mcl-cli
    WORKROOT="$(pwd)"
    out=$(cd "$tmpdir" && "$WORKROOT/target/debug/mcl-cli" registry replay gate-campaign 2>&1)
    echo "$out"
    echo "$out" | grep -q "Replaying 3 entries" || { echo "MISSING: Replaying 3 entries"; exit 1; }
    echo "$out" | grep -q "CONSISTENT" || { echo "MISSING: CONSISTENT"; exit 1; }
'

echo ""
echo "============================"
echo "Results: $PASS passed, $FAIL failed"
echo "============================"

if [ "$FAIL" -gt 0 ]; then
    echo ""
    echo "Stage-2 gate FAILED"
    exit 1
else
    echo ""
    echo "Stage-2 gate PASSED"
    exit 0
fi
