//! Policy types, DSL expressions, and violation actions (R2 §1.1-1.3, §2.2-2.3).
//!
//! Types only — no evaluation logic. `PolicyViolationAction` is separate from
//! R1's `ViolationAction` in `narrator.rs`.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Event
// ---------------------------------------------------------------------------

/// Events that can trigger a policy evaluation (R2 §2.2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Event {
    SignalAdmit,
    SignalDispatch,
    NarrationEmit,
    StateMutate,
    AuditExport,
    PhaseTransition,
    SessionStart,
    SessionEnd,
}

// ---------------------------------------------------------------------------
// PolicyViolationAction
// ---------------------------------------------------------------------------

/// Violation actions specific to policies (R2 §2.3).
///
/// Separate from R1's `ViolationAction` in `narrator.rs`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyViolationAction {
    Reject { reason: String },
    Deny,
    RedactAndWarn,
    Rewrite,
    RouteTo { channel: String },
    DemandConfirmation { prompt: Option<String> },
    LogOnly,
    AuditOnly,
}

// ---------------------------------------------------------------------------
// PolicyPriority
// ---------------------------------------------------------------------------

/// Policy priority level (R2 §6).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PolicyPriority {
    #[default]
    Default,
    High,
}

// ---------------------------------------------------------------------------
// BinOp / UnOp / Lit
// ---------------------------------------------------------------------------

/// Binary operators for policy expressions (R2 §1.3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BinOp {
    Eq,
    Neq,
    Lt,
    Lte,
    Gt,
    Gte,
    Add,
    Sub,
    Mul,
    Div,
    And,
    Or,
}

/// Unary operators for policy expressions (R2 §1.3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnOp {
    Not,
    Neg,
}

/// Literals in policy expressions (R2 §1.3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Lit {
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
}

// ---------------------------------------------------------------------------
// PolicyExpr
// ---------------------------------------------------------------------------

/// Expression AST for the policy DSL (R2 §1.3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PolicyExpr {
    Literal {
        value: Lit,
    },
    Path {
        segments: Vec<String>,
    },
    BinaryOp {
        op: BinOp,
        left: Box<PolicyExpr>,
        right: Box<PolicyExpr>,
    },
    UnaryOp {
        op: UnOp,
        operand: Box<PolicyExpr>,
    },
    Call {
        func: String,
        args: Vec<PolicyExpr>,
    },
    Let {
        name: String,
        value: Box<PolicyExpr>,
        body: Box<PolicyExpr>,
    },
}

// ---------------------------------------------------------------------------
// TransformExpr
// ---------------------------------------------------------------------------

/// Transform expressions applied on policy violation (R2 §1.2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TransformExpr {
    DropArgs { fields: Vec<String> },
    RedactFields { fields: Vec<String> },
}

// ---------------------------------------------------------------------------
// PolicyDef
// ---------------------------------------------------------------------------

/// A policy definition (R2 §1.1-1.2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PolicyDef {
    pub name: String,
    pub on: Event,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub match_expr: Option<PolicyExpr>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub require_exprs: Vec<PolicyExpr>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transform: Option<TransformExpr>,
    pub on_violation: PolicyViolationAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<PolicyPriority>,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_serialize_roundtrip() {
        let event = Event::SignalAdmit;
        let json = serde_json::to_string(&event).unwrap();
        assert_eq!(json, "\"signal_admit\"");
        let back: Event = serde_json::from_str(&json).unwrap();
        assert_eq!(event, back);
    }

    #[test]
    fn violation_action_reject_has_reason() {
        let action = PolicyViolationAction::Reject {
            reason: "daily_limit".into(),
        };
        let json = serde_json::to_string(&action).unwrap();
        assert!(json.contains("daily_limit"));
        assert!(json.contains("reject"));
        let back: PolicyViolationAction = serde_json::from_str(&json).unwrap();
        assert_eq!(action, back);
    }

    #[test]
    fn policy_def_default_fields() {
        let policy = PolicyDef {
            name: "test_policy".into(),
            on: Event::SignalAdmit,
            match_expr: None,
            require_exprs: vec![],
            transform: None,
            on_violation: PolicyViolationAction::Deny,
            priority: None,
        };
        let json = serde_json::to_string(&policy).unwrap();
        let obj: serde_json::Value = serde_json::from_str(&json).unwrap();
        // Required fields must be present
        assert_eq!(obj["name"], "test_policy");
        assert_eq!(obj["on"], "signal_admit");
        assert_eq!(obj["on_violation"], "deny");
        // Optional fields must be absent when default/empty
        assert!(obj.get("match_expr").is_none());
        assert!(obj.get("require_exprs").is_none());
        assert!(obj.get("transform").is_none());
        assert!(obj.get("priority").is_none());
        // Roundtrip
        let back: PolicyDef = serde_json::from_str(&json).unwrap();
        assert_eq!(policy, back);
    }

    #[test]
    fn policy_expr_literal_and_path() {
        let lit = PolicyExpr::Literal {
            value: Lit::Bool(true),
        };
        let json = serde_json::to_string(&lit).unwrap();
        assert!(json.contains("\"type\":\"literal\""));
        let back: PolicyExpr = serde_json::from_str(&json).unwrap();
        assert_eq!(lit, back);

        let path = PolicyExpr::Path {
            segments: vec!["signal".into(), "name".into()],
        };
        let json = serde_json::to_string(&path).unwrap();
        assert!(json.contains("\"type\":\"path\""));
        assert!(json.contains("signal"));
        let back: PolicyExpr = serde_json::from_str(&json).unwrap();
        assert_eq!(path, back);
    }

    #[test]
    fn binop_precedence_types() {
        let variants = vec![
            BinOp::Eq,
            BinOp::Neq,
            BinOp::Lt,
            BinOp::Lte,
            BinOp::Gt,
            BinOp::Gte,
            BinOp::Add,
            BinOp::Sub,
            BinOp::Mul,
            BinOp::Div,
            BinOp::And,
            BinOp::Or,
        ];
        for op in variants {
            let json = serde_json::to_string(&op).unwrap();
            let back: BinOp = serde_json::from_str(&json).unwrap();
            assert_eq!(op, back);
        }
    }
}
