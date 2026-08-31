# Signal Reliability Validation — 2026-04-22

First real-LLM sweep answering the Milestone 5 / Stage 2 M2.10 existential
question: *will LLMs reliably emit `MCL::*` signals inline with narration?*

## Method

- 9 models drawn across the parameter spectrum (3B → 671B MoE), 4 provider
  families (Meta, Mistral, Google, OpenAI, Anthropic, DeepSeek), routed via
  OpenRouter's OpenAI-compatible endpoint.
- Context: `examples/crypt/crypt_antechamber.mcl`.
- 20 benchmark scenarios from `mcl_signal::bench::DEFAULT_SCENARIOS` × 3 runs
  each → 60 turns per model, 540 turns total.
- No structured-output mode, no retries, no MCL-specific fine-tuning — raw
  system prompt from `mcl_runtime::prompt::build_system_prompt`.
- Script: `scripts/signal-validation.sh`.

## Results

| Rank | Model                                     | Tier            | Accuracy | Missing | Hallucinated | Malformed |
|-----:|-------------------------------------------|-----------------|---------:|--------:|-------------:|----------:|
|    1 | deepseek/deepseek-chat-v3-0324            | reasoning-moe   | **98.3%**|       1 |            0 |         0 |
|    2 | meta-llama/llama-3.1-8b-instruct          | small (8B)      |    91.7% |       4 |            1 |         0 |
|    2 | mistralai/mistral-small-3.2-24b-instruct  | medium (24B)    |    91.7% |       5 |            0 |         0 |
|    4 | meta-llama/llama-3.3-70b-instruct         | large (70B)     |    83.3% |      10 |            0 |         0 |
|    5 | meta-llama/llama-3.2-3b-instruct          | tiny (3B)       |    78.3% |      13 |            0 |         0 |
|    6 | openai/gpt-4o-mini                        | flagship-cheap  |    75.0% |      14 |            1 |         0 |
|    7 | anthropic/claude-3.5-haiku                | flagship-prem   |    68.3% |      19 |            0 |         0 |
|    8 | mistralai/mistral-nemo                    | small (12B)     |    60.0% |      24 |            0 |         0 |
|    9 | google/gemma-3-4b-it                      | tiny (4B)       |    41.7% |      35 |            0 |         0 |

## Findings

1. **Zero malformed signals across 540 turns.** When a model emits, it emits
   correctly. The signal grammar is not a source of unreliability.

2. **Near-zero hallucinations (2/540 = 0.37%).** The arbiter's
   `HallucinationArbiter` will be quiet-but-ready, not fire-often — suggesting
   the action-whitelist enforcement path is architecturally correct rather
   than regularly needed.

3. **The 95% gate is achievable.** DeepSeek v3 cleared it on the first sweep,
   with no MCL-specific tuning. The existential "can LLMs do this at all"
   question is answered: **yes.**

4. **Parameter size is NOT the primary driver.** Claude 3.5 Haiku
   (flagship) underperformed 8B Llama by 23 points. GPT-4o-mini
   underperformed 3B Llama by 3 points. Llama-3.3-70B underperformed
   Llama-3.1-8B by 8 points. Same-family 23× size gaps produce flat
   results. The real driver is **instruction-following literalism** —
   models RLHF'd for "helpfulness" deviate from strict format more than
   models that take the system prompt at face value.

5. **The single dominant failure mode is under-emission** (missing signal),
   not corruption or invention. The model narrates correctly but forgets
   the `MCL::*` line. This is a prompt-engineering and retry-loop problem,
   not an architectural one.

## Implications for Stage 2 M2.10

- **M2.10 acceptance gate (≥95% Accept rate) is PASSABLE** with model
  selection alone (DeepSeek v3) and almost certainly passable for most
  models with structured-output mode and/or retry-on-missing-signal enabled.
- The Hallucination/Dedup/Schema arbiters are architecturally justified
  but rarely triggered at baseline. Their telemetry value is
  **proof-of-correctness** (showing the arbitration runs without noise)
  as much as it is failure detection.

## Recommended follow-ups

1. **Re-run with structured output mode** (`build_structured_system_prompt`
   + OpenAI JSON schema / Anthropic tool-use). Hypothesis: GPT-4o-mini
   and Claude Haiku jump ≥20 points into the 95% band.
2. **Verify retry-on-missing-signal is wired into the live LLM node** and
   re-measure the bottom 4 models. Missing signals are recoverable; a
   single retry at cost ~1 extra turn should nearly close the gap.
3. **Diff the per-model missing-signal transcripts** (`*.log`) to locate
   which action types cluster the failures; strengthen those few-shot
   examples in `mcl-runtime/src/prompt.rs`.

## Raw transcripts

One `.log` file per model holds the full summary report. The transcripts are
versioned as ground truth for reproducibility and diffing.

## Reproducibility

```bash
export OPENROUTER_API_KEY=<your-key>
bash scripts/signal-validation.sh
```

Override `RUNS=5 COUNT=20` for tighter variance or `COUNT=10` for a faster
smoke. Total cost of this run: roughly $0.30 on OpenRouter metering.
