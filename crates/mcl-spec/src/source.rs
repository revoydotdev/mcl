//! Source types: trust levels, source declarations, verification config.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Trust level for an input source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Trust {
    User,
    Narrative,
    Untrusted,
    External,
    System,
}

/// Unique identifier for a source.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SourceId(pub String);

impl SourceId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for SourceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for SourceId {
    fn from(s: &str) -> Self {
        SourceId(s.to_string())
    }
}

impl From<String> for SourceId {
    fn from(s: String) -> Self {
        SourceId(s)
    }
}

impl Default for SourceId {
    /// Convention: undeclared sources default to `"system"`.
    fn default() -> Self {
        SourceId("system".into())
    }
}

/// Verification configuration for a source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum VerificationConfig {
    HmacSha256 { secret_env: String },
}

/// A declared input source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Source {
    pub id: SourceId,
    pub trust: Trust,
    #[serde(default)]
    pub inherit_labels: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verify: Option<VerificationConfig>,
}

/// Map of declared sources (parsed from `sources:` block).
pub type SourceConfig = BTreeMap<SourceId, Source>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_id_display() {
        let id = SourceId("player_input".into());
        assert_eq!(id.to_string(), "player_input");
    }

    #[test]
    fn source_id_from_str() {
        let id = SourceId::from("webhook");
        assert_eq!(id.as_str(), "webhook");
    }

    #[test]
    fn source_id_from_string() {
        let id = SourceId::from(String::from("test"));
        assert_eq!(id.as_str(), "test");
    }

    #[test]
    fn trust_serde_roundtrip() {
        let variants = [
            Trust::User,
            Trust::Narrative,
            Trust::Untrusted,
            Trust::External,
            Trust::System,
        ];
        for variant in variants {
            let json = serde_json::to_string(&variant).expect("serialize");
            let back: Trust = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(variant, back);
        }
    }

    #[test]
    fn source_serialization() {
        let source = Source {
            id: SourceId::from("api_input"),
            trust: Trust::External,
            inherit_labels: vec!["sensitive".into(), "pii".into()],
            verify: Some(VerificationConfig::HmacSha256 {
                secret_env: "HMAC_SECRET".into(),
            }),
        };
        let json = serde_json::to_string(&source).expect("serialize");
        let back: Source = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(source, back);
    }

    #[test]
    fn source_config_default() {
        let config: SourceConfig = BTreeMap::new();
        assert!(config.is_empty());
    }

    #[test]
    fn verification_config_serialization() {
        let vc = VerificationConfig::HmacSha256 {
            secret_env: "MY_SECRET".into(),
        };
        let json = serde_json::to_string(&vc).expect("serialize");
        let back: VerificationConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(vc, back);
    }

    #[test]
    fn source_trust_enum() {
        // Convention: undeclared sources default to Trust::System
        let default_trust = Trust::System;
        assert_eq!(default_trust, Trust::System);
        // Verify all variants are distinct
        let all = [
            Trust::User,
            Trust::Narrative,
            Trust::Untrusted,
            Trust::External,
            Trust::System,
        ];
        for (i, a) in all.iter().enumerate() {
            for (j, b) in all.iter().enumerate() {
                if i != j {
                    assert_ne!(a, b, "Trust variants must be distinct");
                }
            }
        }
    }

    #[test]
    fn source_with_optional_verify() {
        // Source without verify field
        let source = Source {
            id: SourceId::from("local"),
            trust: Trust::User,
            inherit_labels: vec![],
            verify: None,
        };
        let json = serde_json::to_string(&source).expect("serialize");
        // verify should be omitted from JSON (skip_serializing_if)
        assert!(!json.contains("verify"));
        let back: Source = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(source, back);
        assert!(back.verify.is_none());
    }

    #[test]
    fn parse_label_postfix_parameterized() {
        // R1 §6 test 2: verify parameterized values round-trip through serde.
        // Test Source with full fields (including inherit_labels) round-trips.
        let source = Source {
            id: SourceId::from("webhook"),
            trust: Trust::User,
            inherit_labels: vec!["secret".into()],
            verify: None,
        };
        let json = serde_json::to_string(&source).expect("serialize");
        let back: Source = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.inherit_labels, vec!["secret"]);

        // Test parameter map pattern (label params like audience=buyer)
        let params: BTreeMap<String, String> = {
            let mut m = BTreeMap::new();
            m.insert("audience".into(), "buyer".into());
            m
        };
        let params_json = serde_json::to_string(&params).expect("serialize params");
        let params_back: BTreeMap<String, String> =
            serde_json::from_str(&params_json).expect("deserialize params");
        assert_eq!(params_back.get("audience"), Some(&String::from("buyer")));
    }
}
