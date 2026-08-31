#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

MCL_MOCK_EMBEDDING=1 cargo test -p mcl-cli -- playtest --nocapture
