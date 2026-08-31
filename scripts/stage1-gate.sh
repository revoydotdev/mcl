#!/usr/bin/env bash
set -euo pipefail

PASS=0
FAIL=0

check() {
    local name="$1"
    shift
    echo "=== $name ==="
    if "$@"; then
        echo "  PASS"
        ((PASS++))
    else
        echo "  FAIL"
        ((FAIL++))
    fi
}

cd "$(dirname "$0")/.."

echo "M2.0 Stage 1 Closeout Gate"
echo "============================"
echo ""

# 1. cargo fmt --check
check "cargo fmt --check" cargo fmt --check

# 2. cargo clippy
check "cargo clippy --workspace -- -D warnings" cargo clippy --workspace -- -D warnings

# 3. cargo test
check "cargo test (with MCL_MOCK_EMBEDDING=1)" bash -c 'MCL_MOCK_EMBEDDING=1 cargo test --workspace'

# 4. Check for unwrap() in production code
check "no unwrap() in production src/" bash -c '
    count=0
    for file in $(find crates/*/src -name "*.rs"); do
        basename=$(basename "$file")
        # Skip test files
        if [[ "$basename" == tests.rs ]] || [[ "$basename" == test_*.rs ]]; then
            continue
        fi
        
        # Check if file has unwrap() outside of #[cfg(test)] modules
        if grep -q "#\[cfg(test)\]" "$file"; then
            # File has test module, check for unwrap() before it
            test_start=$(grep -n "#\[cfg(test)\]" "$file" | head -1 | cut -d: -f1)
            unwrap_before=$(head -n $((test_start - 1)) "$file" | grep -c "\.unwrap()" || true)
            if [ "$unwrap_before" -gt 0 ]; then
                echo "$file: $unwrap_before unwrap() before test module"
                count=$((count + 1))
            fi
        else
            # File has no test module, check for any unwrap()
            unwrap_count=$(grep -c "\.unwrap()" "$file" || true)
            if [ "$unwrap_count" -gt 0 ]; then
                echo "$file: $unwrap_count unwrap() (no test module)"
                count=$((count + 1))
            fi
        fi
    done
    [ "$count" -eq 0 ]'

# 5. Verify playtest baseline exists
check "playtest-stage2-baseline.jsonl exists" test -f playtest-stage2-baseline.jsonl

# 6. Optional Playwright smoke test
if [ "${PLAYWRIGHT:-0}" = "1" ]; then
    check "playwright smoke test" bash scripts/stage2-playwright.sh
fi

echo ""
echo "============================"
echo "Results: $PASS passed, $FAIL failed"
echo "============================"

if [ "$FAIL" -gt 0 ]; then
    echo ""
    echo "M2.0 gate FAILED - Stage 1 not ready for M2.1"
    exit 1
else
    echo ""
    echo "M2.0 gate PASSED - Stage 1 closeout complete"
    exit 0
fi
