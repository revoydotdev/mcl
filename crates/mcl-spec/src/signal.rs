use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::label::LabelSet;
use crate::span::Span;
use crate::value::MclValue;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MclSignal {
    pub kind: SignalKind,
    pub span: Span,
    #[serde(default, skip_serializing_if = "LabelSet::is_empty")]
    pub labels: LabelSet,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SignalKind {
    /// MCL::action(name, key=val, ...)
    Action {
        name: String,
        args: IndexMap<String, MclValue>,
    },
    /// MCL::mutate(field.path, value)
    Mutate { path: String, value: MclValue },
    /// MCL::transition(context_id)
    Transition { context: String },
    /// MCL::dialogue(entity_id, "text")
    Dialogue { entity: String, text: String },
    /// MCL::narrate(tag)
    Narrate { tag: String },
    /// MCL::query(field.path)
    Query { path: String },
    /// MCL::error("description")
    Error { message: String },
    /// MCL::noop()
    Noop,
}

impl std::fmt::Display for MclSignal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.kind {
            SignalKind::Action { name, args } => {
                write!(f, "MCL::action({name}")?;
                for (k, v) in args {
                    write!(f, ", {k}={v}")?;
                }
                write!(f, ")")
            }
            SignalKind::Mutate { path, value } => write!(f, "MCL::mutate({path}, {value})"),
            SignalKind::Transition { context } => write!(f, "MCL::transition({context})"),
            SignalKind::Dialogue { entity, text } => {
                write!(f, "MCL::dialogue({entity}, \"{text}\")")
            }
            SignalKind::Narrate { tag } => write!(f, "MCL::narrate({tag})"),
            SignalKind::Query { path } => write!(f, "MCL::query({path})"),
            SignalKind::Error { message } => write!(f, "MCL::error(\"{message}\")"),
            SignalKind::Noop => write!(f, "MCL::noop()"),
        }
    }
}

/// Result of parsing LLM output: narration text + extracted signals
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ParsedResponse {
    pub narration: Vec<String>,
    pub signals: Vec<MclSignal>,
    pub warnings: Vec<String>,
}

/// Resolution result sent back to the LLM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resolution {
    pub action: String,
    pub outcome: ResolutionOutcome,
    pub details: IndexMap<String, MclValue>,
    pub narration_hint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResolutionOutcome {
    Success,
    Failure,
    Partial,
}

impl std::fmt::Display for ResolutionOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResolutionOutcome::Success => write!(f, "success"),
            ResolutionOutcome::Failure => write!(f, "failure"),
            ResolutionOutcome::Partial => write!(f, "partial"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_span() -> Span {
        Span::new(0, 1, 1, 1)
    }

    #[test]
    fn signal_display_action() {
        let mut args = indexmap::IndexMap::new();
        args.insert("target".into(), MclValue::String("goblin".into()));
        args.insert("damage".into(), MclValue::Int(10));
        let sig = MclSignal {
            kind: SignalKind::Action {
                name: "attack".into(),
                args,
            },
            span: test_span(),
            labels: LabelSet::default(),
        };
        let s = sig.to_string();
        assert!(s.starts_with("MCL::action(attack"));
        assert!(s.contains("target=goblin"));
        assert!(s.contains("damage=10"));
    }

    #[test]
    fn signal_display_noop() {
        let sig = MclSignal {
            kind: SignalKind::Noop,
            span: test_span(),
            labels: LabelSet::default(),
        };
        assert_eq!(sig.to_string(), "MCL::noop()");
    }

    #[test]
    fn signal_display_mutate() {
        let sig = MclSignal {
            kind: SignalKind::Mutate {
                path: "player.hp".into(),
                value: MclValue::Int(0),
            },
            span: test_span(),
            labels: LabelSet::default(),
        };
        assert_eq!(sig.to_string(), "MCL::mutate(player.hp, 0)");
    }

    #[test]
    fn signal_display_dialogue() {
        let sig = MclSignal {
            kind: SignalKind::Dialogue {
                entity: "guard".into(),
                text: "Halt!".into(),
            },
            span: test_span(),
            labels: LabelSet::default(),
        };
        assert_eq!(sig.to_string(), "MCL::dialogue(guard, \"Halt!\")");
    }

    #[test]
    fn signal_display_error() {
        let sig = MclSignal {
            kind: SignalKind::Error {
                message: "bad input".into(),
            },
            span: test_span(),
            labels: LabelSet::default(),
        };
        assert_eq!(sig.to_string(), "MCL::error(\"bad input\")");
    }

    #[test]
    fn resolution_outcome_display() {
        assert_eq!(ResolutionOutcome::Success.to_string(), "success");
        assert_eq!(ResolutionOutcome::Failure.to_string(), "failure");
        assert_eq!(ResolutionOutcome::Partial.to_string(), "partial");
    }

    #[test]
    fn signal_serde_round_trip() {
        let sig = MclSignal {
            kind: SignalKind::Transition {
                context: "room_2".into(),
            },
            span: test_span(),
            labels: LabelSet::default(),
        };
        let json = serde_json::to_string(&sig).unwrap();
        let back: MclSignal = serde_json::from_str(&json).unwrap();
        assert_eq!(sig, back);
    }

    #[test]
    fn parsed_response_default() {
        let pr = ParsedResponse::default();
        assert!(pr.narration.is_empty());
        assert!(pr.signals.is_empty());
        assert!(pr.warnings.is_empty());
    }
}
