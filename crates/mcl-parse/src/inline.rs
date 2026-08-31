use std::collections::BTreeMap;

use indexmap::IndexMap;
use mcl_spec::label::{Label, LabelParam, LabelSet};
use mcl_spec::value::LabeledValue;
use mcl_spec::{MclError, MclValue, Span};

/// Parse an inline value string into an MclValue.
/// Handles: primitives, lists, maps, quoted strings, identifiers.
pub fn parse_value(input: &str, span: Span) -> Result<MclValue, MclError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(MclValue::Null);
    }

    let mut parser = ValueParser::new(trimmed, span);
    parser.parse_value()
}

/// Parse an inline map like `{ key: val, key2: val2 }`
pub fn parse_inline_map(input: &str, span: Span) -> Result<IndexMap<String, MclValue>, MclError> {
    let trimmed = input.trim();
    if !trimmed.starts_with('{') || !trimmed.ends_with('}') {
        return Err(MclError::parse("expected inline map `{ ... }`", span));
    }
    let inner = &trimmed[1..trimmed.len() - 1];
    parse_map_entries(inner, span)
}

/// Parse comma-separated key:value pairs
pub fn parse_map_entries(input: &str, span: Span) -> Result<IndexMap<String, MclValue>, MclError> {
    let mut map = IndexMap::new();
    if input.trim().is_empty() {
        return Ok(map);
    }

    for part in split_top_level(input, ',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some(colon_pos) = find_top_level_colon(part) {
            let key = part[..colon_pos].trim().to_string();
            let val_str = part[colon_pos + 1..].trim();
            let val = parse_value(val_str, span)?;
            map.insert(key, val);
        } else {
            return Err(MclError::parse(
                format!("expected `key: value`, got `{part}`"),
                span,
            ));
        }
    }
    Ok(map)
}

/// Parse a list like `[a, b, c]`
pub fn parse_inline_list(input: &str, span: Span) -> Result<Vec<MclValue>, MclError> {
    let trimmed = input.trim();
    if !trimmed.starts_with('[') || !trimmed.ends_with(']') {
        return Err(MclError::parse("expected inline list `[ ... ]`", span));
    }
    let inner = &trimmed[1..trimmed.len() - 1];
    let mut items = Vec::new();
    for part in split_top_level(inner, ',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        items.push(parse_value(part, span)?);
    }
    Ok(items)
}

/// Parse a list of strings from `[a, b, c]` — for requires, tags, etc.
pub fn parse_string_list(input: &str, span: Span) -> Result<Vec<String>, MclError> {
    let trimmed = input.trim();
    if !trimmed.starts_with('[') || !trimmed.ends_with(']') {
        return Err(MclError::parse("expected list `[ ... ]`", span));
    }
    let inner = &trimmed[1..trimmed.len() - 1];
    let mut items = Vec::new();
    for part in split_top_level(inner, ',') {
        let s = part.trim();
        if s.is_empty() {
            continue;
        }
        // Strip quotes if present
        if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
            items.push(s[1..s.len() - 1].to_string());
        } else {
            items.push(s.to_string());
        }
    }
    Ok(items)
}

struct ValueParser<'a> {
    input: &'a str,
    span: Span,
}

impl<'a> ValueParser<'a> {
    fn new(input: &'a str, span: Span) -> Self {
        Self { input, span }
    }

    fn parse_value(&mut self) -> Result<MclValue, MclError> {
        let s = self.input.trim();

        // Null
        if s == "null" || s == "~" {
            return Ok(MclValue::Null);
        }

        // Booleans
        if s == "true" {
            return Ok(MclValue::Bool(true));
        }
        if s == "false" {
            return Ok(MclValue::Bool(false));
        }

        // Quoted string
        if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
            return Ok(MclValue::String(s[1..s.len() - 1].to_string()));
        }

        // List
        if s.starts_with('[') && s.ends_with(']') {
            return Ok(MclValue::List(parse_inline_list(s, self.span)?));
        }

        // Map
        if s.starts_with('{') && s.ends_with('}') {
            return Ok(MclValue::Map(parse_inline_map(s, self.span)?));
        }

        // Integer
        if let Ok(n) = s.parse::<i64>() {
            return Ok(MclValue::Int(n));
        }

        // Float
        if let Ok(n) = s.parse::<f64>() {
            if s.contains('.') {
                return Ok(MclValue::Float(n));
            }
        }

        // Bare identifier/string
        Ok(MclValue::String(s.to_string()))
    }
}

/// Split a string at top-level occurrences of `sep`, respecting quotes, braces, brackets
fn split_top_level(input: &str, sep: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut in_quotes = false;
    let mut brace_depth = 0i32;
    let mut bracket_depth = 0i32;

    for (i, ch) in input.char_indices() {
        match ch {
            '"' => in_quotes = !in_quotes,
            '{' if !in_quotes => brace_depth += 1,
            '}' if !in_quotes => brace_depth -= 1,
            '[' if !in_quotes => bracket_depth += 1,
            ']' if !in_quotes => bracket_depth -= 1,
            c if c == sep && !in_quotes && brace_depth == 0 && bracket_depth == 0 => {
                parts.push(&input[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&input[start..]);
    parts
}

/// Parse a value with optional `@label` postfix(es).
/// `"value" @phi` or `50000 @secret(audience=buyer)` → `LabeledValue`.
/// No `@` → `LabeledValue` with empty `LabelSet`.
pub fn parse_labeled_value(input: &str, span: Span) -> Result<LabeledValue, MclError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(LabeledValue {
            value: MclValue::Null,
            labels: LabelSet::empty(),
        });
    }

    let at_pos = find_top_level_at(trimmed);
    if let Some(pos) = at_pos {
        let value_str = trimmed[..pos].trim();
        let label_str = &trimmed[pos + 1..];
        let value = parse_value(value_str, span)?;
        let labels = parse_label_set(label_str)?;
        Ok(LabeledValue { value, labels })
    } else {
        let value = parse_value(trimmed, span)?;
        Ok(LabeledValue {
            value,
            labels: LabelSet::empty(),
        })
    }
}

fn parse_label_set(input: &str) -> Result<LabelSet, MclError> {
    let mut labels = LabelSet::empty();
    let parts = split_label_postfix(input.trim());
    for part in parts {
        let label = parse_single_label(part.trim())?;
        labels.insert(label);
    }
    Ok(labels)
}

fn split_label_postfix(input: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut in_quotes = false;
    let mut paren_depth = 0i32;

    let chars = input.char_indices().peekable();
    for (i, ch) in chars {
        match ch {
            '"' => in_quotes = !in_quotes,
            '(' if !in_quotes => paren_depth += 1,
            ')' if !in_quotes => paren_depth -= 1,
            '@' if !in_quotes && paren_depth == 0 && i > 0 => {
                let before = &input[..i];
                if before.ends_with(' ') || before.ends_with('\t') {
                    parts.push(&input[start..i]);
                    start = i;
                }
            }
            _ => {}
        }
    }
    parts.push(&input[start..]);
    parts
}

fn parse_single_label(input: &str) -> Result<Label, MclError> {
    let trimmed = input.trim().strip_prefix('@').unwrap_or(input.trim());

    if let Some(paren_pos) = trimmed.find('(') {
        if !trimmed.ends_with(')') {
            return Err(MclError::parse(
                format!("unterminated label params: `{trimmed}`"),
                Span::empty(),
            ));
        }
        let name = trimmed[..paren_pos].trim().to_string();
        if name.is_empty() {
            return Err(MclError::parse("empty label name", Span::empty()));
        }
        let params_str = &trimmed[paren_pos + 1..trimmed.len() - 1];
        let params = parse_label_params(params_str)?;
        Ok(Label { name, params })
    } else {
        let name = trimmed.trim().to_string();
        if name.is_empty() {
            return Err(MclError::parse("empty label name", Span::empty()));
        }
        Ok(Label {
            name,
            params: BTreeMap::new(),
        })
    }
}

fn parse_label_params(input: &str) -> Result<BTreeMap<String, LabelParam>, MclError> {
    let mut params = BTreeMap::new();
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(params);
    }

    for part in split_top_level(trimmed, ',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some(eq_pos) = part.find('=') {
            let key = part[..eq_pos].trim().to_string();
            let val_str = part[eq_pos + 1..].trim();
            let param = parse_label_param_value(val_str)?;
            params.insert(key, param);
        } else {
            params.insert(part.to_string(), LabelParam::Ident(part.to_string()));
        }
    }
    Ok(params)
}

fn parse_label_param_value(s: &str) -> Result<LabelParam, MclError> {
    let s = s.trim();
    if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
        Ok(LabelParam::Str(s[1..s.len() - 1].to_string()))
    } else if let Ok(n) = s.parse::<i64>() {
        Ok(LabelParam::Int(n))
    } else {
        Ok(LabelParam::Ident(s.to_string()))
    }
}

/// Find the first top-level `@` preceded by whitespace in a string.
fn find_top_level_at(input: &str) -> Option<usize> {
    let mut in_quotes = false;
    let mut brace_depth = 0i32;
    let mut bracket_depth = 0i32;
    let mut paren_depth = 0i32;

    for (i, ch) in input.char_indices() {
        match ch {
            '"' => in_quotes = !in_quotes,
            '{' if !in_quotes => brace_depth += 1,
            '}' if !in_quotes => brace_depth -= 1,
            '[' if !in_quotes => bracket_depth += 1,
            ']' if !in_quotes => bracket_depth -= 1,
            '(' if !in_quotes => paren_depth += 1,
            ')' if !in_quotes => paren_depth -= 1,
            '@' if !in_quotes
                && brace_depth == 0
                && bracket_depth == 0
                && paren_depth == 0
                && (i == 0 || input[..i].ends_with(' ') || input[..i].ends_with('\t')) =>
            {
                return Some(i);
            }
            _ => {}
        }
    }
    None
}

/// Find the position of the first top-level `:` in a string
fn find_top_level_colon(s: &str) -> Option<usize> {
    let mut in_quotes = false;
    let mut brace_depth = 0i32;
    let mut bracket_depth = 0i32;

    for (i, ch) in s.char_indices() {
        match ch {
            '"' => in_quotes = !in_quotes,
            '{' if !in_quotes => brace_depth += 1,
            '}' if !in_quotes => brace_depth -= 1,
            '[' if !in_quotes => bracket_depth += 1,
            ']' if !in_quotes => bracket_depth -= 1,
            ':' if !in_quotes && brace_depth == 0 && bracket_depth == 0 => return Some(i),
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s() -> Span {
        Span::empty()
    }

    #[test]
    fn parse_primitives() {
        assert_eq!(parse_value("true", s()).unwrap(), MclValue::Bool(true));
        assert_eq!(parse_value("42", s()).unwrap(), MclValue::Int(42));
        assert_eq!(parse_value("3.14", s()).unwrap(), MclValue::Float(3.14));
        assert_eq!(parse_value("null", s()).unwrap(), MclValue::Null);
        assert_eq!(
            parse_value("\"hello\"", s()).unwrap(),
            MclValue::String("hello".into())
        );
        assert_eq!(
            parse_value("bare_ident", s()).unwrap(),
            MclValue::String("bare_ident".into())
        );
    }

    #[test]
    fn parse_list() {
        let val = parse_value("[1, 2, 3]", s()).unwrap();
        assert_eq!(
            val,
            MclValue::List(vec![MclValue::Int(1), MclValue::Int(2), MclValue::Int(3)])
        );
    }

    #[test]
    fn parse_map() {
        let val = parse_value("{ on: floor, visible: true }", s()).unwrap();
        if let MclValue::Map(m) = val {
            assert_eq!(m.get("on"), Some(&MclValue::String("floor".into())));
            assert_eq!(m.get("visible"), Some(&MclValue::Bool(true)));
        } else {
            panic!("expected map");
        }
    }

    #[test]
    fn parse_nested() {
        let val = parse_value("{ items: [a, b], count: 3 }", s()).unwrap();
        if let MclValue::Map(m) = val {
            assert!(matches!(m.get("items"), Some(MclValue::List(_))));
            assert_eq!(m.get("count"), Some(&MclValue::Int(3)));
        } else {
            panic!("expected map");
        }
    }

    #[test]
    fn parse_label_postfix_basic() {
        let lv = parse_labeled_value("\"value\" @phi", s()).unwrap();
        assert_eq!(lv.value, MclValue::String("value".into()));
        assert!(lv.labels.contains_name("phi"));
    }

    #[test]
    fn parse_label_postfix_parameterized() {
        let lv = parse_labeled_value("50000 @secret(audience=buyer)", s()).unwrap();
        assert_eq!(lv.value, MclValue::Int(50000));
        assert!(lv.labels.contains_name("secret"));
        let labels: Vec<_> = lv.labels.0.iter().collect();
        assert_eq!(labels[0].name, "secret");
        assert_eq!(
            labels[0].params.get("audience"),
            Some(&LabelParam::Ident("buyer".into()))
        );
    }

    #[test]
    fn parse_label_on_list() {
        let lv = parse_labeled_value("[a, b] @secret(audience=buyer)", s()).unwrap();
        assert!(matches!(lv.value, MclValue::List(_)));
        assert!(lv.labels.contains_name("secret"));
    }

    #[test]
    fn parse_labeled_value_no_label() {
        let lv = parse_labeled_value("42", s()).unwrap();
        assert_eq!(lv.value, MclValue::Int(42));
        assert!(lv.labels.is_empty());
    }

    #[test]
    fn parse_labeled_value_multiple_labels() {
        let lv = parse_labeled_value("data @phi @pii", s()).unwrap();
        assert_eq!(lv.value, MclValue::String("data".into()));
        assert!(lv.labels.contains_name("phi"));
        assert!(lv.labels.contains_name("pii"));
    }

    #[test]
    fn parse_label_with_string_param() {
        let lv = parse_labeled_value("x @tag(name=\"hello\")", s()).unwrap();
        assert!(lv.labels.contains_name("tag"));
        let labels: Vec<_> = lv.labels.0.iter().collect();
        assert_eq!(
            labels[0].params.get("name"),
            Some(&LabelParam::Str("hello".into()))
        );
    }

    #[test]
    fn parse_label_with_int_param() {
        let lv = parse_labeled_value("x @level(max=100)", s()).unwrap();
        let labels: Vec<_> = lv.labels.0.iter().collect();
        assert_eq!(labels[0].params.get("max"), Some(&LabelParam::Int(100)));
    }
}
