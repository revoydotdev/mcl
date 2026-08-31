//! Narrator egress filter: 4-pass pipeline (pattern → label → word → length → audit).

use std::time::{SystemTime, UNIX_EPOCH};

use mcl_llm::backend::LlmBackend;
use mcl_spec::label::Label;
use mcl_spec::narrator::{NarratorConstraints, ViolationAction};
use regex::RegexSet;

#[derive(Debug)]
pub struct NarratorAuditEntry {
    pub timestamp: String,
    pub action: ViolationAction,
    pub trigger: String,
    pub original_prose: String,
    pub result_prose: String,
    pub retry_count: u8,
}

#[derive(Debug)]
pub struct NarratorFilterResult {
    pub prose: String,
    pub audit: Vec<NarratorAuditEntry>,
}

#[derive(Debug)]
pub struct NarratorDenyError {
    pub trigger: String,
    pub original_prose: String,
}

impl std::fmt::Display for NarratorDenyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "narrator denied: '{}' in prose", self.trigger)
    }
}

impl std::error::Error for NarratorDenyError {}

fn iso_now() -> String {
    let dur = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(std::time::Duration::ZERO);
    let secs = dur.as_secs();
    let nanos = dur.subsec_nanos();

    let year = 1970 + (secs / 31_536_000);
    let secs = secs % 31_536_000;
    let days_from_jan = secs / 86_400;
    let time_of_day = secs % 86_400;

    let leap_years = (year - 1969) / 4 - (year - 1901) / 100 + (year - 1601) / 400;
    let day_of_year = days_from_jan - leap_years;

    let month_days = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut month = 0;
    let mut day = day_of_year;
    for (i, &md) in month_days.iter().enumerate() {
        if day < md {
            month = i + 1;
            break;
        }
        day -= md;
    }
    if month == 0 {
        month = 12;
    }

    let hour = time_of_day / 3600;
    let minute = (time_of_day % 3600) / 60;
    let second = time_of_day % 60;

    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{nanos:09}Z")
}

fn make_entry(
    action: ViolationAction,
    trigger: &str,
    original_prose: &str,
    result_prose: &str,
    retry_count: u8,
) -> NarratorAuditEntry {
    NarratorAuditEntry {
        timestamp: iso_now(),
        action,
        trigger: trigger.to_string(),
        original_prose: original_prose.to_string(),
        result_prose: result_prose.to_string(),
        retry_count,
    }
}

/// Returns the set of indices into `patterns` that match `prose`, or None if
/// the set is empty / not compiled.
fn pattern_match_indices(prose: &str, set: &Option<RegexSet>) -> Vec<usize> {
    let Some(s) = set else {
        return Vec::new();
    };
    s.matches(prose).into_iter().collect()
}

/// For each matching pattern, replace the first match of that regex in prose
/// with `[REDACTED]`. Falls back to a simple scan if individual regex compilation
/// fails for any pattern.
fn redact_pattern_matches(prose: &str, patterns: &[String], match_indices: &[usize]) -> String {
    let mut result = prose.to_string();
    for &idx in match_indices {
        let pat = &patterns[idx];
        if let Ok(re) = regex::Regex::new(pat) {
            result = re.replace(&result, "[REDACTED]").to_string();
        }
    }
    result
}

fn apply_redaction(prose: &str, triggers: &[String]) -> String {
    let mut result = prose.to_string();
    for trigger in triggers {
        result = result.replace(trigger.as_str(), "[REDACTED]");
    }
    result
}

fn has_violations(
    prose: &str,
    pattern_set: &Option<RegexSet>,
    constraints: &NarratorConstraints,
    label_lookup: &[(String, Label)],
) -> bool {
    if let Some(set) = pattern_set {
        if set.matches(prose).into_iter().next().is_some() {
            return true;
        }
    }
    if label_lookup.iter().any(|(v, _)| prose.contains(v.as_str())) {
        return true;
    }
    if constraints
        .must_not_emit_word
        .iter()
        .any(|w| prose.contains(w.as_str()))
    {
        return true;
    }
    if let Some(max) = constraints.prose_max_length {
        if prose.chars().count() > max {
            return true;
        }
    }
    false
}

/// Run all four passes (pattern → label → word → length) and return the
/// post-violation prose plus a list of trigger strings. Handles each pass
/// according to `on_violation`. Returns `Err(NarratorDenyError)` on deny.
fn run_passes(
    prose: &str,
    constraints: &NarratorConstraints,
    label_lookup: &[(String, Label)],
    pattern_set: &Option<RegexSet>,
) -> Result<(String, Vec<String>), NarratorDenyError> {
    let original = prose.to_string();
    let mut current = prose.to_string();
    let mut triggers = Vec::new();

    let pat_indices = pattern_match_indices(&current, pattern_set);
    if !pat_indices.is_empty() {
        let trigger_str: Vec<String> = pat_indices
            .iter()
            .map(|&i| constraints.must_not_emit_pattern[i].clone())
            .collect();
        match constraints.on_violation {
            ViolationAction::Deny => {
                return Err(NarratorDenyError {
                    trigger: trigger_str.join(", "),
                    original_prose: original,
                });
            }
            ViolationAction::RedactAndWarn => {
                current = redact_pattern_matches(
                    &current,
                    &constraints.must_not_emit_pattern,
                    &pat_indices,
                );
                triggers.extend(trigger_str);
            }
            ViolationAction::AuditOnly => {
                triggers.extend(trigger_str);
            }
            ViolationAction::Rewrite => {
                triggers.extend(trigger_str);
            }
        }
    }

    let label_hits: Vec<String> = label_lookup
        .iter()
        .filter(|(v, _)| current.contains(v.as_str()))
        .map(|(v, _)| v.clone())
        .collect();
    if !label_hits.is_empty() {
        let trigger_str: Vec<String> = label_hits.iter().map(|v| format!("label:{v}")).collect();
        match constraints.on_violation {
            ViolationAction::Deny => {
                return Err(NarratorDenyError {
                    trigger: label_hits.join(", "),
                    original_prose: original,
                });
            }
            ViolationAction::RedactAndWarn => {
                current = apply_redaction(&current, &label_hits);
                triggers.extend(trigger_str);
            }
            ViolationAction::AuditOnly => {
                triggers.extend(trigger_str);
            }
            ViolationAction::Rewrite => {
                triggers.extend(trigger_str);
            }
        }
    }

    let word_hits: Vec<String> = constraints
        .must_not_emit_word
        .iter()
        .filter(|w| current.contains(w.as_str()))
        .cloned()
        .collect();
    if !word_hits.is_empty() {
        match constraints.on_violation {
            ViolationAction::Deny => {
                return Err(NarratorDenyError {
                    trigger: word_hits.join(", "),
                    original_prose: original,
                });
            }
            ViolationAction::RedactAndWarn => {
                current = apply_redaction(&current, &word_hits);
                triggers.extend(word_hits);
            }
            ViolationAction::AuditOnly => {
                triggers.extend(word_hits);
            }
            ViolationAction::Rewrite => {
                triggers.extend(word_hits);
            }
        }
    }

    if let Some(max) = constraints.prose_max_length {
        let char_count = current.chars().count();
        if char_count > max {
            current = current.chars().take(max).collect();
            triggers.push(format!("length:{char_count}>{max}"));
        }
    }

    if current.trim().is_empty() {
        current = "[REDACTED]".to_string();
    }

    Ok((current, triggers))
}

pub async fn narrator_filter(
    prose: &str,
    constraints: &NarratorConstraints,
    label_lookup: &[(String, Label)],
    llm: Option<&dyn LlmBackend>,
) -> Result<NarratorFilterResult, NarratorDenyError> {
    if constraints.must_not_emit_pattern.is_empty()
        && constraints.must_not_emit_word.is_empty()
        && constraints.must_not_mention.is_empty()
        && constraints.prose_max_length.is_none()
    {
        return Ok(NarratorFilterResult {
            prose: prose.to_string(),
            audit: Vec::new(),
        });
    }

    let original_prose = prose.to_string();
    let pattern_set = RegexSet::new(&constraints.must_not_emit_pattern).ok();

    let (mut current, triggers) = run_passes(prose, constraints, label_lookup, &pattern_set)?;

    let mut audit = Vec::new();

    if triggers.is_empty() {
        return Ok(NarratorFilterResult {
            prose: current,
            audit,
        });
    }

    if constraints.on_violation == ViolationAction::Rewrite {
        if let Some(backend) = llm {
            let mut forbidden = Vec::new();
            forbidden.extend(constraints.must_not_emit_word.iter().cloned());
            for (value, _) in label_lookup {
                forbidden.push(value.clone());
            }

            let system = if forbidden.is_empty() {
                "Rewrite the prose to remove any sensitive or forbidden content.".to_string()
            } else {
                format!(
                    "Rewrite the prose. Do NOT include: {}.",
                    forbidden.join(", ")
                )
            };
            let user_msg = format!("Rewrite, removing forbidden content:\n\n{current}");

            if let Ok(rewritten) = backend.complete(&system, &[], &user_msg).await {
                if !has_violations(&rewritten, &pattern_set, constraints, label_lookup) {
                    audit.push(make_entry(
                        ViolationAction::Rewrite,
                        &triggers.join(", "),
                        &original_prose,
                        &rewritten,
                        1,
                    ));
                    return Ok(NarratorFilterResult {
                        prose: rewritten,
                        audit,
                    });
                }
                current = rewritten;
            }

            let mut redacted = current.clone();
            for (value, _) in label_lookup {
                if redacted.contains(value.as_str()) {
                    redacted = redacted.replace(value.as_str(), "[REDACTED]");
                }
            }
            for w in &constraints.must_not_emit_word {
                if redacted.contains(w.as_str()) {
                    redacted = redacted.replace(w.as_str(), "[REDACTED]");
                }
            }
            if redacted.trim().is_empty() {
                redacted = "[REDACTED]".to_string();
            }
            audit.push(make_entry(
                ViolationAction::RedactAndWarn,
                &format!("rewrite_failed: {}", triggers.join(", ")),
                &original_prose,
                &redacted,
                1,
            ));
            return Ok(NarratorFilterResult {
                prose: redacted,
                audit,
            });
        } else {
            let mut redacted = current.clone();
            for (value, _) in label_lookup {
                if redacted.contains(value.as_str()) {
                    redacted = redacted.replace(value.as_str(), "[REDACTED]");
                }
            }
            for w in &constraints.must_not_emit_word {
                if redacted.contains(w.as_str()) {
                    redacted = redacted.replace(w.as_str(), "[REDACTED]");
                }
            }
            if redacted.trim().is_empty() {
                redacted = "[REDACTED]".to_string();
            }
            audit.push(make_entry(
                ViolationAction::RedactAndWarn,
                &format!("no_llm: {}", triggers.join(", ")),
                &original_prose,
                &redacted,
                0,
            ));
            return Ok(NarratorFilterResult {
                prose: redacted,
                audit,
            });
        }
    }

    audit.push(make_entry(
        constraints.on_violation,
        &triggers.join(", "),
        &original_prose,
        &current,
        0,
    ));

    Ok(NarratorFilterResult {
        prose: current,
        audit,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use mcl_llm::mock::MockLlmBackend;
    use mcl_spec::label::Label;
    use std::collections::BTreeMap;

    fn phi_label() -> Label {
        Label {
            name: "phi".to_string(),
            params: BTreeMap::new(),
        }
    }

    #[tokio::test]
    async fn narrator_filter_substring_match() {
        let label_lookup = vec![("Jane Doe".to_string(), phi_label())];
        let constraints = NarratorConstraints {
            must_not_mention: vec!["@phi".to_string()],
            must_not_emit_pattern: vec![],
            must_not_emit_word: vec![],
            prose_max_length: None,
            on_violation: ViolationAction::RedactAndWarn,
        };

        let result = narrator_filter(
            "The patient Jane Doe arrived at noon.",
            &constraints,
            &label_lookup,
            None,
        )
        .await
        .unwrap();

        assert_eq!(result.prose, "The patient [REDACTED] arrived at noon.");
        assert_eq!(result.audit.len(), 1);
        assert!(result.audit[0].trigger.contains("Jane Doe"));
        assert_eq!(result.audit[0].action, ViolationAction::RedactAndWarn);
    }

    #[tokio::test]
    async fn narrator_filter_pattern_match() {
        let constraints = NarratorConstraints {
            must_not_mention: vec![],
            must_not_emit_pattern: vec![r"\d{3}-\d{2}-\d{4}".to_string()],
            must_not_emit_word: vec![],
            prose_max_length: None,
            on_violation: ViolationAction::RedactAndWarn,
        };

        let result = narrator_filter(
            "Her SSN is 123-45-6789, keep it safe.",
            &constraints,
            &[],
            None,
        )
        .await
        .unwrap();

        assert_eq!(result.prose, "Her SSN is [REDACTED], keep it safe.");
        assert!(!result.audit.is_empty());
        assert_eq!(result.audit[0].action, ViolationAction::RedactAndWarn);
    }

    #[tokio::test]
    async fn narrator_rewrite_one_retry() {
        let label_lookup = vec![("SECRET_TOKEN".to_string(), phi_label())];
        let constraints = NarratorConstraints {
            must_not_mention: vec!["@phi".to_string()],
            must_not_emit_pattern: vec![],
            must_not_emit_word: vec![],
            prose_max_length: None,
            on_violation: ViolationAction::Rewrite,
        };

        let mock = MockLlmBackend::new(vec![
            "The code is classified and very important.".to_string()
        ]);

        let result = narrator_filter(
            "The code is SECRET_TOKEN and very important.",
            &constraints,
            &label_lookup,
            Some(&mock),
        )
        .await
        .unwrap();

        assert!(
            !result.prose.contains("SECRET_TOKEN"),
            "final prose should not contain the secret: {}",
            result.prose
        );
        assert!(!result.audit.is_empty());
        assert!(result.audit.iter().any(|e| e.retry_count > 0));
    }

    #[tokio::test]
    async fn narrator_filter_empty_constraints() {
        let constraints = NarratorConstraints::default();
        let label_lookup = vec![("Jane Doe".to_string(), phi_label())];

        let result = narrator_filter(
            "Jane Doe walked into the room.",
            &constraints,
            &label_lookup,
            None,
        )
        .await
        .unwrap();

        assert_eq!(result.prose, "Jane Doe walked into the room.");
        assert!(result.audit.is_empty());
    }

    #[tokio::test]
    async fn narrator_filter_label_with_metacharacters() {
        let label_lookup = vec![("Jane (Doe)".to_string(), phi_label())];
        let constraints = NarratorConstraints {
            must_not_mention: vec!["@phi".to_string()],
            must_not_emit_pattern: vec![],
            must_not_emit_word: vec![],
            prose_max_length: None,
            on_violation: ViolationAction::RedactAndWarn,
        };

        let result = narrator_filter(
            "Jane (Doe) is the patient.",
            &constraints,
            &label_lookup,
            None,
        )
        .await
        .unwrap();

        assert_eq!(result.prose, "[REDACTED] is the patient.");
        assert_eq!(result.audit.len(), 1);
        assert!(result.audit[0].trigger.contains("Jane (Doe)"));
    }

    #[tokio::test]
    async fn narrator_filter_unicode_length() {
        let constraints = NarratorConstraints {
            must_not_mention: vec![],
            must_not_emit_pattern: vec![],
            must_not_emit_word: vec![],
            prose_max_length: Some(5),
            on_violation: ViolationAction::RedactAndWarn,
        };

        let prose = "日本語テストです";
        let result = narrator_filter(prose, &constraints, &[], None)
            .await
            .unwrap();

        assert_eq!(
            result.prose.chars().count(),
            5,
            "should truncate to 5 chars"
        );
        assert_eq!(result.prose, "日本語テス");
    }

    #[tokio::test]
    async fn narrator_filter_empty_after_redaction() {
        let label_lookup = vec![("everything".to_string(), phi_label())];
        let constraints = NarratorConstraints {
            must_not_mention: vec!["@phi".to_string()],
            must_not_emit_pattern: vec![],
            must_not_emit_word: vec![],
            prose_max_length: None,
            on_violation: ViolationAction::RedactAndWarn,
        };

        let result = narrator_filter("everything", &constraints, &label_lookup, None)
            .await
            .unwrap();

        assert_eq!(result.prose, "[REDACTED]");
    }

    #[tokio::test]
    async fn narrator_filter_deny_returns_error() {
        let constraints = NarratorConstraints {
            must_not_mention: vec![],
            must_not_emit_pattern: vec![],
            must_not_emit_word: vec!["forbidden".to_string()],
            prose_max_length: None,
            on_violation: ViolationAction::Deny,
        };

        let result =
            narrator_filter("This contains forbidden content.", &constraints, &[], None).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.trigger.contains("forbidden"));
    }

    #[tokio::test]
    async fn narrator_filter_audit_only_passes_through() {
        let label_lookup = vec![("Jane Doe".to_string(), phi_label())];
        let constraints = NarratorConstraints {
            must_not_mention: vec!["@phi".to_string()],
            must_not_emit_pattern: vec![],
            must_not_emit_word: vec![],
            prose_max_length: None,
            on_violation: ViolationAction::AuditOnly,
        };

        let result = narrator_filter("Jane Doe is here.", &constraints, &label_lookup, None)
            .await
            .unwrap();

        assert_eq!(result.prose, "Jane Doe is here.");
        assert_eq!(result.audit.len(), 1);
        assert_eq!(result.audit[0].action, ViolationAction::AuditOnly);
    }

    #[tokio::test]
    async fn narrator_filter_word_pass() {
        let constraints = NarratorConstraints {
            must_not_mention: vec![],
            must_not_emit_pattern: vec![],
            must_not_emit_word: vec!["classified".to_string()],
            prose_max_length: None,
            on_violation: ViolationAction::RedactAndWarn,
        };

        let result = narrator_filter("This document is classified.", &constraints, &[], None)
            .await
            .unwrap();

        assert_eq!(result.prose, "This document is [REDACTED].");
        assert_eq!(result.audit.len(), 1);
    }

    #[tokio::test]
    async fn narrator_filter_length_pass() {
        let constraints = NarratorConstraints {
            must_not_mention: vec![],
            must_not_emit_pattern: vec![],
            must_not_emit_word: vec![],
            prose_max_length: Some(5),
            on_violation: ViolationAction::RedactAndWarn,
        };

        let result = narrator_filter("Hello world", &constraints, &[], None)
            .await
            .unwrap();

        assert_eq!(result.prose, "Hello");
        assert_eq!(result.audit.len(), 1);
        assert!(result.audit[0].trigger.contains("length"));
    }

    #[tokio::test]
    async fn narrator_rewrite_no_llm_falls_through() {
        let label_lookup = vec![("secret".to_string(), phi_label())];
        let constraints = NarratorConstraints {
            must_not_mention: vec!["@phi".to_string()],
            must_not_emit_pattern: vec![],
            must_not_emit_word: vec![],
            prose_max_length: None,
            on_violation: ViolationAction::Rewrite,
        };

        let result = narrator_filter("This is secret data.", &constraints, &label_lookup, None)
            .await
            .unwrap();

        assert_eq!(result.prose, "This is [REDACTED] data.");
        assert!(result.audit.iter().any(|e| e.trigger.contains("no_llm")));
    }

    #[tokio::test]
    async fn narrator_audit_entry_timestamp() {
        let constraints = NarratorConstraints {
            must_not_mention: vec![],
            must_not_emit_pattern: vec![],
            must_not_emit_word: vec!["bad".to_string()],
            prose_max_length: None,
            on_violation: ViolationAction::RedactAndWarn,
        };

        let result = narrator_filter("bad word", &constraints, &[], None)
            .await
            .unwrap();

        assert_eq!(result.audit.len(), 1);
        assert!(!result.audit[0].timestamp.is_empty());
        assert!(result.audit[0].timestamp.contains('T'));
    }
}
