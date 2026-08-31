use mcl_spec::expression::{CmpOp, Condition, FieldPath};
use mcl_spec::value::MclValue;
use mcl_spec::{MclError, Span};

/// Parse a condition expression from a string.
/// Supports:
///   - `field.path == value`
///   - `field.path != value`
///   - `field.path > value` (and <, >=, <=)
///   - `field.path` (truthy)
///   - `!field.path` (falsy)
pub fn parse_condition(input: &str, span: Span) -> Result<Condition, MclError> {
    let trimmed = input.trim();

    if trimmed.is_empty() {
        return Err(MclError::parse("empty condition", span));
    }

    // Falsy check: !field.path
    if let Some(rest) = trimmed.strip_prefix('!') {
        let path = parse_field_path(rest.trim())?;
        return Ok(Condition::Falsy(path));
    }

    // Try to find a comparison operator
    if let Some((left, op, right)) = split_comparison(trimmed) {
        let path = parse_field_path(left.trim())?;
        let value = parse_condition_value(right.trim());
        return Ok(Condition::Compare {
            left: path,
            op,
            right: value,
        });
    }

    // Truthy check: field.path
    let path = parse_field_path(trimmed)?;
    Ok(Condition::Truthy(path))
}

/// Parse a list of conditions from a string list like `[cond1, cond2]`
/// Returns Condition::All if multiple, single condition if one
pub fn parse_condition_list(items: &[String], span: Span) -> Result<Vec<Condition>, MclError> {
    items.iter().map(|s| parse_condition(s, span)).collect()
}

pub fn parse_field_path(s: &str) -> Result<FieldPath, MclError> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return Err(MclError::parse("empty field path", Span::empty()));
    }
    let segments: Vec<String> = trimmed.split('.').map(|s| s.trim().to_string()).collect();
    for seg in &segments {
        if seg.is_empty() {
            return Err(MclError::parse(
                format!("invalid field path: `{trimmed}`"),
                Span::empty(),
            ));
        }
    }
    Ok(FieldPath::new(segments))
}

fn split_comparison(s: &str) -> Option<(&str, CmpOp, &str)> {
    // Check two-char operators first
    for (pattern, op) in &[
        ("==", CmpOp::Eq),
        ("!=", CmpOp::Ne),
        (">=", CmpOp::Ge),
        ("<=", CmpOp::Le),
    ] {
        if let Some(pos) = s.find(pattern) {
            let left = &s[..pos];
            let right = &s[pos + 2..];
            return Some((left, *op, right));
        }
    }
    // Single-char operators (check after two-char to avoid false matches)
    for (pattern, op) in &[('>', CmpOp::Gt), ('<', CmpOp::Lt)] {
        if let Some(pos) = s.find(*pattern) {
            // Make sure it's not part of >= or <=
            if pos + 1 < s.len() && &s[pos + 1..pos + 2] == "=" {
                continue;
            }
            let left = &s[..pos];
            let right = &s[pos + 1..];
            return Some((left, *op, right));
        }
    }
    None
}

fn parse_condition_value(s: &str) -> MclValue {
    let s = s.trim();
    if s == "true" {
        MclValue::Bool(true)
    } else if s == "false" {
        MclValue::Bool(false)
    } else if s == "null" {
        MclValue::Null
    } else if let Ok(n) = s.parse::<i64>() {
        MclValue::Int(n)
    } else if let Ok(n) = s.parse::<f64>() {
        MclValue::Float(n)
    } else if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
        MclValue::String(s[1..s.len() - 1].to_string())
    } else {
        // Bare identifier treated as string value
        MclValue::String(s.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s() -> Span {
        Span::empty()
    }

    #[test]
    fn parse_compare_eq() {
        let cond = parse_condition("door_north == unlocked", s()).unwrap();
        assert!(matches!(cond, Condition::Compare {
            ref left, op: CmpOp::Eq, ref right
        } if left.to_string() == "door_north" && *right == MclValue::String("unlocked".into())));
    }

    #[test]
    fn parse_compare_gt() {
        let cond = parse_condition("player.hp > 0", s()).unwrap();
        assert!(matches!(cond, Condition::Compare {
            op: CmpOp::Gt, ref right, ..
        } if *right == MclValue::Int(0)));
    }

    #[test]
    fn parse_truthy() {
        let cond = parse_condition("inventory.torch", s()).unwrap();
        assert!(matches!(cond, Condition::Truthy(ref p) if p.to_string() == "inventory.torch"));
    }

    #[test]
    fn parse_falsy() {
        let cond = parse_condition("!lit", s()).unwrap();
        assert!(matches!(cond, Condition::Falsy(ref p) if p.to_string() == "lit"));
    }
}
