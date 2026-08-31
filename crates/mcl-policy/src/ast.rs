//! Internal AST representation for the policy engine.
//!
//! Provides runtime types for policy evaluation: binding environments,
//! values, compiled policies, and evaluation errors.

use indexmap::IndexMap;
use mcl_spec::policy::{Event, PolicyExpr, PolicyViolationAction, TransformExpr};
use mcl_spec::span::Span;

/// Value in the binding environment.
#[derive(Debug, Clone, PartialEq)]
pub enum BindingValue {
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    List(Vec<BindingValue>),
    Set(Vec<BindingValue>),
    Absent,
}

/// Typed bindings for policy evaluation.
/// Maps binding names (signal, context, this_turn, etc.) to their values.
#[derive(Debug, Clone, Default)]
pub struct BindingEnv {
    bindings: IndexMap<String, BindingValue>,
}

impl BindingEnv {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, name: impl Into<String>, value: BindingValue) {
        self.bindings.insert(name.into(), value);
    }

    pub fn get(&self, name: &str) -> BindingValue {
        self.bindings
            .get(name)
            .cloned()
            .unwrap_or(BindingValue::Absent)
    }

    pub fn get_path(&self, segments: &[String]) -> BindingValue {
        if segments.is_empty() {
            return BindingValue::Absent;
        }

        let mut current = self.get(&segments[0]);

        for segment in &segments[1..] {
            match current {
                BindingValue::List(ref items) => {
                    if let Ok(index) = segment.parse::<usize>() {
                        if index < items.len() {
                            current = items[index].clone();
                        } else {
                            return BindingValue::Absent;
                        }
                    } else {
                        return BindingValue::Absent;
                    }
                }
                BindingValue::Set(_) => return BindingValue::Absent,
                _ => return BindingValue::Absent,
            }
        }

        current
    }
}

/// Error from expression evaluation.
#[derive(Debug, Clone)]
pub struct EvalErr {
    pub message: String,
    pub expr_span: Option<Span>,
}

/// A policy compiled from PolicyDef, ready for execution.
/// Stores the PolicyExpr AST and evaluates at runtime (no dyn Fn).
#[derive(Debug, Clone)]
pub struct CompiledPolicy {
    pub name: String,
    pub on: Event,
    pub match_expr: Option<PolicyExpr>,
    pub require_exprs: Vec<PolicyExpr>,
    pub transform: Option<TransformExpr>,
    pub on_violation: PolicyViolationAction,
    pub priority: mcl_spec::policy::PolicyPriority,
}

#[cfg(test)]
mod tests {
    use super::*;
    use mcl_spec::policy::{BinOp, Lit, PolicyPriority};

    #[test]
    fn binding_env_insert_and_lookup() {
        let mut env = BindingEnv::new();
        env.insert("signal.name", BindingValue::Str("test".to_string()));
        env.insert("context.user_id", BindingValue::Int(42));

        assert_eq!(
            env.get("signal.name"),
            BindingValue::Str("test".to_string())
        );
        assert_eq!(env.get("context.user_id"), BindingValue::Int(42));
    }

    #[test]
    fn binding_env_absent_returns_absent() {
        let env = BindingEnv::new();
        assert_eq!(env.get("nonexistent"), BindingValue::Absent);
    }

    #[test]
    fn binding_env_path_lookup() {
        let mut env = BindingEnv::new();
        env.insert(
            "signal",
            BindingValue::List(vec![
                BindingValue::Int(10),
                BindingValue::Int(20),
                BindingValue::Int(30),
            ]),
        );

        let result = env.get_path(&["signal".to_string(), "0".to_string()]);
        assert_eq!(result, BindingValue::Int(10));

        let result = env.get_path(&["signal".to_string(), "2".to_string()]);
        assert_eq!(result, BindingValue::Int(30));

        let result = env.get_path(&["signal".to_string(), "10".to_string()]);
        assert_eq!(result, BindingValue::Absent);

        let result = env.get_path(&["nonexistent".to_string()]);
        assert_eq!(result, BindingValue::Absent);
    }

    #[test]
    fn binding_env_set_does_not_support_path_navigation() {
        let mut env = BindingEnv::new();
        env.insert(
            "labels",
            BindingValue::Set(vec![
                BindingValue::Str("admin".to_string()),
                BindingValue::Str("beta".to_string()),
            ]),
        );

        let result = env.get_path(&["labels".to_string(), "0".to_string()]);
        assert_eq!(result, BindingValue::Absent);
    }

    #[test]
    fn compiled_policy_default_fields() {
        let policy = CompiledPolicy {
            name: "test_policy".to_string(),
            on: Event::SignalAdmit,
            match_expr: None,
            require_exprs: vec![],
            transform: None,
            on_violation: PolicyViolationAction::Deny,
            priority: PolicyPriority::Default,
        };

        assert_eq!(policy.name, "test_policy");
        assert_eq!(policy.on, Event::SignalAdmit);
        assert!(policy.match_expr.is_none());
        assert!(policy.require_exprs.is_empty());
        assert!(policy.transform.is_none());
        assert_eq!(policy.on_violation, PolicyViolationAction::Deny);
        assert_eq!(policy.priority, PolicyPriority::Default);
    }

    #[test]
    fn compiled_policy_with_expressions() {
        let policy = CompiledPolicy {
            name: "amount_limit".to_string(),
            on: Event::SignalAdmit,
            match_expr: Some(PolicyExpr::Literal {
                value: Lit::Bool(true),
            }),
            require_exprs: vec![PolicyExpr::BinaryOp {
                op: BinOp::Lte,
                left: Box::new(PolicyExpr::Path {
                    segments: vec!["signal".to_string(), "amount".to_string()],
                }),
                right: Box::new(PolicyExpr::Literal {
                    value: Lit::Int(1000),
                }),
            }],
            transform: None,
            on_violation: PolicyViolationAction::Reject {
                reason: "amount exceeded limit".to_string(),
            },
            priority: PolicyPriority::High,
        };

        assert_eq!(policy.name, "amount_limit");
        assert!(policy.match_expr.is_some());
        assert_eq!(policy.require_exprs.len(), 1);
        assert_eq!(policy.priority, PolicyPriority::High);
    }

    #[test]
    fn binding_value_eq() {
        let bool_val = BindingValue::Bool(true);
        let int_val = BindingValue::Int(42);
        let str_val = BindingValue::Str("hello".to_string());
        let list_val = BindingValue::List(vec![BindingValue::Int(1), BindingValue::Int(2)]);
        let set_val = BindingValue::Set(vec![BindingValue::Str("a".to_string())]);
        let absent_val = BindingValue::Absent;

        assert_eq!(bool_val, BindingValue::Bool(true));
        assert_eq!(int_val, BindingValue::Int(42));
        assert_eq!(str_val, BindingValue::Str("hello".to_string()));
        assert_eq!(
            list_val,
            BindingValue::List(vec![BindingValue::Int(1), BindingValue::Int(2)])
        );
        assert_eq!(
            set_val,
            BindingValue::Set(vec![BindingValue::Str("a".to_string())])
        );
        assert_eq!(absent_val, BindingValue::Absent);
    }

    #[test]
    fn binding_env_clone() {
        let mut env = BindingEnv::new();
        env.insert("key", BindingValue::Int(42));

        let mut cloned = env.clone();
        assert_eq!(cloned.get("key"), BindingValue::Int(42));

        cloned.insert("new_key", BindingValue::Str("test".to_string()));
        assert_eq!(env.get("new_key"), BindingValue::Absent);
    }

    #[test]
    fn eval_err_creation() {
        let err = EvalErr {
            message: "division by zero".to_string(),
            expr_span: Some(Span::new(10, 5, 1, 10)),
        };

        assert_eq!(err.message, "division by zero");
        assert!(err.expr_span.is_some());
        assert_eq!(err.expr_span.unwrap().offset, 10);
    }

    #[test]
    fn eval_err_without_span() {
        let err = EvalErr {
            message: "unexpected error".to_string(),
            expr_span: None,
        };

        assert_eq!(err.message, "unexpected error");
        assert!(err.expr_span.is_none());
    }

    #[test]
    fn compiled_policy_clone() {
        let policy = CompiledPolicy {
            name: "clone_test".to_string(),
            on: Event::SignalDispatch,
            match_expr: Some(PolicyExpr::Literal {
                value: Lit::Bool(false),
            }),
            require_exprs: vec![],
            transform: None,
            on_violation: PolicyViolationAction::LogOnly,
            priority: PolicyPriority::Default,
        };

        let cloned = policy.clone();
        assert_eq!(cloned.name, "clone_test");
        assert_eq!(cloned.on, Event::SignalDispatch);
    }
}
