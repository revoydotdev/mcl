//! Policy block parser (R2 §1.1-1.2).
//!
//! Parses `policy name { ... }` blocks from the `# Policies` section into
//! `PolicyDef` structs.

use indexmap::IndexMap;
use mcl_spec::policy::{Event, PolicyDef, PolicyPriority, PolicyViolationAction, TransformExpr};
use mcl_spec::{MclError, Span};

use crate::expr::parse_expression;
use crate::lexer::{Line, LineKind};

/// Parse all policy blocks from the `# Policies` section lines.
pub fn parse_policies(lines: &[Line]) -> Result<IndexMap<String, PolicyDef>, MclError> {
    let mut policies = IndexMap::new();
    let mut i = 0;

    while i < lines.len() {
        // Look for the start of a policy block: "policy <name> {"
        if let Some(policy_line) = try_find_policy_header(&lines[i]) {
            let span = lines[i].span;
            let (name, _) = &policy_line;
            let name = name.clone();

            i += 1;

            // Collect all lines until the closing '}'
            let mut fields: Vec<(String, String, Span)> = Vec::new();
            let mut multiline_buf = String::new();
            let mut multiline_key = String::new();
            let mut multiline_span = Span::empty();

            while i < lines.len() {
                let content = match &lines[i].kind {
                    LineKind::Indented(_, c) => c.clone(),
                    LineKind::Blank | LineKind::Comment(_) => {
                        i += 1;
                        continue;
                    }
                    // A closing brace at indent 0 ends the block
                    _ => {
                        let raw = lines[i].raw.trim();
                        if raw == "}" {
                            // flush any pending multiline
                            if !multiline_buf.is_empty() {
                                fields.push((
                                    multiline_key.clone(),
                                    multiline_buf.trim().to_string(),
                                    multiline_span,
                                ));
                                multiline_buf.clear();
                                multiline_key.clear();
                            }
                            i += 1;
                            break;
                        }
                        // Could be a non-indented field (e.g. policy header itself)
                        let raw = lines[i].raw.trim();
                        if raw == "}" {
                            if !multiline_buf.is_empty() {
                                fields.push((
                                    multiline_key.clone(),
                                    multiline_buf.trim().to_string(),
                                    multiline_span,
                                ));
                                multiline_buf.clear();
                                multiline_key.clear();
                            }
                            i += 1;
                            break;
                        }
                        raw.to_string()
                    }
                };

                // Check for closing brace
                if content.trim() == "}" {
                    if !multiline_buf.is_empty() {
                        fields.push((
                            multiline_key.clone(),
                            multiline_buf.trim().to_string(),
                            multiline_span,
                        ));
                        multiline_buf.clear();
                        multiline_key.clear();
                    }
                    i += 1;
                    break;
                }

                // Try to parse as key: value
                if let Some((key, val)) = split_kv(&content) {
                    // Flush previous multiline if any
                    if !multiline_buf.is_empty() && !multiline_key.is_empty() {
                        fields.push((
                            multiline_key.clone(),
                            multiline_buf.trim().to_string(),
                            multiline_span,
                        ));
                        multiline_buf.clear();
                        multiline_key.clear();
                    }

                    // Some values span multiple lines (require: with continuation)
                    // Check if this is a field that could have continuations
                    if is_multiline_field(&key) {
                        multiline_key = key;
                        multiline_buf = val;
                        multiline_span = lines[i].span;
                    } else {
                        fields.push((key, val, lines[i].span));
                    }
                } else {
                    // Continuation of previous multiline field
                    if !multiline_key.is_empty() {
                        multiline_buf.push(' ');
                        multiline_buf.push_str(content.trim());
                    }
                    // Otherwise ignore (bare content, shouldn't happen in valid MCL)
                }

                i += 1;
            }

            // Flush remaining multiline
            if !multiline_buf.is_empty() && !multiline_key.is_empty() {
                fields.push((
                    multiline_key.clone(),
                    multiline_buf.trim().to_string(),
                    multiline_span,
                ));
            }

            let policy = build_policy_def(&name, &fields, span)?;
            policies.insert(name, policy);
        } else {
            i += 1;
        }
    }

    Ok(policies)
}

/// Check if the line is a policy header: `policy <name> {`
fn try_find_policy_header(line: &Line) -> Option<(String, String)> {
    let content = match &line.kind {
        LineKind::Indented(_, c) => c.as_str(),
        LineKind::KeyValue(_, _) | LineKind::ListItem(_, _) => return None,
        _ => {
            // Use raw for unclassified lines at indent 0
            line.raw.trim()
        }
    };

    let trimmed = content.trim();
    if !trimmed.starts_with("policy ") {
        return None;
    }

    let after_policy = &trimmed[7..]; // "policy ".len()
    if !after_policy.ends_with('{') {
        return None;
    }

    // Strip the trailing '{' and any whitespace before it
    let before_brace = after_policy[..after_policy.len() - 1].trim();

    if before_brace.is_empty() {
        return None;
    }

    Some((before_brace.to_string(), trimmed.to_string()))
}

/// Check if a field can span multiple lines (continuation lines).
fn is_multiline_field(key: &str) -> bool {
    matches!(key, "require" | "match")
}

/// Build a `PolicyDef` from collected key-value fields.
fn build_policy_def(
    name: &str,
    fields: &[(String, String, Span)],
    span: Span,
) -> Result<PolicyDef, MclError> {
    let mut on = None;
    let mut match_expr = None;
    let mut require_exprs = Vec::new();
    let mut transform = None;
    let mut on_violation = None;
    let mut priority = None;

    for (key, val, field_span) in fields {
        match key.as_str() {
            "on" => {
                on = Some(parse_event(val, *field_span)?);
            }
            "match" => {
                match_expr = Some(parse_expression(val)?);
            }
            "require" => {
                require_exprs.push(parse_expression(val)?);
            }
            "transform" => {
                transform = Some(parse_transform(val, *field_span)?);
            }
            "on_violation" => {
                on_violation = Some(parse_violation_action(val, *field_span)?);
            }
            "priority" => {
                priority = Some(parse_priority(val, *field_span)?);
            }
            _ => {
                log::warn!("unknown policy field `{key}`, ignoring");
            }
        }
    }

    let on =
        on.ok_or_else(|| MclError::parse(format!("policy `{name}` missing `on` field"), span))?;
    let on_violation = on_violation.ok_or_else(|| {
        MclError::parse(
            format!("policy `{name}` missing `on_violation` field"),
            span,
        )
    })?;

    Ok(PolicyDef {
        name: name.to_string(),
        on,
        match_expr,
        require_exprs,
        transform,
        on_violation,
        priority,
    })
}

/// Parse an event string into the Event enum.
fn parse_event(s: &str, span: Span) -> Result<Event, MclError> {
    match s.trim() {
        "signal_admit" => Ok(Event::SignalAdmit),
        "signal_dispatch" => Ok(Event::SignalDispatch),
        "narration_emit" => Ok(Event::NarrationEmit),
        "state_mutate" => Ok(Event::StateMutate),
        "audit_export" => Ok(Event::AuditExport),
        "phase_transition" => Ok(Event::PhaseTransition),
        "session_start" => Ok(Event::SessionStart),
        "session_end" => Ok(Event::SessionEnd),
        other => Err(MclError::parse(format!("unknown event `{other}`"), span)),
    }
}

/// Parse a violation action string.
///
/// Handles both simple forms (`deny`, `log_only`) and parameterized forms
/// (`reject(reason="...")`, `route_to(channel="...")`, `demand_confirmation(prompt="...")`).
fn parse_violation_action(s: &str, span: Span) -> Result<PolicyViolationAction, MclError> {
    let s = s.trim();

    // Check for parameterized forms: `name(args)`
    if let Some(paren_pos) = s.find('(') {
        if !s.ends_with(')') {
            return Err(MclError::parse(
                format!("unclosed parentheses in on_violation: `{s}`"),
                span,
            ));
        }
        let name = &s[..paren_pos];
        let args_str = &s[paren_pos + 1..s.len() - 1];
        let kv = parse_kv_args(args_str);

        match name.trim() {
            "reject" => {
                let reason = kv
                    .get("reason")
                    .cloned()
                    .ok_or_else(|| MclError::parse("reject() requires reason= argument", span))?;
                Ok(PolicyViolationAction::Reject { reason })
            }
            "route_to" => {
                let channel = kv.get("channel").cloned().ok_or_else(|| {
                    MclError::parse("route_to() requires channel= argument", span)
                })?;
                Ok(PolicyViolationAction::RouteTo { channel })
            }
            "demand_confirmation" => {
                let prompt = kv.get("prompt").cloned();
                Ok(PolicyViolationAction::DemandConfirmation { prompt })
            }
            other => Err(MclError::parse(
                format!("unknown parameterized on_violation: `{other}`"),
                span,
            )),
        }
    } else {
        // Simple forms
        match s {
            "deny" => Ok(PolicyViolationAction::Deny),
            "redact_and_warn" => Ok(PolicyViolationAction::RedactAndWarn),
            "rewrite" => Ok(PolicyViolationAction::Rewrite),
            "log_only" => Ok(PolicyViolationAction::LogOnly),
            "audit_only" => Ok(PolicyViolationAction::AuditOnly),
            other => Err(MclError::parse(
                format!("unknown on_violation action: `{other}`"),
                span,
            )),
        }
    }
}

/// Parse key=value arguments from inside parentheses.
/// E.g., `reason="daily_limit_exceeded"` → {"reason": "daily_limit_exceeded"}
fn parse_kv_args(input: &str) -> IndexMap<String, String> {
    let mut map = IndexMap::new();
    let mut pos = 0;
    let bytes = input.as_bytes();

    while pos < input.len() {
        // Skip whitespace
        while pos < input.len() && bytes[pos] == b' ' {
            pos += 1;
        }
        if pos >= input.len() {
            break;
        }

        // Read key (up to '=')
        let key_start = pos;
        while pos < input.len() && bytes[pos] != b'=' {
            pos += 1;
        }
        if pos >= input.len() {
            break;
        }
        let key = input[key_start..pos].trim().to_string();
        pos += 1; // skip '='

        // Read value (quoted string or bare)
        while pos < input.len() && bytes[pos] == b' ' {
            pos += 1;
        }
        if pos >= input.len() {
            break;
        }

        let value = if bytes[pos] == b'"' {
            // Quoted string
            pos += 1;
            let val_start = pos;
            while pos < input.len() && bytes[pos] != b'"' {
                pos += 1;
            }
            let val = input[val_start..pos].to_string();
            pos += 1; // skip closing quote
            val
        } else {
            // Bare value (up to comma or end)
            let val_start = pos;
            while pos < input.len() && bytes[pos] != b',' {
                pos += 1;
            }
            input[val_start..pos].trim().to_string()
        };

        map.insert(key, value);

        // Skip comma
        while pos < input.len() && bytes[pos] == b' ' {
            pos += 1;
        }
        if pos < input.len() && bytes[pos] == b',' {
            pos += 1;
        }
    }

    map
}

/// Parse a transform expression.
///
/// `drop_args([name, ssn])` → `TransformExpr::DropArgs`
/// `redact_fields([name])` → `TransformExpr::RedactFields`
fn parse_transform(s: &str, span: Span) -> Result<TransformExpr, MclError> {
    let s = s.trim();

    if let Some(paren_pos) = s.find('(') {
        if !s.ends_with(')') {
            return Err(MclError::parse(
                format!("unclosed parentheses in transform: `{s}`"),
                span,
            ));
        }
        let name = &s[..paren_pos];
        let args_str = &s[paren_pos + 1..s.len() - 1];

        // Parse field list: [name, ssn]
        let fields = parse_field_list(args_str);

        match name.trim() {
            "drop_args" => Ok(TransformExpr::DropArgs { fields }),
            "redact_fields" => Ok(TransformExpr::RedactFields { fields }),
            other => Err(MclError::parse(
                format!("unknown transform: `{other}`"),
                span,
            )),
        }
    } else {
        Err(MclError::parse(
            format!("transform requires parentheses: `{s}`"),
            span,
        ))
    }
}

/// Parse a bracket-delimited field list: `[name, ssn]` → `vec!["name", "ssn"]`
fn parse_field_list(input: &str) -> Vec<String> {
    let s = input.trim();
    if !s.starts_with('[') || !s.ends_with(']') {
        return Vec::new();
    }
    let inner = &s[1..s.len() - 1];
    if inner.trim().is_empty() {
        return Vec::new();
    }
    inner
        .split(',')
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect()
}

/// Parse priority value.
fn parse_priority(s: &str, _span: Span) -> Result<PolicyPriority, MclError> {
    match s.trim() {
        "high" => Ok(PolicyPriority::High),
        "default" => Ok(PolicyPriority::Default),
        other => Err(MclError::parse(
            format!("unknown priority `{other}`, expected `high` or `default`"),
            Span::empty(),
        )),
    }
}

/// Split a line into (key, value) on the first `:`.
/// Reused locally — same logic as blocks.rs `split_kv`.
fn split_kv(s: &str) -> Option<(String, String)> {
    let pos = s.find(':')?;
    let key = s[..pos].trim().to_string();
    let val = s[pos + 1..].trim().to_string();
    if key.is_empty() {
        return None;
    }
    Some((key, val))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::lex;
    use mcl_spec::policy::{Event, PolicyPriority, PolicyViolationAction, TransformExpr};

    /// Helper: lex + extract section lines for the Policies section.
    fn lex_policies(input: &str) -> Vec<Line> {
        let lines = lex(input);
        let mut start = None;
        let mut end = lines.len();
        for (i, line) in lines.iter().enumerate() {
            if let LineKind::SectionHeader(name) = &line.kind {
                if name == "Policies" {
                    start = Some(i + 1);
                } else if start.is_some() {
                    end = i;
                    break;
                }
            }
        }
        if let Some(s) = start {
            lines[s..end].to_vec()
        } else {
            // No section header — return all lines (for unit tests)
            lines
        }
    }

    #[test]
    fn parse_policy_basic_roundtrip() {
        let input = r#"
policy max_spend_per_turn {
  on:           signal_admit
  match:        action.kind == "transfer"
  require:      context.daily_limit > 0
  on_violation: deny
}
"#;
        let lines = lex_policies(input);
        let policies = parse_policies(&lines).expect("parse failed");
        assert_eq!(policies.len(), 1);

        let p = &policies["max_spend_per_turn"];
        assert_eq!(p.name, "max_spend_per_turn");
        assert_eq!(p.on, Event::SignalAdmit);
        assert!(p.match_expr.is_some());
        assert_eq!(p.require_exprs.len(), 1);
        assert_eq!(p.on_violation, PolicyViolationAction::Deny);
        assert!(p.priority.is_none());
    }

    #[test]
    fn parse_policy_all_fields() {
        let input = r#"
policy full_policy {
  on:           audit_export
  match:        signal.labels.contains(phi)
  require:      context.enabled == true
  transform:    drop_args([name, ssn])
  on_violation: reject(reason="policy_violation")
  priority:     high
}
"#;
        let lines = lex_policies(input);
        let policies = parse_policies(&lines).expect("parse failed");
        assert_eq!(policies.len(), 1);

        let p = &policies["full_policy"];
        assert_eq!(p.on, Event::AuditExport);
        assert!(p.match_expr.is_some());
        assert_eq!(p.require_exprs.len(), 1);
        assert!(p.transform.is_some());
        assert!(p.priority.is_some());
        assert_eq!(p.priority, Some(PolicyPriority::High));
    }

    #[test]
    fn parse_policy_reject_with_reason() {
        let input = r#"
policy spend_limit {
  on:           signal_admit
  match:        action.kind == "transfer"
  require:      signal.args.amount <= 1000
  on_violation: reject(reason="daily_limit_exceeded")
}
"#;
        let lines = lex_policies(input);
        let policies = parse_policies(&lines).expect("parse failed");

        let p = &policies["spend_limit"];
        assert!(matches!(
            &p.on_violation,
            PolicyViolationAction::Reject { reason } if reason == "daily_limit_exceeded"
        ));
    }

    #[test]
    fn parse_policy_route_to_channel() {
        let input = r#"
policy review_required {
  on:           narration_emit
  match:        signal.labels.contains(sensitive)
  on_violation: route_to(channel="human_review")
}
"#;
        let lines = lex_policies(input);
        let policies = parse_policies(&lines).expect("parse failed");

        let p = &policies["review_required"];
        assert!(matches!(
            &p.on_violation,
            PolicyViolationAction::RouteTo { channel } if channel == "human_review"
        ));
    }

    #[test]
    fn parse_policy_require_multiple() {
        let input = r#"
policy multi_require {
  on:           signal_admit
  require:      context.daily_limit > 0
  require:      signal.args.amount <= context.daily_limit
  require:      context.enabled == true
  on_violation: log_only
}
"#;
        let lines = lex_policies(input);
        let policies = parse_policies(&lines).expect("parse failed");

        let p = &policies["multi_require"];
        assert_eq!(p.require_exprs.len(), 3);
    }

    #[test]
    fn parse_policy_match_and_require() {
        let input = r#"
policy match_and_req {
  on:           signal_dispatch
  match:        action.kind == "transfer"
  require:      signal.args.amount <= context.daily_limit
  on_violation: deny
}
"#;
        let lines = lex_policies(input);
        let policies = parse_policies(&lines).expect("parse failed");

        let p = &policies["match_and_req"];
        assert!(p.match_expr.is_some());
        assert_eq!(p.require_exprs.len(), 1);
    }

    #[test]
    fn parse_policy_priority_high() {
        let input = r#"
policy high_priority {
  on:           signal_admit
  on_violation: deny
  priority:     high
}
"#;
        let lines = lex_policies(input);
        let policies = parse_policies(&lines).expect("parse failed");

        let p = &policies["high_priority"];
        assert_eq!(p.priority, Some(PolicyPriority::High));
    }

    #[test]
    fn parse_policy_all_events() {
        let events = [
            ("signal_admit", Event::SignalAdmit),
            ("signal_dispatch", Event::SignalDispatch),
            ("narration_emit", Event::NarrationEmit),
            ("state_mutate", Event::StateMutate),
            ("audit_export", Event::AuditExport),
            ("phase_transition", Event::PhaseTransition),
            ("session_start", Event::SessionStart),
            ("session_end", Event::SessionEnd),
        ];

        for (event_str, expected_event) in events {
            let input = format!(
                r#"
policy test_{event_str} {{
  on:           {event_str}
  on_violation: log_only
}}
"#
            );
            let lines = lex_policies(&input);
            let policies =
                parse_policies(&lines).unwrap_or_else(|_| panic!("parse failed for {event_str}"));
            let p = &policies[&format!("test_{event_str}")];
            assert_eq!(p.on, expected_event, "failed for event {event_str}");
        }
    }

    #[test]
    fn parse_policy_transform_redact_fields() {
        let input = r#"
policy redact_policy {
  on:           audit_export
  transform:    redact_fields([name, email])
  on_violation: log_only
}
"#;
        let lines = lex_policies(input);
        let policies = parse_policies(&lines).expect("parse failed");

        let p = &policies["redact_policy"];
        assert!(matches!(
            &p.transform,
            Some(TransformExpr::RedactFields { fields }) if fields == &vec!["name".to_string(), "email".to_string()]
        ));
    }

    #[test]
    fn parse_policy_demand_confirmation() {
        let input = r#"
policy confirm_action {
  on:           signal_admit
  on_violation: demand_confirmation(prompt="Are you sure?")
}
"#;
        let lines = lex_policies(input);
        let policies = parse_policies(&lines).expect("parse failed");

        let p = &policies["confirm_action"];
        assert!(matches!(
            &p.on_violation,
            PolicyViolationAction::DemandConfirmation { prompt } if prompt == &Some("Are you sure?".to_string())
        ));
    }

    #[test]
    fn parse_policy_demand_confirmation_no_prompt() {
        let input = r#"
policy confirm_action {
  on:           signal_admit
  on_violation: demand_confirmation()
}
"#;
        let lines = lex_policies(input);
        let policies = parse_policies(&lines).expect("parse failed");

        let p = &policies["confirm_action"];
        assert!(matches!(
            &p.on_violation,
            PolicyViolationAction::DemandConfirmation { prompt } if prompt.is_none()
        ));
    }

    #[test]
    fn parse_policy_missing_on_field() {
        let input = r#"
policy bad_policy {
  on_violation: deny
}
"#;
        let lines = lex_policies(input);
        let result = parse_policies(&lines);
        assert!(result.is_err());
        let err = result.err().unwrap();
        assert!(err.to_string().contains("missing `on`"));
    }

    #[test]
    fn parse_policy_missing_on_violation() {
        let input = r#"
policy bad_policy {
  on: signal_admit
}
"#;
        let lines = lex_policies(input);
        let result = parse_policies(&lines);
        assert!(result.is_err());
        let err = result.err().unwrap();
        assert!(err.to_string().contains("missing `on_violation`"));
    }

    #[test]
    fn parse_policy_simple_violation_actions() {
        let actions = [
            "deny",
            "redact_and_warn",
            "rewrite",
            "log_only",
            "audit_only",
        ];
        for action in actions {
            let input = format!(
                r#"
policy test_{action} {{
  on:           signal_admit
  on_violation: {action}
}}
"#
            );
            let lines = lex_policies(&input);
            let policies =
                parse_policies(&lines).unwrap_or_else(|_| panic!("parse failed for {action}"));
            assert!(
                policies.contains_key(&format!("test_{action}")),
                "missing policy for action {action}"
            );
        }
    }

    #[test]
    fn parse_multiple_policies() {
        let input = r#"
policy first {
  on:           signal_admit
  on_violation: deny
}
policy second {
  on:           audit_export
  on_violation: log_only
}
"#;
        let lines = lex_policies(input);
        let policies = parse_policies(&lines).expect("parse failed");
        assert_eq!(policies.len(), 2);
        assert!(policies.contains_key("first"));
        assert!(policies.contains_key("second"));
    }
}
