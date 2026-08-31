# Known LLM Models

Compatibility matrix for LLM providers used with MCL. Updated when new models
are validated against the signal grammar.

## Canonical key form

`is_known()` in `crates/mcl-llm/src/models.rs` matches `(provider, model)` as a
lowercase tuple. Both arguments are case-insensitive. Unknown models are not
blocked — the check is advisory.

## Matrix

| Provider | Model | Signal Reliability | Context Window | Tested-on Commit | Notes |
|---|---|---|---|---|---|
| anthropic | claude-opus-4-7 | untested | 200K | 1709e47 | |
| anthropic | claude-sonnet-4-6 | untested | 200K | 1709e47 | |
| anthropic | claude-haiku-4-5 | untested | 200K | 1709e47 | |
| openai | gpt-5 | untested | 128K | 1709e47 | |
| openai | gpt-4o | untested | 128K | 1709e47 | |
| zai | glm-5-turbo | untested | 128K | 1709e47 | |
| zai | glm-5.1 | untested | 128K | 1709e47 | |
| zai | glm-4.7 | untested | 128K | 1709e47 | |
| ollama | llama3.1 | untested | 128K | 1709e47 | Common local model |
| mock | mock | 100% | n/a | 1709e47 | Deterministic test backend |
