//! Narrator constraint types: violation actions, prose egress filter config.

use serde::{Deserialize, Serialize};

/// Action to take when narrator filter detects a violation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViolationAction {
    Deny,
    RedactAndWarn,
    Rewrite,
    #[default]
    AuditOnly,
}

/// Constraints for the narrator (prose-channel) egress filter.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct NarratorConstraints {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub must_not_mention: Vec<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub must_not_emit_pattern: Vec<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub must_not_emit_word: Vec<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prose_max_length: Option<usize>,

    #[serde(default)]
    pub on_violation: ViolationAction,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn violation_action_default() {
        assert_eq!(ViolationAction::default(), ViolationAction::AuditOnly);
    }

    #[test]
    fn narrator_constraints_default() {
        let nc = NarratorConstraints {
            must_not_mention: vec![],
            must_not_emit_pattern: vec![],
            must_not_emit_word: vec![],
            prose_max_length: None,
            on_violation: ViolationAction::default(),
        };
        assert!(nc.must_not_mention.is_empty());
        assert!(nc.must_not_emit_pattern.is_empty());
        assert!(nc.must_not_emit_word.is_empty());
        assert!(nc.prose_max_length.is_none());
        assert_eq!(nc.on_violation, ViolationAction::AuditOnly);
    }

    #[test]
    fn narrator_constraints_serialization() {
        let nc = NarratorConstraints {
            must_not_mention: vec!["@phi".into()],
            must_not_emit_pattern: vec![r"\d{3}-\d{2}-\d{4}".into()],
            must_not_emit_word: vec!["forbidden_word".into()],
            prose_max_length: Some(500),
            on_violation: ViolationAction::Deny,
        };
        let json = serde_json::to_string(&nc).expect("serialize");
        let back: NarratorConstraints = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(nc, back);
    }

    #[test]
    fn narrator_constraints_partial() {
        let nc = NarratorConstraints {
            must_not_mention: vec!["@secret".into()],
            must_not_emit_pattern: vec![],
            must_not_emit_word: vec![],
            prose_max_length: Some(1000),
            on_violation: ViolationAction::RedactAndWarn,
        };
        let json = serde_json::to_string(&nc).expect("serialize");
        let back: NarratorConstraints = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.must_not_mention, vec!["@secret"]);
        assert!(back.must_not_emit_pattern.is_empty());
        assert!(back.must_not_emit_word.is_empty());
        assert_eq!(back.prose_max_length, Some(1000));
        assert_eq!(back.on_violation, ViolationAction::RedactAndWarn);
    }

    #[test]
    fn violation_action_serde_all_variants() {
        let cases = [
            (ViolationAction::Deny, "\"deny\""),
            (ViolationAction::RedactAndWarn, "\"redact_and_warn\""),
            (ViolationAction::Rewrite, "\"rewrite\""),
            (ViolationAction::AuditOnly, "\"audit_only\""),
        ];
        for (variant, expected_json) in cases {
            let json = serde_json::to_string(&variant).expect("serialize");
            assert_eq!(json, expected_json);
            let back: ViolationAction = serde_json::from_str(expected_json).expect("deserialize");
            assert_eq!(back, variant);
        }
    }
}
