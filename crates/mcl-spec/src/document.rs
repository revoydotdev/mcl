use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::effects::Effect;
use crate::expression::{Condition, FieldPath};
use crate::label::{DegradationRule, Lattice};
use crate::narrator::NarratorConstraints;
use crate::policy::PolicyDef;
use crate::source::SourceConfig;
use crate::span::Span;
use crate::value::MclValue;

use crate::error::suggest_context_type;

/// A complete parsed MCL document representing one context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MclDocument {
    pub frontmatter: Frontmatter,
    pub state: IndexMap<String, MclValue>,
    pub entities: IndexMap<String, EntityDef>,
    pub items: IndexMap<String, ItemDef>,
    pub actions: IndexMap<String, ActionDef>,
    pub transitions: Vec<TransitionDef>,
    pub narrator: NarratorDef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub narrator_constraints: Option<NarratorConstraints>,
    pub knowledge: Vec<KnowledgeEntry>,
    pub constraints: Vec<String>,
    #[serde(default, skip_serializing_if = "SourceConfig::is_empty")]
    pub sources: SourceConfig,
    #[serde(default, skip_serializing_if = "Lattice::is_empty")]
    pub labels: Lattice,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub degrade_rules: Vec<DegradationRule>,
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub policies: IndexMap<String, PolicyDef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub effects: Vec<Effect>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Frontmatter {
    pub context: String,
    pub context_type: ContextType,
    pub domain: Option<String>,
    pub version: Option<u32>,
    pub tags: Vec<String>,
    pub extends: Option<String>,
    pub extra: IndexMap<String, MclValue>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextType {
    Room,
    Scene,
    Step,
    Dialogue,
    Encounter,
    LabelLattice,
}

impl std::str::FromStr for ContextType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "room" => Ok(Self::Room),
            "scene" => Ok(Self::Scene),
            "step" => Ok(Self::Step),
            "dialogue" => Ok(Self::Dialogue),
            "encounter" => Ok(Self::Encounter),
            "label_lattice" => Ok(Self::LabelLattice),
            _ => {
                let base = format!(
                    "unknown context type '{s}', expected one of: room, scene, step, dialogue, encounter, label_lattice"
                );
                let msg = match suggest_context_type(s) {
                    Some(hint) => format!("{base} (did you mean '{hint}'?)"),
                    None => base,
                };
                Err(msg)
            }
        }
    }
}

impl std::fmt::Display for ContextType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ContextType::Room => write!(f, "room"),
            ContextType::Scene => write!(f, "scene"),
            ContextType::Step => write!(f, "step"),
            ContextType::Dialogue => write!(f, "dialogue"),
            ContextType::Encounter => write!(f, "encounter"),
            ContextType::LabelLattice => write!(f, "label_lattice"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityDef {
    pub name: String,
    pub entity_type: Option<String>,
    pub disposition: Option<String>,
    pub knowledge: Vec<String>,
    pub properties: IndexMap<String, MclValue>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemDef {
    pub name: String,
    pub properties: IndexMap<String, MclValue>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionDef {
    pub name: String,
    pub requires: Vec<Condition>,
    pub requires_any: Vec<Condition>,
    pub target: Option<TargetSpec>,
    pub mutates: IndexMap<String, MclValue>,
    pub resolution: Option<String>,
    pub mode: Option<String>,
    pub args: IndexMap<String, String>,
    pub cooldown: Option<u32>,
    pub description: Option<String>,
    /// Hint phrases that help the LLM decide when to use this action.
    #[serde(default)]
    pub trigger_phrases: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub effects: Vec<Effect>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TargetSpec {
    /// enum(field.path) — valid targets from state
    Enum(FieldPath),
    /// Fixed single target
    Fixed(String),
}

impl std::fmt::Display for TargetSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TargetSpec::Enum(path) => write!(f, "enum({path})"),
            TargetSpec::Fixed(name) => write!(f, "{name}"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransitionDef {
    pub target: String,
    pub when: Option<Condition>,
    pub direction: Option<String>,
    pub auto: bool,
    pub narrate: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NarratorDef {
    pub tone: Option<String>,
    pub voice: Option<String>,
    pub tense: Option<String>,
    pub context: Option<String>,
    pub style_notes: Option<String>,
    pub forbidden: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum KnowledgeEntry {
    /// LLM knows this fact
    Known(String),
    /// LLM explicitly does NOT know this
    Unknown(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn context_type_from_str_valid() {
        assert_eq!(ContextType::from_str("room").unwrap(), ContextType::Room);
        assert_eq!(ContextType::from_str("scene").unwrap(), ContextType::Scene);
        assert_eq!(ContextType::from_str("step").unwrap(), ContextType::Step);
        assert_eq!(
            ContextType::from_str("dialogue").unwrap(),
            ContextType::Dialogue
        );
        assert_eq!(
            ContextType::from_str("encounter").unwrap(),
            ContextType::Encounter
        );
    }

    #[test]
    fn context_type_from_str_invalid() {
        assert!(ContextType::from_str("invalid").is_err());
        assert!(ContextType::from_str("").is_err());
        assert!(ContextType::from_str("ROOM").is_err());
    }

    #[test]
    fn context_type_display() {
        assert_eq!(ContextType::Room.to_string(), "room");
        assert_eq!(ContextType::Scene.to_string(), "scene");
        assert_eq!(ContextType::Encounter.to_string(), "encounter");
    }

    #[test]
    fn target_spec_display() {
        let path_enum =
            TargetSpec::Enum(crate::expression::FieldPath::new(vec!["entities".into()]));
        assert_eq!(path_enum.to_string(), "enum(entities)");
        let fixed = TargetSpec::Fixed("goblin".into());
        assert_eq!(fixed.to_string(), "goblin");
    }

    #[test]
    fn knowledge_entry_serde() {
        let known = KnowledgeEntry::Known("guards_tomb".into());
        let json = serde_json::to_string(&known).unwrap();
        let back: KnowledgeEntry = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, KnowledgeEntry::Known(_)));

        let unknown = KnowledgeEntry::Unknown("secret_passage".into());
        let json = serde_json::to_string(&unknown).unwrap();
        let back: KnowledgeEntry = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, KnowledgeEntry::Unknown(_)));
    }

    #[test]
    fn narrator_def_default() {
        let n = NarratorDef::default();
        assert!(n.tone.is_none());
        assert!(n.voice.is_none());
        assert!(n.tense.is_none());
        assert!(n.forbidden.is_empty());
    }

    #[test]
    fn document_empty_policies_effects() {
        let doc = MclDocument {
            frontmatter: Frontmatter {
                context: "test".into(),
                context_type: ContextType::Room,
                domain: None,
                version: None,
                tags: Vec::new(),
                extends: None,
                extra: IndexMap::new(),
                span: Span::empty(),
            },
            state: IndexMap::new(),
            entities: IndexMap::new(),
            items: IndexMap::new(),
            actions: IndexMap::new(),
            transitions: Vec::new(),
            narrator: NarratorDef::default(),
            narrator_constraints: None,
            knowledge: Vec::new(),
            constraints: Vec::new(),
            sources: SourceConfig::default(),
            labels: Lattice::default(),
            degrade_rules: Vec::new(),
            policies: IndexMap::new(),
            effects: Vec::new(),
            span: Span::empty(),
        };

        // Empty policies and effects should not appear in JSON (backward compatibility)
        let json = serde_json::to_string(&doc).unwrap();
        assert!(!json.contains("policies"));
        assert!(!json.contains("effects"));

        // Roundtrip should preserve empty fields
        let back: MclDocument = serde_json::from_str(&json).unwrap();
        assert!(back.policies.is_empty());
        assert!(back.effects.is_empty());
    }

    #[test]
    fn document_with_policy_serializes() {
        let mut doc = MclDocument {
            frontmatter: Frontmatter {
                context: "test".into(),
                context_type: ContextType::Room,
                domain: None,
                version: None,
                tags: Vec::new(),
                extends: None,
                extra: IndexMap::new(),
                span: Span::empty(),
            },
            state: IndexMap::new(),
            entities: IndexMap::new(),
            items: IndexMap::new(),
            actions: IndexMap::new(),
            transitions: Vec::new(),
            narrator: NarratorDef::default(),
            narrator_constraints: None,
            knowledge: Vec::new(),
            constraints: Vec::new(),
            sources: SourceConfig::default(),
            labels: Lattice::default(),
            degrade_rules: Vec::new(),
            policies: IndexMap::new(),
            effects: Vec::new(),
            span: Span::empty(),
        };

        doc.policies.insert(
            "test_policy".into(),
            PolicyDef {
                name: "test_policy".into(),
                on: crate::policy::Event::StateMutate,
                match_expr: None,
                require_exprs: Vec::new(),
                transform: None,
                on_violation: crate::policy::PolicyViolationAction::LogOnly,
                priority: None,
            },
        );

        // Policy should appear in JSON
        let json = serde_json::to_string(&doc).unwrap();
        assert!(json.contains("policies"));
        assert!(json.contains("test_policy"));

        // Roundtrip should preserve the policy
        let back: MclDocument = serde_json::from_str(&json).unwrap();
        assert_eq!(back.policies.len(), 1);
        assert!(back.policies.contains_key("test_policy"));
    }

    #[test]
    fn action_with_effects_serializes() {
        let action = ActionDef {
            name: "test_action".into(),
            requires: Vec::new(),
            requires_any: Vec::new(),
            target: None,
            mutates: IndexMap::new(),
            resolution: None,
            mode: None,
            args: IndexMap::new(),
            cooldown: None,
            description: None,
            trigger_phrases: Vec::new(),
            effects: Vec::new(),
            span: Span::empty(),
        };

        // Empty effects should not appear in JSON
        let json = serde_json::to_string(&action).unwrap();
        assert!(!json.contains("effects"));

        // Roundtrip with empty effects
        let back: ActionDef = serde_json::from_str(&json).unwrap();
        assert!(back.effects.is_empty());

        let mut action_with_effect = action.clone();
        action_with_effect.effects.push(Effect::StateMutating);

        // Effect should appear in JSON
        let json = serde_json::to_string(&action_with_effect).unwrap();
        assert!(json.contains("effects"));

        // Roundtrip should preserve the effect
        let back: ActionDef = serde_json::from_str(&json).unwrap();
        assert_eq!(back.effects.len(), 1);
    }
}
