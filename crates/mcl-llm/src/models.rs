//! Known model compatibility table.
//!
//! This module mirrors the [MODELS.md](../../MODELS.md) compatibility matrix.
//! Use `is_known(provider, model)` to check whether a (provider, model) pair
//! is recognized. Unknown models are not blocked — the function is advisory.

/// Returns `true` if `(provider, model)` is a known, tested (or mock) LLM pair.
///
/// Both `provider` and `model` are matched case-insensitively.
/// This function is advisory only — unknown models are not blocked.
pub fn is_known(provider: &str, model: &str) -> bool {
    let provider = provider.to_lowercase();
    let model = model.to_lowercase();
    KNOWN_MODELS
        .iter()
        .any(|(p, m)| *p == provider && *m == model)
}

/// Returns `true` if `(provider, model)` is an unknown pair.
pub fn is_unknown(provider: &str, model: &str) -> bool {
    !is_known(provider, model)
}

// Static table of known (provider, model) pairs.
// Keep in sync with MODELS.md at repo root.
static KNOWN_MODELS: &[(&str, &str)] = &[
    ("anthropic", "claude-opus-4-7"),
    ("anthropic", "claude-sonnet-4-6"),
    ("anthropic", "claude-haiku-4-5"),
    ("openai", "gpt-5"),
    ("openai", "gpt-4o"),
    ("zai", "glm-5-turbo"),
    ("zai", "glm-5.1"),
    ("zai", "glm-4.7"),
    ("ollama", "llama3.1"),
    ("mock", "mock"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_non_empty() {
        assert!(!KNOWN_MODELS.is_empty());
    }

    #[test]
    fn known_models_return_true() {
        assert!(is_known("anthropic", "claude-opus-4-7"));
        assert!(is_known("anthropic", "claude-sonnet-4-6"));
        assert!(is_known("openai", "gpt-5"));
        assert!(is_known("openai", "gpt-4o"));
        assert!(is_known("zai", "glm-5-turbo"));
        assert!(is_known("zai", "glm-5.1"));
        assert!(is_known("zai", "glm-4.7"));
        assert!(is_known("ollama", "llama3.1"));
        assert!(is_known("mock", "mock"));
    }

    #[test]
    fn case_insensitive_matching() {
        assert!(is_known("Anthropic", "Claude-Opus-4-7"));
        assert!(is_known("OPENAI", "GPT-5"));
    }

    #[test]
    fn unknown_pair_returns_false() {
        assert!(!is_known("acme", "fakemodel"));
        assert!(!is_known("anthropic", "nonexistent-model"));
        assert!(!is_known("", ""));
    }

    #[test]
    fn is_unknown_inverts_is_known() {
        assert!(is_unknown("acme", "fakemodel"));
        assert!(!is_unknown("mock", "mock"));
    }
}
