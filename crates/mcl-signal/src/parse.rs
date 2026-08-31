use indexmap::IndexMap;
use mcl_spec::label::LabelSet;
use mcl_spec::signal::{MclSignal, SignalKind};
use mcl_spec::{MclError, MclValue, Span};

/// Parse a single MCL signal line like `MCL::action(attack, target=skeleton_guard)`
pub fn parse_signal(input: &str, line_num: u32) -> Result<MclSignal, MclError> {
    let trimmed = input.trim();
    let span = Span::new(0, trimmed.len(), line_num, 1);

    let rest = trimmed
        .strip_prefix("MCL::")
        .ok_or_else(|| MclError::signal("expected `MCL::` prefix", 0))?;

    // Find the signal name and args
    let paren_start = rest
        .find('(')
        .ok_or_else(|| MclError::signal("expected `(` after signal name", 5))?;

    let name = &rest[..paren_start];

    // Find matching closing paren
    let paren_end = find_closing_paren(rest, paren_start)
        .ok_or_else(|| MclError::signal("unmatched `(`", paren_start + 5))?;

    let args_str = &rest[paren_start + 1..paren_end];

    let kind = match name {
        "action" => parse_action_signal(args_str)?,
        "mutate" => parse_mutate_signal(args_str)?,
        "transition" => parse_transition_signal(args_str)?,
        "dialogue" => parse_dialogue_signal(args_str)?,
        "narrate" => parse_narrate_signal(args_str)?,
        "query" => parse_query_signal(args_str)?,
        "error" => parse_error_signal(args_str)?,
        "noop" => SignalKind::Noop,
        _ => return Err(MclError::signal(format!("unknown signal type `{name}`"), 5)),
    };

    Ok(MclSignal {
        kind,
        span,
        labels: LabelSet::default(),
    })
}

fn parse_action_signal(args: &str) -> Result<SignalKind, MclError> {
    let parts = split_signal_args(args);
    if parts.is_empty() {
        return Err(MclError::signal("action signal requires a name", 0));
    }

    let name = parts[0].trim().to_string();
    let mut kwargs = IndexMap::new();

    for part in &parts[1..] {
        if let Some((key, val)) = split_kwarg(part) {
            kwargs.insert(key, parse_signal_value(&val));
        } else {
            // Positional arg after first becomes target
            let val = part.trim().to_string();
            if !kwargs.contains_key("target") {
                kwargs.insert("target".to_string(), MclValue::String(val));
            }
        }
    }

    Ok(SignalKind::Action { name, args: kwargs })
}

fn parse_mutate_signal(args: &str) -> Result<SignalKind, MclError> {
    let parts = split_signal_args(args);
    if parts.len() < 2 {
        return Err(MclError::signal("mutate signal requires path and value", 0));
    }
    Ok(SignalKind::Mutate {
        path: parts[0].trim().to_string(),
        value: parse_signal_value(parts[1].trim()),
    })
}

fn parse_transition_signal(args: &str) -> Result<SignalKind, MclError> {
    let context = args.trim().to_string();
    if context.is_empty() {
        return Err(MclError::signal("transition signal requires context id", 0));
    }
    Ok(SignalKind::Transition { context })
}

fn parse_dialogue_signal(args: &str) -> Result<SignalKind, MclError> {
    // First arg is entity, second is quoted text
    // MCL::dialogue(skeleton_guard, "Who dares?")
    let first_comma = find_top_level_comma(args);
    let (entity_part, text_part) = if let Some(pos) = first_comma {
        (&args[..pos], args[pos + 1..].trim())
    } else {
        return Err(MclError::signal(
            "dialogue signal requires entity and text",
            0,
        ));
    };

    let entity = entity_part.trim().to_string();
    let text = unquote(text_part);

    Ok(SignalKind::Dialogue { entity, text })
}

fn parse_narrate_signal(args: &str) -> Result<SignalKind, MclError> {
    Ok(SignalKind::Narrate {
        tag: args.trim().to_string(),
    })
}

fn parse_query_signal(args: &str) -> Result<SignalKind, MclError> {
    Ok(SignalKind::Query {
        path: args.trim().to_string(),
    })
}

fn parse_error_signal(args: &str) -> Result<SignalKind, MclError> {
    Ok(SignalKind::Error {
        message: unquote(args.trim()),
    })
}

/// Split signal arguments respecting quotes
fn split_signal_args(args: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut in_quotes = false;
    let mut paren_depth = 0;

    for (i, ch) in args.char_indices() {
        match ch {
            '"' => in_quotes = !in_quotes,
            '(' if !in_quotes => paren_depth += 1,
            ')' if !in_quotes => paren_depth -= 1,
            ',' if !in_quotes && paren_depth == 0 => {
                parts.push(&args[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    let tail = args[start..].trim();
    if !tail.is_empty() {
        parts.push(&args[start..]);
    }
    parts
}

fn split_kwarg(s: &str) -> Option<(String, String)> {
    let trimmed = s.trim();
    let eq_pos = trimmed.find('=')?;
    let key = trimmed[..eq_pos].trim().to_string();
    let val = trimmed[eq_pos + 1..].trim().to_string();
    if key.is_empty() {
        return None;
    }
    Some((key, val))
}

fn parse_signal_value(s: &str) -> MclValue {
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
        if s.contains('.') {
            return MclValue::Float(n);
        }
        MclValue::String(unquote(s))
    } else {
        MclValue::String(unquote(s))
    }
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

fn find_closing_paren(s: &str, open_pos: usize) -> Option<usize> {
    let mut depth = 0;
    let mut in_quotes = false;

    for (i, ch) in s[open_pos..].char_indices() {
        match ch {
            '"' => in_quotes = !in_quotes,
            '(' if !in_quotes => depth += 1,
            ')' if !in_quotes => {
                depth -= 1;
                if depth == 0 {
                    return Some(open_pos + i);
                }
            }
            _ => {}
        }
    }
    None
}

fn find_top_level_comma(s: &str) -> Option<usize> {
    let mut in_quotes = false;
    let mut paren_depth = 0;

    for (i, ch) in s.char_indices() {
        match ch {
            '"' => in_quotes = !in_quotes,
            '(' if !in_quotes => paren_depth += 1,
            ')' if !in_quotes => paren_depth -= 1,
            ',' if !in_quotes && paren_depth == 0 => return Some(i),
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_action_basic() {
        let sig = parse_signal("MCL::action(attack, target=skeleton_guard)", 1).unwrap();
        if let SignalKind::Action { name, args } = &sig.kind {
            assert_eq!(name, "attack");
            assert_eq!(
                args.get("target"),
                Some(&MclValue::String("skeleton_guard".into()))
            );
        } else {
            panic!("expected action signal");
        }
    }

    #[test]
    fn parse_dialogue() {
        let sig = parse_signal(r#"MCL::dialogue(skeleton_guard, "Who dares?")"#, 1).unwrap();
        if let SignalKind::Dialogue { entity, text } = &sig.kind {
            assert_eq!(entity, "skeleton_guard");
            assert_eq!(text, "Who dares?");
        } else {
            panic!("expected dialogue signal");
        }
    }

    #[test]
    fn parse_transition() {
        let sig = parse_signal("MCL::transition(crypt_inner)", 1).unwrap();
        assert!(
            matches!(&sig.kind, SignalKind::Transition { context } if context == "crypt_inner")
        );
    }

    #[test]
    fn parse_noop() {
        let sig = parse_signal("MCL::noop()", 1).unwrap();
        assert!(matches!(&sig.kind, SignalKind::Noop));
    }

    #[test]
    fn parse_mutate() {
        let sig = parse_signal("MCL::mutate(lit, true)", 1).unwrap();
        if let SignalKind::Mutate { path, value } = &sig.kind {
            assert_eq!(path, "lit");
            assert_eq!(*value, MclValue::Bool(true));
        } else {
            panic!("expected mutate signal");
        }
    }

    #[test]
    fn parse_error_signal() {
        let sig = parse_signal(r#"MCL::error("cannot reach that")"#, 1).unwrap();
        assert!(
            matches!(&sig.kind, SignalKind::Error { message } if message == "cannot reach that")
        );
    }

    #[test]
    fn unknown_signal_errors() {
        assert!(parse_signal("MCL::fly(away)", 1).is_err());
    }

    #[test]
    fn missing_prefix_errors() {
        let err = parse_signal("action(attack)", 1).unwrap_err();
        assert!(err.to_string().contains("expected `MCL::` prefix"));
    }

    #[test]
    fn missing_open_paren_errors() {
        let err = parse_signal("MCL::action_attack", 1).unwrap_err();
        assert!(err.to_string().contains("expected `(`"));
    }

    #[test]
    fn unmatched_paren_errors() {
        let err = parse_signal("MCL::action(attack, target=foo", 1).unwrap_err();
        assert!(err.to_string().contains("unmatched"));
    }

    #[test]
    fn empty_action_name_errors() {
        let err = parse_signal("MCL::action()", 1).unwrap_err();
        assert!(err.to_string().contains("requires a name"));
    }

    #[test]
    fn mutate_requires_two_args() {
        let err = parse_signal("MCL::mutate(lit)", 1).unwrap_err();
        assert!(err.to_string().contains("requires path and value"));
    }

    #[test]
    fn transition_requires_context() {
        let err = parse_signal("MCL::transition()", 1).unwrap_err();
        assert!(err.to_string().contains("requires context id"));
    }

    #[test]
    fn dialogue_requires_entity_and_text() {
        let err = parse_signal("MCL::dialogue(guard)", 1).unwrap_err();
        assert!(err.to_string().contains("requires entity and text"));
    }
}
