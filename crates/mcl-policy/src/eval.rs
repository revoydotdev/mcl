//! Tree-walking expression evaluator for the policy DSL (R2 §2.1 step 3).
//!
//! Evaluates `PolicyExpr` against a `BindingEnv` to produce `BindingValue` results.
//! Pure functions — no side effects during evaluation.

use mcl_spec::policy::{BinOp, Lit, PolicyExpr, UnOp};

use crate::ast::{BindingEnv, BindingValue, EvalErr};

/// Evaluate a policy expression against a binding environment.
pub fn eval_expr(expr: &PolicyExpr, env: &BindingEnv) -> Result<BindingValue, EvalErr> {
    match expr {
        PolicyExpr::Literal { value } => eval_literal(value),

        PolicyExpr::Path { segments } => {
            let val = env.get_path(segments);
            Ok(val)
        }

        PolicyExpr::BinaryOp { op, left, right } => {
            let lhs = eval_expr(left, env)?;
            let rhs = eval_expr(right, env)?;
            eval_binop(op, lhs, rhs)
        }

        PolicyExpr::UnaryOp { op, operand } => {
            let val = eval_expr(operand, env)?;
            eval_unop(op, val)
        }

        PolicyExpr::Call { func, args } => {
            let evaluated: Result<Vec<BindingValue>, EvalErr> =
                args.iter().map(|a| eval_expr(a, env)).collect();
            eval_call(func, evaluated?)
        }

        PolicyExpr::Let { name, value, body } => {
            let val = eval_expr(value, env)?;
            let mut child_env = env.clone();
            child_env.insert(name.clone(), val);
            eval_expr(body, &child_env)
        }
    }
}

fn eval_literal(lit: &Lit) -> Result<BindingValue, EvalErr> {
    match lit {
        Lit::Bool(b) => Ok(BindingValue::Bool(*b)),
        Lit::Int(i) => Ok(BindingValue::Int(*i)),
        Lit::Float(f) => Ok(BindingValue::Float(*f)),
        Lit::Str(s) => Ok(BindingValue::Str(s.clone())),
    }
}

fn eval_binop(op: &BinOp, lhs: BindingValue, rhs: BindingValue) -> Result<BindingValue, EvalErr> {
    match op {
        BinOp::Eq => Ok(BindingValue::Bool(binding_eq(&lhs, &rhs))),
        BinOp::Neq => Ok(BindingValue::Bool(!binding_eq(&lhs, &rhs))),
        BinOp::Lt => binding_ord(&lhs, &rhs, |o| o.is_lt()),
        BinOp::Lte => binding_ord(&lhs, &rhs, |o| o.is_le()),
        BinOp::Gt => binding_ord(&lhs, &rhs, |o| o.is_gt()),
        BinOp::Gte => binding_ord(&lhs, &rhs, |o| o.is_ge()),
        BinOp::Add => arithmetic_op(&lhs, &rhs, |a, b| a + b, |a, b| a + b, "+"),
        BinOp::Sub => arithmetic_op(&lhs, &rhs, |a, b| a - b, |a, b| a - b, "-"),
        BinOp::Mul => arithmetic_op(&lhs, &rhs, |a, b| a * b, |a, b| a * b, "*"),
        BinOp::Div => match (&lhs, &rhs) {
            (BindingValue::Int(a), BindingValue::Int(b)) => {
                if *b == 0 {
                    return Err(EvalErr {
                        message: "division by zero".to_string(),
                        expr_span: None,
                    });
                }
                Ok(BindingValue::Int(a / b))
            }
            (BindingValue::Float(a), BindingValue::Float(b)) => {
                if *b == 0.0 {
                    return Err(EvalErr {
                        message: "division by zero".to_string(),
                        expr_span: None,
                    });
                }
                Ok(BindingValue::Float(a / b))
            }
            (BindingValue::Int(a), BindingValue::Float(b)) => {
                if *b == 0.0 {
                    return Err(EvalErr {
                        message: "division by zero".to_string(),
                        expr_span: None,
                    });
                }
                Ok(BindingValue::Float(*a as f64 / b))
            }
            (BindingValue::Float(a), BindingValue::Int(b)) => {
                if *b == 0 {
                    return Err(EvalErr {
                        message: "division by zero".to_string(),
                        expr_span: None,
                    });
                }
                Ok(BindingValue::Float(a / *b as f64))
            }
            _ => Err(EvalErr {
                message: format!("type error: cannot divide {:?} by {:?}", lhs, rhs),
                expr_span: None,
            }),
        },
        BinOp::And => match (&lhs, &rhs) {
            (BindingValue::Bool(a), BindingValue::Bool(b)) => Ok(BindingValue::Bool(*a && *b)),
            _ => Err(EvalErr {
                message: format!(
                    "type error: 'and' requires bools, got {:?} and {:?}",
                    lhs, rhs
                ),
                expr_span: None,
            }),
        },
        BinOp::Or => match (&lhs, &rhs) {
            (BindingValue::Bool(a), BindingValue::Bool(b)) => Ok(BindingValue::Bool(*a || *b)),
            _ => Err(EvalErr {
                message: format!(
                    "type error: 'or' requires bools, got {:?} and {:?}",
                    lhs, rhs
                ),
                expr_span: None,
            }),
        },
    }
}

fn eval_unop(op: &UnOp, val: BindingValue) -> Result<BindingValue, EvalErr> {
    match op {
        UnOp::Not => match val {
            BindingValue::Bool(b) => Ok(BindingValue::Bool(!b)),
            _ => Err(EvalErr {
                message: format!("type error: 'not' requires bool, got {:?}", val),
                expr_span: None,
            }),
        },
        UnOp::Neg => match val {
            BindingValue::Int(i) => Ok(BindingValue::Int(-i)),
            BindingValue::Float(f) => Ok(BindingValue::Float(-f)),
            _ => Err(EvalErr {
                message: format!("type error: negation requires number, got {:?}", val),
                expr_span: None,
            }),
        },
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn binding_eq(a: &BindingValue, b: &BindingValue) -> bool {
    match (a, b) {
        (BindingValue::Bool(x), BindingValue::Bool(y)) => x == y,
        (BindingValue::Int(x), BindingValue::Int(y)) => x == y,
        (BindingValue::Float(x), BindingValue::Float(y)) => x == y,
        (BindingValue::Int(x), BindingValue::Float(y)) => (*x as f64) == *y,
        (BindingValue::Float(x), BindingValue::Int(y)) => *x == (*y as f64),
        (BindingValue::Str(x), BindingValue::Str(y)) => x == y,
        (BindingValue::Absent, BindingValue::Absent) => true,
        _ => false,
    }
}

fn binding_ord(
    lhs: &BindingValue,
    rhs: &BindingValue,
    cmp: impl Fn(std::cmp::Ordering) -> bool,
) -> Result<BindingValue, EvalErr> {
    let ord = match (lhs, rhs) {
        (BindingValue::Int(a), BindingValue::Int(b)) => Some(a.cmp(b)),
        (BindingValue::Float(a), BindingValue::Float(b)) => a.partial_cmp(b),
        (BindingValue::Int(a), BindingValue::Float(b)) => (*a as f64).partial_cmp(b),
        (BindingValue::Float(a), BindingValue::Int(b)) => a.partial_cmp(&(*b as f64)),
        (BindingValue::Str(a), BindingValue::Str(b)) => Some(a.cmp(b)),
        (BindingValue::Bool(a), BindingValue::Bool(b)) => Some(a.cmp(b)),
        _ => {
            return Err(EvalErr {
                message: format!("type error: cannot compare {:?} with {:?}", lhs, rhs),
                expr_span: None,
            });
        }
    };

    match ord {
        Some(o) => Ok(BindingValue::Bool(cmp(o))),
        None => Ok(BindingValue::Bool(false)),
    }
}

fn arithmetic_op(
    lhs: &BindingValue,
    rhs: &BindingValue,
    int_fn: impl Fn(i64, i64) -> i64,
    float_fn: impl Fn(f64, f64) -> f64,
    symbol: &str,
) -> Result<BindingValue, EvalErr> {
    match (lhs, rhs) {
        (BindingValue::Int(a), BindingValue::Int(b)) => Ok(BindingValue::Int(int_fn(*a, *b))),
        (BindingValue::Float(a), BindingValue::Float(b)) => {
            Ok(BindingValue::Float(float_fn(*a, *b)))
        }
        (BindingValue::Int(a), BindingValue::Float(b)) => {
            Ok(BindingValue::Float(float_fn(*a as f64, *b)))
        }
        (BindingValue::Float(a), BindingValue::Int(b)) => {
            Ok(BindingValue::Float(float_fn(*a, *b as f64)))
        }
        _ => Err(EvalErr {
            message: format!(
                "type error: cannot apply '{}' to {:?} and {:?}",
                symbol, lhs, rhs
            ),
            expr_span: None,
        }),
    }
}

fn as_f64(v: &BindingValue) -> Option<f64> {
    match v {
        BindingValue::Int(i) => Some(*i as f64),
        BindingValue::Float(f) => Some(*f),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Stdlib call evaluation
// ---------------------------------------------------------------------------

fn eval_call(func: &str, args: Vec<BindingValue>) -> Result<BindingValue, EvalErr> {
    match func {
        "sum" => call_sum(args),
        "count" => call_count(args),
        "min" => call_min(args),
        "max" => call_max(args),
        "exists" => call_exists(args),
        "forall" => call_forall(args),
        "contains" => call_contains(args),
        "labels" => call_labels(args),
        "path_eq" => call_path_eq(args),
        "now" => call_now(args),
        "turn" => call_turn(args),
        "phase" => call_phase(args),
        "regex_match" => call_regex_match(args),
        _ => Err(EvalErr {
            message: format!("unknown function: {}", func),
            expr_span: None,
        }),
    }
}

fn call_sum(args: Vec<BindingValue>) -> Result<BindingValue, EvalErr> {
    if args.len() != 1 {
        return Err(EvalErr {
            message: format!("sum() takes 1 argument, got {}", args.len()),
            expr_span: None,
        });
    }
    match &args[0] {
        BindingValue::List(items) => {
            let mut total: f64 = 0.0;
            let mut all_int = true;
            for item in items {
                match as_f64(item) {
                    Some(v) => total += v,
                    None => {
                        return Err(EvalErr {
                            message: format!("sum() requires numeric elements, got {:?}", item),
                            expr_span: None,
                        });
                    }
                }
                if !matches!(item, BindingValue::Int(_)) {
                    all_int = false;
                }
            }
            if all_int {
                Ok(BindingValue::Int(total as i64))
            } else {
                Ok(BindingValue::Float(total))
            }
        }
        BindingValue::Absent => Ok(BindingValue::Int(0)),
        _ => Err(EvalErr {
            message: format!("sum() requires a list, got {:?}", args[0]),
            expr_span: None,
        }),
    }
}

fn call_count(args: Vec<BindingValue>) -> Result<BindingValue, EvalErr> {
    if args.len() != 1 {
        return Err(EvalErr {
            message: format!("count() takes 1 argument, got {}", args.len()),
            expr_span: None,
        });
    }
    match &args[0] {
        BindingValue::List(items) => Ok(BindingValue::Int(items.len() as i64)),
        BindingValue::Set(items) => Ok(BindingValue::Int(items.len() as i64)),
        BindingValue::Str(s) => Ok(BindingValue::Int(s.len() as i64)),
        BindingValue::Absent => Ok(BindingValue::Int(0)),
        _ => Err(EvalErr {
            message: format!("count() requires a collection, got {:?}", args[0]),
            expr_span: None,
        }),
    }
}

fn call_min(args: Vec<BindingValue>) -> Result<BindingValue, EvalErr> {
    if args.len() != 2 {
        return Err(EvalErr {
            message: format!("min() takes 2 arguments, got {}", args.len()),
            expr_span: None,
        });
    }
    let a = as_f64(&args[0]).ok_or_else(|| EvalErr {
        message: format!("min() requires numbers, got {:?}", args[0]),
        expr_span: None,
    })?;
    let b = as_f64(&args[1]).ok_or_else(|| EvalErr {
        message: format!("min() requires numbers, got {:?}", args[1]),
        expr_span: None,
    })?;
    let result = a.min(b);
    if matches!(args[0], BindingValue::Int(_)) && matches!(args[1], BindingValue::Int(_)) {
        Ok(BindingValue::Int(result as i64))
    } else {
        Ok(BindingValue::Float(result))
    }
}

fn call_max(args: Vec<BindingValue>) -> Result<BindingValue, EvalErr> {
    if args.len() != 2 {
        return Err(EvalErr {
            message: format!("max() takes 2 arguments, got {}", args.len()),
            expr_span: None,
        });
    }
    let a = as_f64(&args[0]).ok_or_else(|| EvalErr {
        message: format!("max() requires numbers, got {:?}", args[0]),
        expr_span: None,
    })?;
    let b = as_f64(&args[1]).ok_or_else(|| EvalErr {
        message: format!("max() requires numbers, got {:?}", args[1]),
        expr_span: None,
    })?;
    let result = a.max(b);
    if matches!(args[0], BindingValue::Int(_)) && matches!(args[1], BindingValue::Int(_)) {
        Ok(BindingValue::Int(result as i64))
    } else {
        Ok(BindingValue::Float(result))
    }
}

fn call_exists(args: Vec<BindingValue>) -> Result<BindingValue, EvalErr> {
    if args.len() != 1 {
        return Err(EvalErr {
            message: format!("exists() takes 1 argument, got {}", args.len()),
            expr_span: None,
        });
    }
    match &args[0] {
        BindingValue::List(items) => Ok(BindingValue::Bool(!items.is_empty())),
        BindingValue::Set(items) => Ok(BindingValue::Bool(!items.is_empty())),
        BindingValue::Str(s) => Ok(BindingValue::Bool(!s.is_empty())),
        _ => Err(EvalErr {
            message: format!("exists() requires a collection, got {:?}", args[0]),
            expr_span: None,
        }),
    }
}

fn call_forall(args: Vec<BindingValue>) -> Result<BindingValue, EvalErr> {
    if args.len() != 1 {
        return Err(EvalErr {
            message: format!("forall() takes 1 argument, got {}", args.len()),
            expr_span: None,
        });
    }
    match &args[0] {
        BindingValue::List(items) => {
            let all_truthy = items.iter().all(is_truthy);
            Ok(BindingValue::Bool(all_truthy))
        }
        BindingValue::Set(items) => {
            let all_truthy = items.iter().all(is_truthy);
            Ok(BindingValue::Bool(all_truthy))
        }
        _ => Err(EvalErr {
            message: format!("forall() requires a collection, got {:?}", args[0]),
            expr_span: None,
        }),
    }
}

fn call_contains(args: Vec<BindingValue>) -> Result<BindingValue, EvalErr> {
    if args.len() != 2 {
        return Err(EvalErr {
            message: format!("contains() takes 2 arguments, got {}", args.len()),
            expr_span: None,
        });
    }
    let haystack = &args[0];
    let needle = &args[1];
    match (haystack, needle) {
        (BindingValue::Str(hay), BindingValue::Str(nee)) => {
            Ok(BindingValue::Bool(hay.contains(nee.as_str())))
        }
        (BindingValue::List(items), needle) => Ok(BindingValue::Bool(
            items.iter().any(|i| binding_eq(i, needle)),
        )),
        (BindingValue::Set(items), needle) => Ok(BindingValue::Bool(
            items.iter().any(|i| binding_eq(i, needle)),
        )),
        _ => Err(EvalErr {
            message: format!(
                "contains() requires string/list/set and value, got {:?} and {:?}",
                haystack, needle
            ),
            expr_span: None,
        }),
    }
}

fn call_labels(args: Vec<BindingValue>) -> Result<BindingValue, EvalErr> {
    if args.len() != 1 {
        return Err(EvalErr {
            message: format!("labels() takes 1 argument, got {}", args.len()),
            expr_span: None,
        });
    }
    match &args[0] {
        BindingValue::Str(_) => Ok(BindingValue::Bool(false)),
        _ => Err(EvalErr {
            message: format!("labels() requires a string tag, got {:?}", args[0]),
            expr_span: None,
        }),
    }
}

fn call_path_eq(args: Vec<BindingValue>) -> Result<BindingValue, EvalErr> {
    if args.len() != 2 {
        return Err(EvalErr {
            message: format!("path_eq() takes 2 arguments, got {}", args.len()),
            expr_span: None,
        });
    }
    Ok(BindingValue::Bool(binding_eq(&args[0], &args[1])))
}

fn call_now(args: Vec<BindingValue>) -> Result<BindingValue, EvalErr> {
    if !args.is_empty() {
        return Err(EvalErr {
            message: format!("now() takes 0 arguments, got {}", args.len()),
            expr_span: None,
        });
    }
    Ok(BindingValue::Int(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0),
    ))
}

fn call_turn(args: Vec<BindingValue>) -> Result<BindingValue, EvalErr> {
    if !args.is_empty() {
        return Err(EvalErr {
            message: format!("turn() takes 0 arguments, got {}", args.len()),
            expr_span: None,
        });
    }
    Ok(BindingValue::Int(0))
}

fn call_phase(args: Vec<BindingValue>) -> Result<BindingValue, EvalErr> {
    if !args.is_empty() {
        return Err(EvalErr {
            message: format!("phase() takes 0 arguments, got {}", args.len()),
            expr_span: None,
        });
    }
    Ok(BindingValue::Absent)
}

fn call_regex_match(args: Vec<BindingValue>) -> Result<BindingValue, EvalErr> {
    if args.len() != 2 {
        return Err(EvalErr {
            message: format!("regex_match() takes 2 arguments, got {}", args.len()),
            expr_span: None,
        });
    }
    let pattern = match &args[0] {
        BindingValue::Str(s) => s.as_str(),
        _ => {
            return Err(EvalErr {
                message: format!("regex_match() requires string pattern, got {:?}", args[0]),
                expr_span: None,
            });
        }
    };
    let text = match &args[1] {
        BindingValue::Str(s) => s.as_str(),
        _ => {
            return Err(EvalErr {
                message: format!("regex_match() requires string text, got {:?}", args[1]),
                expr_span: None,
            });
        }
    };
    let result = simple_pattern_match(pattern, text);
    Ok(BindingValue::Bool(result))
}

fn simple_pattern_match(pattern: &str, text: &str) -> bool {
    if !pattern.contains('*') && !pattern.contains('?') {
        text.contains(pattern)
    } else {
        let segments: Vec<&str> = pattern.split('*').collect();
        if segments.is_empty() {
            return true;
        }
        let mut search_from = 0;
        for seg in &segments {
            if seg.is_empty() {
                continue;
            }
            let pos = match text[search_from..].find(seg) {
                Some(p) => p + search_from,
                None => return false,
            };
            search_from = pos + seg.len();
        }
        true
    }
}

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

    // Helper to make a binding env quickly
    fn env(pairs: Vec<(&str, BindingValue)>) -> BindingEnv {
        let mut e = BindingEnv::new();
        for (k, v) in pairs {
            e.insert(k, v);
        }
        e
    }

    fn expr_int(i: i64) -> PolicyExpr {
        PolicyExpr::Literal { value: Lit::Int(i) }
    }
    fn expr_float(f: f64) -> PolicyExpr {
        PolicyExpr::Literal {
            value: Lit::Float(f),
        }
    }
    fn expr_str(s: &str) -> PolicyExpr {
        PolicyExpr::Literal {
            value: Lit::Str(s.to_string()),
        }
    }
    fn expr_bool(b: bool) -> PolicyExpr {
        PolicyExpr::Literal {
            value: Lit::Bool(b),
        }
    }

    #[test]
    fn eval_literal_bool_int_float_str() {
        let e = BindingEnv::new();
        assert_eq!(
            eval_expr(&expr_bool(true), &e).ok(),
            Some(BindingValue::Bool(true))
        );
        assert_eq!(
            eval_expr(&expr_bool(false), &e).ok(),
            Some(BindingValue::Bool(false))
        );
        assert_eq!(
            eval_expr(&expr_int(42), &e).ok(),
            Some(BindingValue::Int(42))
        );
        assert_eq!(
            eval_expr(&expr_float(3.14), &e).ok(),
            Some(BindingValue::Float(3.14))
        );
        assert_eq!(
            eval_expr(&expr_str("hello"), &e).ok(),
            Some(BindingValue::Str("hello".to_string()))
        );
    }

    #[test]
    fn eval_binary_ops_arithmetic() {
        let e = BindingEnv::new();

        // 1 + 2 = 3
        let add = PolicyExpr::BinaryOp {
            op: BinOp::Add,
            left: Box::new(expr_int(1)),
            right: Box::new(expr_int(2)),
        };
        assert_eq!(eval_expr(&add, &e).ok(), Some(BindingValue::Int(3)));

        // 10 - 3 = 7
        let sub = PolicyExpr::BinaryOp {
            op: BinOp::Sub,
            left: Box::new(expr_int(10)),
            right: Box::new(expr_int(3)),
        };
        assert_eq!(eval_expr(&sub, &e).ok(), Some(BindingValue::Int(7)));

        // 6 / 2 = 3
        let div = PolicyExpr::BinaryOp {
            op: BinOp::Div,
            left: Box::new(expr_int(6)),
            right: Box::new(expr_int(2)),
        };
        assert_eq!(eval_expr(&div, &e).ok(), Some(BindingValue::Int(3)));

        // 3 * 4 = 12
        let mul = PolicyExpr::BinaryOp {
            op: BinOp::Mul,
            left: Box::new(expr_int(3)),
            right: Box::new(expr_int(4)),
        };
        assert_eq!(eval_expr(&mul, &e).ok(), Some(BindingValue::Int(12)));

        // float arithmetic
        let fadd = PolicyExpr::BinaryOp {
            op: BinOp::Add,
            left: Box::new(expr_float(1.5)),
            right: Box::new(expr_float(2.5)),
        };
        assert_eq!(eval_expr(&fadd, &e).ok(), Some(BindingValue::Float(4.0)));

        // mixed int/float
        let mixed = PolicyExpr::BinaryOp {
            op: BinOp::Add,
            left: Box::new(expr_int(1)),
            right: Box::new(expr_float(2.5)),
        };
        assert_eq!(eval_expr(&mixed, &e).ok(), Some(BindingValue::Float(3.5)));
    }

    #[test]
    fn eval_binary_ops_comparison() {
        let e = BindingEnv::new();

        // 1 == 1 -> true
        let eq = PolicyExpr::BinaryOp {
            op: BinOp::Eq,
            left: Box::new(expr_int(1)),
            right: Box::new(expr_int(1)),
        };
        assert_eq!(eval_expr(&eq, &e).ok(), Some(BindingValue::Bool(true)));

        // 1 != 2 -> true
        let neq = PolicyExpr::BinaryOp {
            op: BinOp::Neq,
            left: Box::new(expr_int(1)),
            right: Box::new(expr_int(2)),
        };
        assert_eq!(eval_expr(&neq, &e).ok(), Some(BindingValue::Bool(true)));

        // 3 < 5 -> true
        let lt = PolicyExpr::BinaryOp {
            op: BinOp::Lt,
            left: Box::new(expr_int(3)),
            right: Box::new(expr_int(5)),
        };
        assert_eq!(eval_expr(&lt, &e).ok(), Some(BindingValue::Bool(true)));

        // 5 <= 5 -> true
        let lte = PolicyExpr::BinaryOp {
            op: BinOp::Lte,
            left: Box::new(expr_int(5)),
            right: Box::new(expr_int(5)),
        };
        assert_eq!(eval_expr(&lte, &e).ok(), Some(BindingValue::Bool(true)));

        // 5 > 3 -> true
        let gt = PolicyExpr::BinaryOp {
            op: BinOp::Gt,
            left: Box::new(expr_int(5)),
            right: Box::new(expr_int(3)),
        };
        assert_eq!(eval_expr(&gt, &e).ok(), Some(BindingValue::Bool(true)));

        // 3 >= 5 -> false
        let gte = PolicyExpr::BinaryOp {
            op: BinOp::Gte,
            left: Box::new(expr_int(3)),
            right: Box::new(expr_int(5)),
        };
        assert_eq!(eval_expr(&gte, &e).ok(), Some(BindingValue::Bool(false)));

        // string equality
        let seq = PolicyExpr::BinaryOp {
            op: BinOp::Eq,
            left: Box::new(expr_str("abc")),
            right: Box::new(expr_str("abc")),
        };
        assert_eq!(eval_expr(&seq, &e).ok(), Some(BindingValue::Bool(true)));

        // string comparison
        let slt = PolicyExpr::BinaryOp {
            op: BinOp::Lt,
            left: Box::new(expr_str("a")),
            right: Box::new(expr_str("b")),
        };
        assert_eq!(eval_expr(&slt, &e).ok(), Some(BindingValue::Bool(true)));
    }

    #[test]
    fn eval_binary_ops_logical() {
        let e = BindingEnv::new();

        // true and false -> false
        let and = PolicyExpr::BinaryOp {
            op: BinOp::And,
            left: Box::new(expr_bool(true)),
            right: Box::new(expr_bool(false)),
        };
        assert_eq!(eval_expr(&and, &e).ok(), Some(BindingValue::Bool(false)));

        // true or false -> true
        let or = PolicyExpr::BinaryOp {
            op: BinOp::Or,
            left: Box::new(expr_bool(true)),
            right: Box::new(expr_bool(false)),
        };
        assert_eq!(eval_expr(&or, &e).ok(), Some(BindingValue::Bool(true)));

        // true and true -> true
        let and2 = PolicyExpr::BinaryOp {
            op: BinOp::And,
            left: Box::new(expr_bool(true)),
            right: Box::new(expr_bool(true)),
        };
        assert_eq!(eval_expr(&and2, &e).ok(), Some(BindingValue::Bool(true)));

        // false or false -> false
        let or2 = PolicyExpr::BinaryOp {
            op: BinOp::Or,
            left: Box::new(expr_bool(false)),
            right: Box::new(expr_bool(false)),
        };
        assert_eq!(eval_expr(&or2, &e).ok(), Some(BindingValue::Bool(false)));
    }

    #[test]
    fn eval_path_lookup() {
        let e = env(vec![(
            "context",
            BindingValue::List(vec![BindingValue::Int(100), BindingValue::Int(200)]),
        )]);

        // context.0 -> 100
        let path = PolicyExpr::Path {
            segments: vec!["context".to_string(), "0".to_string()],
        };
        assert_eq!(eval_expr(&path, &e).ok(), Some(BindingValue::Int(100)));

        // context.1 -> 200
        let path1 = PolicyExpr::Path {
            segments: vec!["context".to_string(), "1".to_string()],
        };
        assert_eq!(eval_expr(&path1, &e).ok(), Some(BindingValue::Int(200)));

        // nonexistent -> Absent
        let missing = PolicyExpr::Path {
            segments: vec!["nonexistent".to_string()],
        };
        assert_eq!(eval_expr(&missing, &e).ok(), Some(BindingValue::Absent));
    }

    #[test]
    fn eval_call_sum_count() {
        // sum([1,2,3]) -> 6
        let result = call_sum(vec![BindingValue::List(vec![
            BindingValue::Int(1),
            BindingValue::Int(2),
            BindingValue::Int(3),
        ])]);
        assert_eq!(result.ok(), Some(BindingValue::Int(6)));

        // count([1,2,3]) -> 3
        let result = call_count(vec![BindingValue::List(vec![
            BindingValue::Int(1),
            BindingValue::Int(2),
            BindingValue::Int(3),
        ])]);
        assert_eq!(result.ok(), Some(BindingValue::Int(3)));

        // sum of empty list -> 0
        let result = call_sum(vec![BindingValue::List(vec![])]);
        assert_eq!(result.ok(), Some(BindingValue::Int(0)));

        // sum via expression eval
        let sum_expr = PolicyExpr::Call {
            func: "sum".to_string(),
            args: vec![PolicyExpr::Path {
                segments: vec!["nums".to_string()],
            }],
        };
        let env_with_list = env(vec![(
            "nums",
            BindingValue::List(vec![
                BindingValue::Int(1),
                BindingValue::Int(2),
                BindingValue::Int(3),
            ]),
        )]);
        assert_eq!(
            eval_expr(&sum_expr, &env_with_list).ok(),
            Some(BindingValue::Int(6))
        );
    }

    #[test]
    fn eval_call_contains() {
        // contains("hello world", "world") -> true
        let result = call_contains(vec![
            BindingValue::Str("hello world".to_string()),
            BindingValue::Str("world".to_string()),
        ]);
        assert_eq!(result.ok(), Some(BindingValue::Bool(true)));

        // contains("hello world", "xyz") -> false
        let result = call_contains(vec![
            BindingValue::Str("hello world".to_string()),
            BindingValue::Str("xyz".to_string()),
        ]);
        assert_eq!(result.ok(), Some(BindingValue::Bool(false)));

        // contains list
        let result = call_contains(vec![
            BindingValue::List(vec![
                BindingValue::Int(1),
                BindingValue::Int(2),
                BindingValue::Int(3),
            ]),
            BindingValue::Int(2),
        ]);
        assert_eq!(result.ok(), Some(BindingValue::Bool(true)));

        // contains list — not found
        let result = call_contains(vec![
            BindingValue::List(vec![
                BindingValue::Int(1),
                BindingValue::Int(2),
                BindingValue::Int(3),
            ]),
            BindingValue::Int(5),
        ]);
        assert_eq!(result.ok(), Some(BindingValue::Bool(false)));
    }

    #[test]
    fn eval_call_min_max() {
        // min(3, 5) = 3
        let result = call_min(vec![BindingValue::Int(3), BindingValue::Int(5)]);
        assert_eq!(result.ok(), Some(BindingValue::Int(3)));

        // max(3, 5) = 5
        let result = call_max(vec![BindingValue::Int(3), BindingValue::Int(5)]);
        assert_eq!(result.ok(), Some(BindingValue::Int(5)));

        // min with floats
        let result = call_min(vec![BindingValue::Float(1.5), BindingValue::Float(2.5)]);
        assert_eq!(result.ok(), Some(BindingValue::Float(1.5)));

        // max with mixed
        let result = call_max(vec![BindingValue::Int(3), BindingValue::Float(5.5)]);
        assert_eq!(result.ok(), Some(BindingValue::Float(5.5)));
    }

    #[test]
    fn eval_let_binding() {
        let e = BindingEnv::new();

        // let x = 5 in x + 1 -> 6
        let let_expr = PolicyExpr::Let {
            name: "x".to_string(),
            value: Box::new(expr_int(5)),
            body: Box::new(PolicyExpr::BinaryOp {
                op: BinOp::Add,
                left: Box::new(PolicyExpr::Path {
                    segments: vec!["x".to_string()],
                }),
                right: Box::new(expr_int(1)),
            }),
        };
        assert_eq!(eval_expr(&let_expr, &e).ok(), Some(BindingValue::Int(6)));
    }

    #[test]
    fn eval_unary_not_neg() {
        let e = BindingEnv::new();

        // not true -> false
        let not_true = PolicyExpr::UnaryOp {
            op: UnOp::Not,
            operand: Box::new(expr_bool(true)),
        };
        assert_eq!(
            eval_expr(&not_true, &e).ok(),
            Some(BindingValue::Bool(false))
        );

        // not false -> true
        let not_false = PolicyExpr::UnaryOp {
            op: UnOp::Not,
            operand: Box::new(expr_bool(false)),
        };
        assert_eq!(
            eval_expr(&not_false, &e).ok(),
            Some(BindingValue::Bool(true))
        );

        // -5 -> -5
        let neg = PolicyExpr::UnaryOp {
            op: UnOp::Neg,
            operand: Box::new(expr_int(5)),
        };
        assert_eq!(eval_expr(&neg, &e).ok(), Some(BindingValue::Int(-5)));

        // -3.14 -> -3.14
        let neg_f = PolicyExpr::UnaryOp {
            op: UnOp::Neg,
            operand: Box::new(expr_float(3.14)),
        };
        assert_eq!(eval_expr(&neg_f, &e).ok(), Some(BindingValue::Float(-3.14)));
    }

    #[test]
    fn eval_error_type_mismatch() {
        let e = BindingEnv::new();

        // "a" + 1 -> error
        let bad_add = PolicyExpr::BinaryOp {
            op: BinOp::Add,
            left: Box::new(expr_str("a")),
            right: Box::new(expr_int(1)),
        };
        let result = eval_expr(&bad_add, &e);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("type error"));

        // true and 1 -> error
        let bad_and = PolicyExpr::BinaryOp {
            op: BinOp::And,
            left: Box::new(expr_bool(true)),
            right: Box::new(expr_int(1)),
        };
        let result = eval_expr(&bad_and, &e);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("type error"));

        // not 5 -> error
        let bad_not = PolicyExpr::UnaryOp {
            op: UnOp::Not,
            operand: Box::new(expr_int(5)),
        };
        let result = eval_expr(&bad_not, &e);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("type error"));
    }

    #[test]
    fn eval_error_division_by_zero() {
        let e = BindingEnv::new();

        // 10 / 0 -> error
        let div_zero = PolicyExpr::BinaryOp {
            op: BinOp::Div,
            left: Box::new(expr_int(10)),
            right: Box::new(expr_int(0)),
        };
        let result = eval_expr(&div_zero, &e);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("division by zero"));

        // 10.0 / 0.0 -> error
        let div_zero_f = PolicyExpr::BinaryOp {
            op: BinOp::Div,
            left: Box::new(expr_float(10.0)),
            right: Box::new(expr_float(0.0)),
        };
        let result = eval_expr(&div_zero_f, &e);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("division by zero"));
    }

    #[test]
    fn eval_nested_let() {
        let e = BindingEnv::new();

        // let x = 1 in let y = 2 in x + y -> 3
        let nested = PolicyExpr::Let {
            name: "x".to_string(),
            value: Box::new(expr_int(1)),
            body: Box::new(PolicyExpr::Let {
                name: "y".to_string(),
                value: Box::new(expr_int(2)),
                body: Box::new(PolicyExpr::BinaryOp {
                    op: BinOp::Add,
                    left: Box::new(PolicyExpr::Path {
                        segments: vec!["x".to_string()],
                    }),
                    right: Box::new(PolicyExpr::Path {
                        segments: vec!["y".to_string()],
                    }),
                }),
            }),
        };
        assert_eq!(eval_expr(&nested, &e).ok(), Some(BindingValue::Int(3)));
    }

    #[test]
    fn eval_call_exists_forall() {
        // exists([1,2,3]) -> true
        let result = call_exists(vec![BindingValue::List(vec![
            BindingValue::Int(1),
            BindingValue::Int(2),
        ])]);
        assert_eq!(result.ok(), Some(BindingValue::Bool(true)));

        // exists([]) -> false
        let result = call_exists(vec![BindingValue::List(vec![])]);
        assert_eq!(result.ok(), Some(BindingValue::Bool(false)));

        // forall([true, true, true]) -> true
        let result = call_forall(vec![BindingValue::List(vec![
            BindingValue::Bool(true),
            BindingValue::Bool(true),
            BindingValue::Bool(true),
        ])]);
        assert_eq!(result.ok(), Some(BindingValue::Bool(true)));

        // forall([true, false, true]) -> false
        let result = call_forall(vec![BindingValue::List(vec![
            BindingValue::Bool(true),
            BindingValue::Bool(false),
            BindingValue::Bool(true),
        ])]);
        assert_eq!(result.ok(), Some(BindingValue::Bool(false)));
    }

    #[test]
    fn eval_call_now_turn_phase() {
        // now() returns an int
        let result = call_now(vec![]);
        assert!(result.is_ok());
        match result.ok() {
            Some(BindingValue::Int(ts)) => assert!(ts > 0),
            _ => panic!("now() should return Int"),
        }

        // turn() returns 0 (placeholder)
        let result = call_turn(vec![]);
        assert_eq!(result.ok(), Some(BindingValue::Int(0)));

        // phase() returns Absent (placeholder)
        let result = call_phase(vec![]);
        assert_eq!(result.ok(), Some(BindingValue::Absent));
    }

    #[test]
    fn eval_call_regex_match() {
        // Plain substring match
        let result = call_regex_match(vec![
            BindingValue::Str("world".to_string()),
            BindingValue::Str("hello world".to_string()),
        ]);
        assert_eq!(result.ok(), Some(BindingValue::Bool(true)));

        // Not found
        let result = call_regex_match(vec![
            BindingValue::Str("xyz".to_string()),
            BindingValue::Str("hello world".to_string()),
        ]);
        assert_eq!(result.ok(), Some(BindingValue::Bool(false)));

        // Glob pattern: "he*ld" matches "hello world"
        let result = call_regex_match(vec![
            BindingValue::Str("he*ld".to_string()),
            BindingValue::Str("hello world".to_string()),
        ]);
        assert_eq!(result.ok(), Some(BindingValue::Bool(true)));
    }

    #[test]
    fn eval_call_path_eq_labels() {
        // path_eq(1, 1) -> true
        let result = call_path_eq(vec![BindingValue::Int(1), BindingValue::Int(1)]);
        assert_eq!(result.ok(), Some(BindingValue::Bool(true)));

        // path_eq("a", "b") -> false
        let result = call_path_eq(vec![
            BindingValue::Str("a".to_string()),
            BindingValue::Str("b".to_string()),
        ]);
        assert_eq!(result.ok(), Some(BindingValue::Bool(false)));

        // labels("admin") -> false (placeholder)
        let result = call_labels(vec![BindingValue::Str("admin".to_string())]);
        assert_eq!(result.ok(), Some(BindingValue::Bool(false)));
    }

    #[test]
    fn eval_unknown_function_error() {
        let e = BindingEnv::new();
        let bad_call = PolicyExpr::Call {
            func: "nonexistent".to_string(),
            args: vec![],
        };
        let result = eval_expr(&bad_call, &e);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("unknown function"));
    }

    #[test]
    fn eval_wrong_arity_error() {
        let result = call_sum(vec![BindingValue::Int(1), BindingValue::Int(2)]);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("takes 1 argument"));
    }
}
