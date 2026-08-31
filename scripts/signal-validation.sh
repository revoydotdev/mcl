#!/usr/bin/env bash
# Signal reliability validation across OpenRouter models.
# Sweep a random sampling of models spanning the parameter spectrum
# (3B → 70B+ flagships) through mcl-cli bench, capture per-model
# summary + raw transcript into docs/signal-validation-<date>/.
#
# Requires: OPENROUTER_API_KEY exported in env.

set -u

: "${OPENROUTER_API_KEY:?set OPENROUTER_API_KEY before running}"

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${REPO_ROOT}/docs/signal-validation-$(date -u +%Y%m%d)"
CONTEXT="${REPO_ROOT}/examples/crypt/crypt_antechamber.mcl"
BASE_URL="https://openrouter.ai/api/v1"
RUNS="${RUNS:-3}"
COUNT="${COUNT:-20}"

mkdir -p "${OUT_DIR}"

# Sampling across the parameter spectrum.  Random-seed pick drawn from
# a curated pool so the run is reproducible from the list committed here.
MODELS=(
  "meta-llama/llama-3.2-3b-instruct|tiny-3b"
  "google/gemma-3-4b-it|tiny-4b"
  "meta-llama/llama-3.1-8b-instruct|small-8b"
  "mistralai/mistral-nemo|small-12b"
  "mistralai/mistral-small-3.2-24b-instruct|medium-24b"
  "meta-llama/llama-3.3-70b-instruct|large-70b"
  "openai/gpt-4o-mini|flagship-cheap"
  "anthropic/claude-3.5-haiku|flagship-prem"
  "deepseek/deepseek-chat-v3-0324|reasoning-moe"
)

SUMMARY="${OUT_DIR}/summary.tsv"
printf "tier\tmodel\taccuracy\tturns\tcorrect\tmalformed\tmissing\thallucinated\tstatus\n" > "${SUMMARY}"

cd "${REPO_ROOT}"

for entry in "${MODELS[@]}"; do
  MODEL="${entry%|*}"
  TIER="${entry#*|}"
  SAFE_NAME="${MODEL//\//__}"
  LOG_FILE="${OUT_DIR}/${TIER}__${SAFE_NAME}.log"

  echo "=== [${TIER}] ${MODEL} ==="

  if timeout 600 cargo run --release --quiet -p mcl-cli -- bench "${CONTEXT}" \
      --backend openai \
      --base-url "${BASE_URL}" \
      --api-key "${OPENROUTER_API_KEY}" \
      --model "${MODEL}" \
      --count "${COUNT}" \
      --runs "${RUNS}" \
      > "${LOG_FILE}" 2>&1; then
    STATUS="ok"
  else
    STATUS="fail"
  fi

  ACC="$(grep -oE 'Accuracy: [0-9.]+' "${LOG_FILE}" | head -1 | awk '{print $2}')"
  TURNS="$(grep -oE 'Turns:\s+[0-9]+' "${LOG_FILE}" | head -1 | awk '{print $2}')"
  CORRECT="$(awk '/Correct/ {print $2; exit}' "${LOG_FILE}")"
  MALFORMED="$(awk '/Malformed/ {print $2; exit}' "${LOG_FILE}")"
  MISSING="$(awk '/Missing/ {print $2; exit}' "${LOG_FILE}")"
  HALLUC="$(awk '/Hallucinated/ {print $2; exit}' "${LOG_FILE}")"

  printf "%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n" \
    "${TIER}" "${MODEL}" "${ACC:-NA}" "${TURNS:-0}" "${CORRECT:-0}" \
    "${MALFORMED:-0}" "${MISSING:-0}" "${HALLUC:-0}" "${STATUS}" \
    >> "${SUMMARY}"

  echo "   -> ${STATUS} acc=${ACC:-NA}% turns=${TURNS:-0}"
done

echo ""
echo "=== SUMMARY (${OUT_DIR}/summary.tsv) ==="
column -t -s $'\t' "${SUMMARY}"
