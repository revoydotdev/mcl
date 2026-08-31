//! Policy compiler (R2 §2.1).
//!
//! Converts a `PolicyDef` into a `CompiledPolicy` with type-checking and
//! basic transform validation.

use mcl_spec::policy::{PolicyDef, TransformExpr};

use crate::ast::{BindingEnv, BindingValue, CompiledPolicy, EvalErr};
use crate::eval::eval_expr;
use crate::typeck::{schema_for_event, typecheck_expr, TypeErr};

// ---------------------------------------------------------------------------
// CompileErr
// ---------------------------------------------------------------------------

/// Errors produced during policy compilation.
#[derive(Debug)]
pub enum CompileErr {
    TypeCheckFailed(TypeErr),
    InvalidTransform(String),
}

impl std::fmt::Display for CompileErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CompileErr::TypeCheckFailed(err) => write!(f, "type check failed: {err}"),
            CompileErr::InvalidTransform(msg) => write!(f, "invalid transform: {msg}"),
        }
    }
}

impl std::error::Error for CompileErr {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            CompileErr::TypeCheckFailed(err) => Some(err),
            CompileErr::InvalidTransform(_) => None,
        }
    }
}

// ---------------------------------------------------------------------------
// compile_policy
// ---------------------------------------------------------------------------

/// Compile a `PolicyDef` into a `CompiledPolicy`.
///
/// Pipeline: type-check all expressions → validate transforms → store in CompiledPolicy.
pub fn compile_policy(def: &PolicyDef) -> Result<CompiledPolicy, CompileErr> {
    let schema = schema_for_event(&def.on);

    // Type-check match expression.
    if let Some(ref expr) = def.match_expr {
        typecheck_expr(expr, &schema).map_err(CompileErr::TypeCheckFailed)?;
    }

    // Type-check require expressions.
    for expr in &def.require_exprs {
        typecheck_expr(expr, &schema).map_err(CompileErr::TypeCheckFailed)?;
    }

    // Validate transform.
    if let Some(ref transform) = def.transform {
        validate_transform(transform)?;
    }

    Ok(CompiledPolicy {
        name: def.name.clone(),
        on: def.on.clone(),
        match_expr: def.match_expr.clone(),
        require_exprs: def.require_exprs.clone(),
        transform: def.transform.clone(),
        on_violation: def.on_violation.clone(),
        priority: def.priority.clone().unwrap_or_default(),
    })
}

// ---------------------------------------------------------------------------
// Transform validation
// ---------------------------------------------------------------------------

fn validate_transform(t: &TransformExpr) -> Result<(), CompileErr> {
    match t {
        TransformExpr::DropArgs { fields } => {
            if fields.is_empty() {
                return Err(CompileErr::InvalidTransform(
                    "DropArgs requires at least one field".to_string(),
                ));
            }
            for f in fields {
                if f.trim().is_empty() {
                    return Err(CompileErr::InvalidTransform(
                        "DropArgs field name must not be empty".to_string(),
                    ));
                }
            }
            Ok(())
        }
        TransformExpr::RedactFields { fields } => {
            if fields.is_empty() {
                return Err(CompileErr::InvalidTransform(
                    "RedactFields requires at least one field".to_string(),
                ));
            }
            for f in fields {
                if f.trim().is_empty() {
                    return Err(CompileErr::InvalidTransform(
                        "RedactFields field name must not be empty".to_string(),
                    ));
                }
            }
            Ok(())
        }
    }
}

// ---------------------------------------------------------------------------
// CompiledPolicy runtime methods
// ---------------------------------------------------------------------------

impl CompiledPolicy {
    /// Check if this policy matches the given binding environment.
    ///
    /// If `match_expr` is `None`, always returns `true`.
    /// Otherwise evaluates the expression and checks truthiness.
    pub fn matches(&self, env: &BindingEnv) -> bool {
        match &self.match_expr {
            None => true,
            Some(expr) => match eval_expr(expr, env) {
                Ok(val) => is_truthy(&val),
                Err(_) => false,
            },
        }
    }

    /// Check all require expressions against the binding environment.
    ///
    /// Returns `Ok(())` if all pass, or `Err` with collected evaluation errors
    /// for any falsy requirements.
    pub fn check_requires(&self, env: &BindingEnv) -> Result<(), Vec<EvalErr>> {
        let mut errors = Vec::new();
        for expr in &self.require_exprs {
            match eval_expr(expr, env) {
                Ok(val) => {
                    if !is_truthy(&val) {
                        errors.push(EvalErr {
                            message: format!("policy '{}' requirement not satisfied", self.name),
                            expr_span: None,
                        });
                    }
                }
                Err(e) => errors.push(e),
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

/// Check if a binding value is truthy.
fn is_truthy(v: &BindingValue) -> bool {
    match v {
        BindingValue::Bool(b) => *b,
        BindingValue::Int(i) => *i != 0,
        BindingValue::Float(f) => *f != 0.0,
        BindingValue::Str(s) => !s.is_empty(),
        BindingValue::List(items) => !items.is_empty(),
        BindingValue::Set(items) => !items.is_empty(),
        BindingValue::Absent => false,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use mcl_spec::policy::{
        BinOp, Event, Lit, PolicyDef, PolicyExpr, PolicyPriority, PolicyViolationAction,
    };

    fn _signal_admit_env_with_amount(amount: i64) -> BindingEnv {
        let mut env = BindingEnv::new();
        env.insert(
            "signal",
            BindingValue::List(vec![BindingValue::Str("transfer".to_string())]),
        );
        env.insert("context", BindingValue::Bool(true));
        env.insert("signal_args_amount", BindingValue::Int(amount));
        env
    }

    #[test]
    fn compile_then_execute_admit() {
        // Policy: always match, require signal_args_amount <= 1000
        let def = PolicyDef {
            name: "amount_limit".to_string(),
            on: Event::SignalAdmit,
            match_expr: Some(PolicyExpr::Literal {
                value: Lit::Bool(true),
            }),
            require_exprs: vec![PolicyExpr::BinaryOp {
                op: BinOp::Lte,
                left: Box::new(PolicyExpr::Literal {
                    value: Lit::Int(500),
                }),
                right: Box::new(PolicyExpr::Literal {
                    value: Lit::Int(1000),
                }),
            }],
            transform: None,
            on_violation: PolicyViolationAction::Deny,
            priority: None,
        };

        let compiled = compile_policy(&def).expect("compilation should succeed");
        assert_eq!(compiled.name, "amount_limit");
        assert_eq!(compiled.priority, PolicyPriority::Default);

        let env = BindingEnv::new();
        assert!(compiled.matches(&env));
        assert!(compiled.check_requires(&env).is_ok());
    }

    #[test]
    fn compile_reject_audit_entry() {
        // Policy on AuditExport with a match expression that evaluates to false
        let def = PolicyDef {
            name: "reject_audit".to_string(),
            on: Event::AuditExport,
            match_expr: Some(PolicyExpr::Literal {
                value: Lit::Bool(false),
            }),
            require_exprs: vec![],
            transform: None,
            on_violation: PolicyViolationAction::Deny,
            priority: Some(PolicyPriority::High),
        };

        let compiled = compile_policy(&def).expect("compilation should succeed");

        let env = BindingEnv::new();
        assert!(!compiled.matches(&env));
    }

    #[test]
    fn compile_invalid_transform() {
        // DropArgs with empty fields list should fail
        let def = PolicyDef {
            name: "bad_transform".to_string(),
            on: Event::SignalAdmit,
            match_expr: None,
            require_exprs: vec![],
            transform: Some(TransformExpr::DropArgs { fields: vec![] }),
            on_violation: PolicyViolationAction::Deny,
            priority: None,
        };

        let result = compile_policy(&def);
        assert!(result.is_err());
        match result.unwrap_err() {
            CompileErr::InvalidTransform(msg) => {
                assert!(msg.contains("DropArgs"));
            }
            other => panic!("expected InvalidTransform, got: {other}"),
        }

        // RedactFields with empty field name should also fail
        let def2 = PolicyDef {
            name: "bad_redact".to_string(),
            on: Event::SignalAdmit,
            match_expr: None,
            require_exprs: vec![],
            transform: Some(TransformExpr::RedactFields {
                fields: vec!["valid".to_string(), "".to_string()],
            }),
            on_violation: PolicyViolationAction::Deny,
            priority: None,
        };

        let result2 = compile_policy(&def2);
        assert!(result2.is_err());
        match result2.unwrap_err() {
            CompileErr::InvalidTransform(msg) => {
                assert!(msg.contains("empty"));
            }
            other => panic!("expected InvalidTransform, got: {other}"),
        }
    }

    #[test]
    fn compile_all_event_schemas() {
        let events = vec![
            Event::SignalAdmit,
            Event::SignalDispatch,
            Event::NarrationEmit,
            Event::StateMutate,
            Event::AuditExport,
            Event::PhaseTransition,
            Event::SessionStart,
            Event::SessionEnd,
        ];

        for event in events {
            let def = PolicyDef {
                name: format!("test_{event:?}"),
                on: event.clone(),
                match_expr: None,
                require_exprs: vec![],
                transform: None,
                on_violation: PolicyViolationAction::LogOnly,
                priority: None,
            };
            let compiled = compile_policy(&def).unwrap_or_else(|_| panic!("compile {event:?}"));
            assert_eq!(compiled.on, event);
            assert!(compiled.matches(&BindingEnv::new()));
        }
    }

    #[test]
    fn compile_type_check_failure() {
        // Path that doesn't exist in the SignalAdmit schema
        let def = PolicyDef {
            name: "bad_path".to_string(),
            on: Event::SignalAdmit,
            match_expr: Some(PolicyExpr::Path {
                segments: vec!["nonexistent_top_level".to_string()],
            }),
            require_exprs: vec![],
            transform: None,
            on_violation: PolicyViolationAction::Deny,
            priority: None,
        };

        let result = compile_policy(&def);
        assert!(result.is_err());
        match result.unwrap_err() {
            CompileErr::TypeCheckFailed(_) => {}
            other => panic!("expected TypeCheckFailed, got: {other}"),
        }
    }

    #[test]
    fn compile_check_requires_fails() {
        let def = PolicyDef {
            name: "strict".to_string(),
            on: Event::SignalAdmit,
            match_expr: None,
            require_exprs: vec![PolicyExpr::Literal {
                value: Lit::Bool(false),
            }],
            transform: None,
            on_violation: PolicyViolationAction::Deny,
            priority: None,
        };

        let compiled = compile_policy(&def).expect("compile");
        let env = BindingEnv::new();
        let result = compiled.check_requires(&env);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("strict"));
    }

    #[test]
    fn compile_valid_transform_passes() {
        let def = PolicyDef {
            name: "redact_policy".to_string(),
            on: Event::SignalAdmit,
            match_expr: None,
            require_exprs: vec![],
            transform: Some(TransformExpr::RedactFields {
                fields: vec!["ssn".to_string(), "email".to_string()],
            }),
            on_violation: PolicyViolationAction::RedactAndWarn,
            priority: None,
        };

        let compiled = compile_policy(&def).expect("compile should succeed");
        assert!(compiled.transform.is_some());
    }

    #[test]
    fn compile_err_display() {
        let err = CompileErr::InvalidTransform("test reason".to_string());
        let msg = format!("{err}");
        assert!(msg.contains("invalid transform"));
        assert!(msg.contains("test reason"));

        let type_err = TypeErr::UnknownPath {
            path: vec!["foo".to_string()],
            available: vec![],
        };
        let err2 = CompileErr::TypeCheckFailed(type_err);
        let msg2 = format!("{err2}");
        assert!(msg2.contains("type check failed"));
    }
}
