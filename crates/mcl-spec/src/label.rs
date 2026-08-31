//! Flow-label types, lattice definitions, and degradation rules.
//!
//! Labels track information-flow constraints on values. A `LabelSet` is an
//! ordered collection of labels attached to a value via `LabeledValue`.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Label
// ---------------------------------------------------------------------------

/// A single flow label like `@phi` or `@secret(audience=buyer)`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Label {
    pub name: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub params: BTreeMap<String, LabelParam>,
}

impl std::fmt::Display for Label {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "@{}", self.name)?;
        if !self.params.is_empty() {
            write!(f, "(")?;
            let mut first = true;
            for (k, v) in &self.params {
                if !first {
                    write!(f, ", ")?;
                }
                first = false;
                write!(f, "{k}={v}")?;
            }
            write!(f, ")")?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// LabelParam
// ---------------------------------------------------------------------------

/// Parameter value for a label argument.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum LabelParam {
    Ident(String),
    Str(String),
    Int(i64),
}

impl std::fmt::Display for LabelParam {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LabelParam::Ident(s) => write!(f, "{s}"),
            LabelParam::Str(s) => write!(f, "\"{s}\""),
            LabelParam::Int(n) => write!(f, "{n}"),
        }
    }
}

// ---------------------------------------------------------------------------
// LabelSet
// ---------------------------------------------------------------------------

/// An ordered set of labels. `BTreeSet` ensures determinism.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LabelSet(pub BTreeSet<Label>);

impl LabelSet {
    /// Create an empty label set.
    pub fn empty() -> Self {
        Self(BTreeSet::new())
    }

    /// Returns `true` if the set contains no labels.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns `true` if the set contains a label with the given name.
    pub fn contains_name(&self, name: &str) -> bool {
        self.0.iter().any(|l| l.name == name)
    }

    /// Union of two label sets.
    pub fn join(&self, other: &LabelSet) -> LabelSet {
        let mut merged = self.0.clone();
        merged.extend(other.0.iter().cloned());
        LabelSet(merged)
    }

    /// Insert a label into the set.
    pub fn insert(&mut self, label: Label) {
        self.0.insert(label);
    }
}

impl std::iter::FromIterator<Label> for LabelSet {
    fn from_iter<I: IntoIterator<Item = Label>>(iter: I) -> Self {
        LabelSet(iter.into_iter().collect())
    }
}

// ---------------------------------------------------------------------------
// EgressPolicy
// ---------------------------------------------------------------------------

/// Egress policy for a lattice label.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum EgressPolicy {
    #[default]
    Deny,
    Allow,
    RedactAndWarn,
}

// ---------------------------------------------------------------------------
// LatticeScope
// ---------------------------------------------------------------------------

/// Scope of a lattice label.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LatticeScope {
    Workspace,
    PerAudience(String),
}

impl LatticeScope {
    /// Default scope factory used by serde.
    pub fn default_workspace() -> LatticeScope {
        LatticeScope::Workspace
    }
}

impl Default for LatticeScope {
    fn default() -> Self {
        Self::default_workspace()
    }
}

// ---------------------------------------------------------------------------
// LatticeEntry
// ---------------------------------------------------------------------------

/// Entry in a label lattice.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LatticeEntry {
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dominates: Vec<String>,
    #[serde(default)]
    pub default_egress: EgressPolicy,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<String>,
    #[serde(default = "LatticeScope::default_workspace")]
    pub scope: LatticeScope,
}

// ---------------------------------------------------------------------------
// Lattice
// ---------------------------------------------------------------------------

/// A label lattice declaration.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lattice {
    pub labels: Vec<LatticeEntry>,
}

impl Lattice {
    pub fn is_empty(&self) -> bool {
        self.labels.is_empty()
    }
}

// ---------------------------------------------------------------------------
// DegradationRule
// ---------------------------------------------------------------------------

/// A degradation rule: how to downgrade a labeled value for a different audience.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DegradationRule {
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_params: Option<BTreeMap<String, String>>,
    pub to_level: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub redact: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub preserve: Vec<String>,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn label(name: &str) -> Label {
        Label {
            name: name.to_string(),
            params: BTreeMap::new(),
        }
    }

    fn make_label(name: &str, params: BTreeMap<String, LabelParam>) -> Label {
        Label {
            name: name.to_string(),
            params,
        }
    }

    #[test]
    fn label_creation() {
        let l = label("phi");
        assert_eq!(l.name, "phi");
        assert!(l.params.is_empty());
        assert!(format!("{l:?}").contains("phi"));
        assert_eq!(l.to_string(), "@phi");
    }

    #[test]
    fn label_with_params() {
        let mut params = BTreeMap::new();
        params.insert("audience".into(), LabelParam::Ident("buyer".into()));
        let l = make_label("secret", params);
        assert_eq!(l.name, "secret");
        assert_eq!(l.params.len(), 1);
        assert_eq!(l.params["audience"], LabelParam::Ident("buyer".into()));
        let display = l.to_string();
        assert!(display.starts_with("@secret("));
        assert!(display.contains("audience=buyer"));
    }

    #[test]
    fn label_set_empty() {
        let ls = LabelSet::default();
        assert!(ls.is_empty());
        let ls2 = LabelSet::empty();
        assert!(ls2.is_empty());
    }

    #[test]
    fn label_set_empty_serialization() {
        let ls = LabelSet::empty();
        let json = serde_json::to_string(&ls).unwrap();
        assert_eq!(json, "[]");
        let back: LabelSet = serde_json::from_str(&json).unwrap();
        assert_eq!(ls, back);
        assert!(back.is_empty());
    }

    #[test]
    fn label_set_insert_and_contains() {
        let mut ls = LabelSet::empty();
        ls.insert(label("phi"));
        assert!(!ls.is_empty());
        assert!(ls.contains_name("phi"));
        assert!(!ls.contains_name("secret"));
    }

    #[test]
    fn label_set_duplicate_idempotent() {
        let mut ls = LabelSet::empty();
        ls.insert(label("phi"));
        ls.insert(label("phi"));
        assert_eq!(ls.0.len(), 1);
        assert!(ls.contains_name("phi"));
    }

    #[test]
    fn label_set_join() {
        let mut a = LabelSet::empty();
        a.insert(label("phi"));
        let mut b = LabelSet::empty();
        b.insert(label("secret"));
        let joined = a.join(&b);
        assert!(joined.contains_name("phi"));
        assert!(joined.contains_name("secret"));
    }

    #[test]
    fn parse_label_postfix_basic() {
        use crate::value::{LabeledValue, MclValue};

        let mut ls = LabelSet::empty();
        ls.insert(label("phi"));
        let lv = LabeledValue {
            value: MclValue::String("value".into()),
            labels: ls,
        };
        assert_eq!(lv.value, MclValue::String("value".into()));
        assert!(lv.labels.contains_name("phi"));
        let display = lv.to_string();
        assert!(display.contains("value"));
        assert!(display.contains("@phi"));
    }

    #[test]
    fn labeled_value_serialization() {
        use crate::value::{LabeledValue, MclValue};

        let mut ls = LabelSet::empty();
        ls.insert(label("phi"));
        let lv = LabeledValue {
            value: MclValue::String("hello".into()),
            labels: ls,
        };
        let json = serde_json::to_string(&lv).unwrap();
        let back: LabeledValue = serde_json::from_str(&json).unwrap();
        assert_eq!(lv, back);
        let obj: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(obj["value"], "hello");
        assert!(obj["labels"].is_array());
    }

    #[test]
    fn label_set_serde_roundtrip() {
        let mut ls = LabelSet::empty();
        ls.insert(label("phi"));
        ls.insert(label("secret"));
        let json = serde_json::to_string(&ls).unwrap();
        let back: LabelSet = serde_json::from_str(&json).unwrap();
        assert_eq!(ls, back);
        assert!(back.contains_name("phi"));
        assert!(back.contains_name("secret"));
    }

    #[test]
    fn degradation_rule_serialization() {
        let rule = DegradationRule {
            label: "secret".into(),
            from_params: None,
            to_level: "internal".into(),
            redact: vec!["body".into()],
            preserve: vec!["id".into()],
        };
        let json = serde_json::to_string(&rule).unwrap();
        let back: DegradationRule = serde_json::from_str(&json).unwrap();
        assert_eq!(rule, back);
    }

    #[test]
    fn lattice_entry_serialization() {
        let entry = LatticeEntry {
            name: "secret".into(),
            dominates: vec!["phi".into()],
            default_egress: EgressPolicy::Deny,
            params: vec!["audience".into()],
            scope: LatticeScope::PerAudience("buyer".into()),
        };
        let json = serde_json::to_string(&entry).unwrap();
        let back: LatticeEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(entry, back);
    }
}
