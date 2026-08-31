use indexmap::IndexMap;
use mcl_spec::document::{
    ActionDef, EntityDef, ItemDef, KnowledgeEntry, NarratorDef, TargetSpec, TransitionDef,
};
use mcl_spec::effects::{Effect, EffectKind};
use mcl_spec::label::{DegradationRule, EgressPolicy, Lattice, LatticeEntry, LatticeScope};
use mcl_spec::narrator::{NarratorConstraints, ViolationAction};
use mcl_spec::source::{Source, SourceConfig, SourceId, Trust, VerificationConfig};
use mcl_spec::{MclError, MclValue, Span};

use crate::conditions::{parse_condition, parse_field_path};
use crate::inline::{parse_inline_map, parse_string_list, parse_value};
use crate::lexer::{Line, LineKind};

/// Collect lines belonging to the current section (until next section header or EOF)
pub fn section_lines(lines: &[Line], start: usize) -> &[Line] {
    let mut end = start;
    while end < lines.len() {
        if matches!(&lines[end].kind, LineKind::SectionHeader(_)) {
            break;
        }
        end += 1;
    }
    &lines[start..end]
}

/// Parse `# State` section: simple key: value pairs
pub fn parse_state(lines: &[Line]) -> Result<IndexMap<String, MclValue>, MclError> {
    let mut state = IndexMap::new();
    for line in lines {
        match &line.kind {
            LineKind::KeyValue(key, val) => {
                state.insert(key.clone(), parse_value(val, line.span)?);
            }
            LineKind::Blank | LineKind::Comment(_) => {}
            _ => {} // skip unexpected lines gracefully
        }
    }
    Ok(state)
}

/// Parse `# Entities` section
pub fn parse_entities(lines: &[Line]) -> Result<IndexMap<String, EntityDef>, MclError> {
    let mut entities = IndexMap::new();
    let mut i = 0;

    while i < lines.len() {
        match &lines[i].kind {
            LineKind::ListItem(name, inline_val) => {
                let span = lines[i].span;
                let mut properties = IndexMap::new();
                let mut entity_type = None;
                let mut disposition = None;
                let mut knowledge = Vec::new();

                // If there's an inline value, parse it as a map
                if let Some(val) = inline_val {
                    if val.starts_with('{') {
                        properties = parse_inline_map(val, span)?;
                    }
                }

                // Consume indented lines as properties
                i += 1;
                while i < lines.len() {
                    if let LineKind::Indented(indent, content) = &lines[i].kind {
                        if *indent < 2 {
                            break;
                        }
                        if let Some((key, val)) = split_kv(content) {
                            match key.as_str() {
                                "type" => entity_type = Some(val.clone()),
                                "disposition" => disposition = Some(val.clone()),
                                "knowledge" => {
                                    if val.starts_with('[') {
                                        knowledge = parse_string_list(&val, lines[i].span)?;
                                    } else {
                                        knowledge.push(val);
                                    }
                                }
                                _ => {
                                    properties.insert(key, parse_value(&val, lines[i].span)?);
                                }
                            }
                        }
                        i += 1;
                    } else {
                        break;
                    }
                }

                // Pull type/disposition out of inline props if they were there
                if entity_type.is_none() {
                    if let Some(MclValue::String(t)) = properties.shift_remove("type") {
                        entity_type = Some(t);
                    }
                }
                if disposition.is_none() {
                    if let Some(MclValue::String(d)) = properties.shift_remove("disposition") {
                        disposition = Some(d);
                    }
                }
                if knowledge.is_empty() {
                    if let Some(MclValue::List(k)) = properties.shift_remove("knowledge") {
                        knowledge = k
                            .into_iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect();
                    }
                }

                entities.insert(
                    name.clone(),
                    EntityDef {
                        name: name.clone(),
                        entity_type,
                        disposition,
                        knowledge,
                        properties,
                        span,
                    },
                );
            }
            LineKind::Blank | LineKind::Comment(_) => {
                i += 1;
            }
            _ => {
                i += 1;
            }
        }
    }

    Ok(entities)
}

/// Parse `# Items` section
pub fn parse_items(lines: &[Line]) -> Result<IndexMap<String, ItemDef>, MclError> {
    let mut items = IndexMap::new();
    let mut i = 0;

    while i < lines.len() {
        match &lines[i].kind {
            LineKind::ListItem(name, inline_val) => {
                let span = lines[i].span;
                let mut properties = IndexMap::new();

                if let Some(val) = inline_val {
                    if val.starts_with('{') {
                        properties = parse_inline_map(val, span)?;
                    }
                }

                i += 1;
                // Consume indented properties
                while i < lines.len() {
                    if let LineKind::Indented(indent, content) = &lines[i].kind {
                        if *indent < 2 {
                            break;
                        }
                        if let Some((key, val)) = split_kv(content) {
                            properties.insert(key, parse_value(&val, lines[i].span)?);
                        }
                        i += 1;
                    } else {
                        break;
                    }
                }

                items.insert(
                    name.clone(),
                    ItemDef {
                        name: name.clone(),
                        properties,
                        span,
                    },
                );
            }
            LineKind::Blank | LineKind::Comment(_) => {
                i += 1;
            }
            _ => {
                i += 1;
            }
        }
    }

    Ok(items)
}

/// Parse `# Actions` section
pub fn parse_actions(lines: &[Line]) -> Result<IndexMap<String, ActionDef>, MclError> {
    let mut actions = IndexMap::new();
    let mut i = 0;

    while i < lines.len() {
        match &lines[i].kind {
            LineKind::ListItem(name, _) => {
                let span = lines[i].span;
                let mut requires = Vec::new();
                let mut requires_any = Vec::new();
                let mut target = None;
                let mut mutates = IndexMap::new();
                let mut resolution = None;
                let mut mode = None;
                let mut args = IndexMap::new();
                let mut cooldown = None;
                let mut description = None;
                let mut trigger_phrases = Vec::new();
                let mut effects = Vec::new();

                i += 1;
                // Consume indented properties
                while i < lines.len() {
                    if let LineKind::Indented(indent, content) = &lines[i].kind {
                        if *indent < 2 {
                            break;
                        }
                        if let Some((key, val)) = split_kv(content) {
                            match key.as_str() {
                                "requires" => {
                                    if val.starts_with('[') {
                                        let items = parse_string_list(&val, lines[i].span)?;
                                        for item in items {
                                            requires.push(parse_condition(&item, lines[i].span)?);
                                        }
                                    } else {
                                        requires.push(parse_condition(&val, lines[i].span)?);
                                    }
                                }
                                "requires_any" => {
                                    if val.starts_with('[') {
                                        let items = parse_string_list(&val, lines[i].span)?;
                                        for item in items {
                                            requires_any
                                                .push(parse_condition(&item, lines[i].span)?);
                                        }
                                    } else {
                                        requires_any.push(parse_condition(&val, lines[i].span)?);
                                    }
                                }
                                "target" => {
                                    target = Some(parse_target_spec(&val)?);
                                }
                                "mutates" if val.starts_with('{') => {
                                    mutates = parse_inline_map(&val, lines[i].span)?;
                                }
                                "resolution" => resolution = Some(val),
                                "mode" => mode = Some(val),
                                "cooldown" => cooldown = val.parse().ok(),
                                "description" => description = Some(unquote(&val)),
                                "trigger" => {
                                    if val.starts_with('[') {
                                        trigger_phrases = parse_string_list(&val, lines[i].span)?
                                            .into_iter()
                                            .map(|s| unquote(&s))
                                            .collect();
                                    } else {
                                        trigger_phrases.push(unquote(&val));
                                    }
                                }
                                "args" if val.starts_with('{') => {
                                    let map = parse_inline_map(&val, lines[i].span)?;
                                    for (k, v) in map {
                                        args.insert(k, v.to_string());
                                    }
                                }
                                "effect" => {
                                    // Don't use parse_string_list for effects - it doesn't respect nested parentheses
                                    effects = parse_effect_decl(&val, lines[i].span)?;
                                }
                                _ => {}
                            }
                        }
                        i += 1;
                    } else {
                        break;
                    }
                }

                actions.insert(
                    name.clone(),
                    ActionDef {
                        name: name.clone(),
                        requires,
                        requires_any,
                        target,
                        mutates,
                        resolution,
                        mode,
                        args,
                        cooldown,
                        description,
                        trigger_phrases,
                        effects,
                        span,
                    },
                );
            }
            LineKind::Blank | LineKind::Comment(_) => {
                i += 1;
            }
            _ => {
                i += 1;
            }
        }
    }

    Ok(actions)
}

/// Parse `# Transitions` section
pub fn parse_transitions(lines: &[Line]) -> Result<Vec<TransitionDef>, MclError> {
    let mut transitions = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        match &lines[i].kind {
            LineKind::ListItem(target, _) => {
                let span = lines[i].span;
                let mut when = None;
                let mut direction = None;
                let mut auto = false;
                let mut narrate = None;

                i += 1;
                while i < lines.len() {
                    if let LineKind::Indented(indent, content) = &lines[i].kind {
                        if *indent < 2 {
                            break;
                        }
                        if let Some((key, val)) = split_kv(content) {
                            match key.as_str() {
                                "when" => when = Some(parse_condition(&val, lines[i].span)?),
                                "direction" => direction = Some(val),
                                "auto" => auto = val == "true",
                                "narrate" => narrate = Some(unquote(&val)),
                                _ => {}
                            }
                        }
                        i += 1;
                    } else {
                        break;
                    }
                }

                transitions.push(TransitionDef {
                    target: target.clone(),
                    when,
                    direction,
                    auto,
                    narrate,
                    span,
                });
            }
            LineKind::Blank | LineKind::Comment(_) => {
                i += 1;
            }
            _ => {
                i += 1;
            }
        }
    }

    Ok(transitions)
}

/// Parse `# Narrator` section — returns both the legacy `NarratorDef` and the new `NarratorConstraints`.
pub fn parse_narrator(
    lines: &[Line],
) -> Result<(NarratorDef, Option<NarratorConstraints>), MclError> {
    let mut narrator = NarratorDef::default();
    let mut constraints = NarratorConstraints::default();
    let mut has_constraints = false;

    for line in lines {
        if let LineKind::KeyValue(key, val) = &line.kind {
            match key.as_str() {
                "tone" => narrator.tone = Some(val.clone()),
                "voice" => narrator.voice = Some(val.clone()),
                "tense" => narrator.tense = Some(val.clone()),
                "context" => narrator.context = Some(unquote(val)),
                "style_notes" => narrator.style_notes = Some(unquote(val)),
                "forbidden" => {
                    if val.starts_with('[') {
                        narrator.forbidden = parse_string_list(val, line.span)?;
                    } else {
                        narrator.forbidden.push(val.clone());
                    }
                }
                "must_not_mention" => {
                    has_constraints = true;
                    if val.starts_with('[') {
                        constraints.must_not_mention = parse_string_list(val, line.span)?;
                    } else {
                        constraints.must_not_mention.push(val.clone());
                    }
                }
                "must_not_emit_pattern" => {
                    has_constraints = true;
                    if val.starts_with('[') {
                        constraints.must_not_emit_pattern = parse_string_list(val, line.span)?;
                    } else {
                        constraints.must_not_emit_pattern.push(val.clone());
                    }
                }
                "must_not_emit_word" => {
                    has_constraints = true;
                    if val.starts_with('[') {
                        constraints.must_not_emit_word = parse_string_list(val, line.span)?;
                    } else {
                        constraints.must_not_emit_word.push(val.clone());
                    }
                }
                "prose_max_length" => {
                    has_constraints = true;
                    constraints.prose_max_length = val.parse().ok();
                }
                "on_violation" => {
                    has_constraints = true;
                    constraints.on_violation = parse_violation_action(val);
                }
                _ => {}
            }
        }
    }

    Ok((narrator, has_constraints.then_some(constraints)))
}

fn parse_violation_action(s: &str) -> ViolationAction {
    match s.trim() {
        "deny" => ViolationAction::Deny,
        "redact_and_warn" => ViolationAction::RedactAndWarn,
        "rewrite" => ViolationAction::Rewrite,
        "audit_only" => ViolationAction::AuditOnly,
        other => {
            log::warn!("unknown on_violation value `{other}`, defaulting to audit_only");
            ViolationAction::AuditOnly
        }
    }
}

/// Parse `# Knowledge` section
pub fn parse_knowledge(lines: &[Line]) -> Result<Vec<KnowledgeEntry>, MclError> {
    let mut entries = Vec::new();

    for line in lines {
        match &line.kind {
            LineKind::ListItem(name, _) => {
                if let Some(stripped) = name.strip_prefix('!') {
                    entries.push(KnowledgeEntry::Unknown(stripped.to_string()));
                } else {
                    entries.push(KnowledgeEntry::Known(name.clone()));
                }
            }
            LineKind::Blank | LineKind::Comment(_) => {}
            _ => {}
        }
    }

    Ok(entries)
}

/// Parse `# Constraints` section — free-form bulleted list
pub fn parse_constraints(lines: &[Line]) -> Result<Vec<String>, MclError> {
    let mut constraints = Vec::new();

    for line in lines {
        match &line.kind {
            LineKind::ListItem(text, extra) => {
                let mut s = text.clone();
                if let Some(e) = extra {
                    s.push_str(": ");
                    s.push_str(e);
                }
                constraints.push(s);
            }
            LineKind::Blank | LineKind::Comment(_) => {}
            _ => {}
        }
    }

    Ok(constraints)
}

fn parse_target_spec(s: &str) -> Result<TargetSpec, MclError> {
    let trimmed = s.trim();
    if trimmed.starts_with("enum(") && trimmed.ends_with(')') {
        let inner = &trimmed[5..trimmed.len() - 1];
        let path = parse_field_path(inner)?;
        Ok(TargetSpec::Enum(path))
    } else {
        Ok(TargetSpec::Fixed(trimmed.to_string()))
    }
}

fn parse_effect_decl(input: &str, span: Span) -> Result<Vec<Effect>, MclError> {
    let input = input.trim();

    // List form: [effect1, effect2, ...]
    if input.starts_with('[') && input.ends_with(']') {
        let inner = &input[1..input.len() - 1];
        if inner.is_empty() {
            return Ok(Vec::new());
        }
        // Split by comma, but respect nested parentheses
        let items = split_commas_nested(inner);
        let mut effects = Vec::new();
        for item in items {
            effects.push(parse_effect(&item, span)?);
        }
        return Ok(effects);
    }

    // Single effect
    Ok(vec![parse_effect(input, span)?])
}

fn split_commas_nested(input: &str) -> Vec<String> {
    let mut items = Vec::new();
    let mut current = String::new();
    let mut paren_depth = 0;
    let mut bracket_depth = 0;

    for ch in input.chars() {
        match ch {
            '(' => {
                paren_depth += 1;
                current.push(ch);
            }
            ')' => {
                paren_depth -= 1;
                current.push(ch);
            }
            '[' => {
                bracket_depth += 1;
                current.push(ch);
            }
            ']' => {
                bracket_depth -= 1;
                current.push(ch);
            }
            ',' if paren_depth == 0 && bracket_depth == 0 => {
                let trimmed = current.trim().to_string();
                if !trimmed.is_empty() {
                    items.push(trimmed);
                }
                current.clear();
            }
            _ => {
                current.push(ch);
            }
        }
    }

    let trimmed = current.trim().to_string();
    if !trimmed.is_empty() {
        items.push(trimmed);
    }

    items
}

fn parse_effect(input: &str, span: Span) -> Result<Effect, MclError> {
    let input = input.trim();

    // Parse effects with parentheses
    if let Some(paren_pos) = input.find('(') {
        if !input.ends_with(')') {
            return Err(MclError::parse(
                format!("effect `{input}` has unclosed parentheses"),
                span,
            ));
        }
        let name = &input[..paren_pos];
        let args = &input[paren_pos + 1..input.len() - 1];

        match name.trim() {
            "external" => {
                let kinds = parse_effect_kinds(args, span)?;
                Ok(Effect::External(kinds))
            }
            "dispatch" => {
                let event_name = args.trim().to_string();
                if event_name.is_empty() {
                    return Err(MclError::parse(
                        "dispatch effect requires an event name",
                        span,
                    ));
                }
                Ok(Effect::Dispatch(event_name))
            }
            "idempotency_key" => {
                let (required, arg) = parse_idempotency_args(args, span)?;
                Ok(Effect::IdempotencyKey { required, arg })
            }
            _ => Err(MclError::parse(
                format!("unknown effect with args: `{name}`"),
                span,
            )),
        }
    } else {
        // Simple effects without parentheses
        match input {
            "pure" => Ok(Effect::Pure),
            "state_mutating" => Ok(Effect::StateMutating),
            "audit_verbose" => Ok(Effect::AuditVerbose),
            "audit_minimal" => Ok(Effect::AuditMinimal),
            "cacheable" => Ok(Effect::Cacheable),
            "replay_safe" => Ok(Effect::ReplaySafe),
            "replay_unsafe" => Ok(Effect::ReplayUnsafe),
            _ => Err(MclError::parse(format!("unknown effect: `{input}`"), span)),
        }
    }
}

fn parse_effect_kinds(input: &str, span: Span) -> Result<Vec<EffectKind>, MclError> {
    let kinds: Vec<&str> = input
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    let mut result = Vec::new();

    for kind in kinds {
        let effect_kind = match kind {
            "io" => EffectKind::Io,
            "network" => EffectKind::Network,
            "filesystem" => EffectKind::Filesystem,
            "time" => EffectKind::Time,
            "random" => EffectKind::Random,
            "deterministic" => EffectKind::Deterministic,
            "nondeterministic" => EffectKind::Nondeterministic,
            _ => {
                return Err(MclError::parse(
                    format!("unknown effect kind: `{kind}`"),
                    span,
                ));
            }
        };
        result.push(effect_kind);
    }

    Ok(result)
}

fn parse_idempotency_args(input: &str, span: Span) -> Result<(bool, Option<String>), MclError> {
    let mut required = false;
    let mut arg = None;

    let args: Vec<&str> = input
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();

    for arg_str in args {
        if let Some((key, value)) = arg_str.split_once('=') {
            match key.trim() {
                "arg" => arg = Some(value.trim().to_string()),
                _ => {
                    return Err(MclError::parse(
                        format!("unknown idempotency_key argument: `{key}`"),
                        span,
                    ))
                }
            }
        } else {
            // Positional argument - must be "required"
            match arg_str {
                "required" => required = true,
                _ => {
                    return Err(MclError::parse(
                        format!("unknown idempotency_key argument: `{arg_str}`"),
                        span,
                    ))
                }
            }
        }
    }

    Ok((required, arg))
}

fn split_kv(s: &str) -> Option<(String, String)> {
    let pos = s.find(':')?;
    let key = s[..pos].trim().to_string();
    let val = s[pos + 1..].trim().to_string();
    if key.is_empty() {
        return None;
    }
    Some((key, val))
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

pub fn parse_sources(lines: &[Line]) -> Result<SourceConfig, MclError> {
    let mut sources = SourceConfig::new();

    for line in lines {
        match &line.kind {
            LineKind::KeyValue(key, val) => {
                let source_id = SourceId::from(key.as_str());
                let source = parse_source_entry(val, line.span)?;
                sources.insert(source_id, source);
            }
            LineKind::Blank | LineKind::Comment(_) => {}
            _ => {}
        }
    }

    Ok(sources)
}

fn parse_source_entry(val: &str, span: Span) -> Result<Source, MclError> {
    let trimmed = val.trim();

    if !trimmed.starts_with('{') || !trimmed.ends_with('}') {
        let trust = parse_trust(trimmed)?;
        return Ok(Source {
            id: SourceId::from(""),
            trust,
            inherit_labels: Vec::new(),
            verify: None,
        });
    }

    let map = parse_inline_map(trimmed, span)?;

    let mut trust = Trust::System;
    let mut inherit_labels = Vec::new();
    let mut verify = None;

    for (key, value) in &map {
        match key.as_str() {
            "trust" => {
                trust = value
                    .as_str()
                    .ok_or_else(|| MclError::parse("trust must be a string", span))
                    .and_then(parse_trust)?;
            }
            "inherit_labels" => {
                if let Some(list) = value.as_list() {
                    inherit_labels = list
                        .iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect();
                }
            }
            "verify" => {
                if let MclValue::String(s) = value {
                    if let Some(secret_env) = s
                        .strip_prefix("hmac_sha256(")
                        .and_then(|r| r.strip_suffix(')'))
                    {
                        if let Some((_, env)) = secret_env.split_once("secret_env=") {
                            verify = Some(VerificationConfig::HmacSha256 {
                                secret_env: env.trim().to_string(),
                            });
                        }
                    }
                }
            }
            _ => {}
        }
    }

    Ok(Source {
        id: SourceId::from(""),
        trust,
        inherit_labels,
        verify,
    })
}

fn parse_trust(s: &str) -> Result<Trust, MclError> {
    match s.trim() {
        "user" => Ok(Trust::User),
        "narrative" => Ok(Trust::Narrative),
        "untrusted" => Ok(Trust::Untrusted),
        "external" => Ok(Trust::External),
        "system" => Ok(Trust::System),
        other => Err(MclError::parse(
            format!("unknown trust level `{other}`"),
            Span::empty(),
        )),
    }
}

pub fn parse_labels(lines: &[Line]) -> Result<Lattice, MclError> {
    let mut entries = Vec::new();

    for line in lines {
        match &line.kind {
            LineKind::ListItem(name, rest) => {
                let entry = if let Some(rest_val) = rest {
                    if rest_val.starts_with('{') && rest_val.ends_with('}') {
                        parse_lattice_entry(name, rest_val, line.span)?
                    } else {
                        LatticeEntry {
                            name: name.clone(),
                            dominates: Vec::new(),
                            default_egress: EgressPolicy::default(),
                            params: Vec::new(),
                            scope: LatticeScope::default(),
                        }
                    }
                } else {
                    LatticeEntry {
                        name: name.clone(),
                        dominates: Vec::new(),
                        default_egress: EgressPolicy::default(),
                        params: Vec::new(),
                        scope: LatticeScope::default(),
                    }
                };
                entries.push(entry);
            }
            LineKind::Blank | LineKind::Comment(_) => {}
            _ => {}
        }
    }

    Ok(Lattice { labels: entries })
}

fn parse_lattice_entry(name: &str, inline: &str, span: Span) -> Result<LatticeEntry, MclError> {
    let map = parse_inline_map(inline, span)?;

    let mut dominates = Vec::new();
    let mut default_egress = EgressPolicy::default();
    let mut params = Vec::new();
    let mut scope = LatticeScope::default();

    for (key, value) in &map {
        match key.as_str() {
            "dominates" => {
                if let Some(list) = value.as_list() {
                    dominates = list
                        .iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect();
                }
            }
            "default_egress" => {
                if let Some(s) = value.as_str() {
                    default_egress = match s {
                        "deny" => EgressPolicy::Deny,
                        "allow" => EgressPolicy::Allow,
                        "redact_and_warn" => EgressPolicy::RedactAndWarn,
                        _ => EgressPolicy::default(),
                    };
                }
            }
            "params" => {
                if let Some(list) = value.as_list() {
                    params = list
                        .iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect();
                }
            }
            "scope" => {
                if let Some(s) = value.as_str() {
                    if let Some(audience) = s
                        .strip_prefix("per_audience(")
                        .and_then(|r| r.strip_suffix(')'))
                    {
                        scope = LatticeScope::PerAudience(audience.trim().to_string());
                    }
                }
            }
            _ => {}
        }
    }

    Ok(LatticeEntry {
        name: name.to_string(),
        dominates,
        default_egress,
        params,
        scope,
    })
}

pub fn parse_degrade(lines: &[Line]) -> Result<Vec<DegradationRule>, MclError> {
    let mut rules = Vec::new();

    let mut i = 0;
    while i < lines.len() {
        if let Some(rule) = try_parse_degrade_block(lines, &mut i) {
            rules.push(rule);
        } else {
            i += 1;
        }
    }

    Ok(rules)
}

fn try_parse_degrade_block(lines: &[Line], pos: &mut usize) -> Option<DegradationRule> {
    let raw = match &lines[*pos].kind {
        LineKind::Indented(_, content) => content.clone(),
        LineKind::KeyValue(_, val) => val.clone(),
        _ => return None,
    };

    let header = raw.trim();

    let Some(degrade_idx) = header.find("degrade ") else {
        return None;
    };
    let after_degrade = header[degrade_idx + 8..].trim();

    let Some(space_idx) = after_degrade.find(' ') else {
        return None;
    };
    let label = after_degrade[..space_idx].trim().to_string();
    let after_label = after_degrade[space_idx + 1..].trim();

    let brace_pos = after_label.rfind('{')?;
    let before_brace = after_label[..brace_pos].trim();

    let to_pos = if before_brace.starts_with("to ") {
        0
    } else {
        before_brace.rfind(" to ")?
    };
    let _from_params = before_brace[..to_pos].trim();
    let to_start = to_pos + if to_pos == 0 { 3 } else { 4 };
    let to_level = before_brace[to_start..].trim().to_string();

    let mut redact = Vec::new();
    let mut preserve = Vec::new();

    *pos += 1;
    while *pos < lines.len() {
        match &lines[*pos].kind {
            LineKind::Indented(indent, content) if *indent >= 2 => {
                if let Some((key, val)) = split_kv(content) {
                    match key.as_str() {
                        "redact" => {
                            if val.starts_with('[') {
                                redact = parse_string_list(&val, lines[*pos].span)
                                    .ok()
                                    .unwrap_or_default();
                            }
                        }
                        "preserve" if val.starts_with('[') => {
                            preserve = parse_string_list(&val, lines[*pos].span)
                                .ok()
                                .unwrap_or_default();
                        }
                        _ => {}
                    }
                }
                *pos += 1;
            }
            _ => break,
        }
    }

    Some(DegradationRule {
        label,
        from_params: None,
        to_level,
        redact,
        preserve,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::lex;

    #[test]
    fn parse_effect_pure() {
        let input = "pure";
        let result = parse_effect_decl(input, Span::empty());
        assert!(result.is_ok());
        let effects = result.unwrap();
        assert_eq!(effects.len(), 1);
        assert_eq!(effects[0], Effect::Pure);
    }

    #[test]
    fn parse_effect_list() {
        let input = "[state_mutating, dispatch(combat), audit_verbose]";
        let result = parse_effect_decl(input, Span::empty());
        assert!(result.is_ok());
        let effects = result.unwrap();
        assert_eq!(effects.len(), 3);
        assert_eq!(effects[0], Effect::StateMutating);
        assert_eq!(effects[1], Effect::Dispatch("combat".to_string()));
        assert_eq!(effects[2], Effect::AuditVerbose);
    }

    #[test]
    fn parse_effect_external_kinds() {
        let input = "external(io, network)";
        let result = parse_effect_decl(input, Span::empty());
        assert!(result.is_ok());
        let effects = result.unwrap();
        assert_eq!(effects.len(), 1);
        assert!(
            matches!(&effects[0], Effect::External(kinds) if kinds.len() == 2
            && kinds.contains(&EffectKind::Io)
            && kinds.contains(&EffectKind::Network))
        );
    }

    #[test]
    fn parse_effect_dispatch() {
        let input = "dispatch(event_name)";
        let result = parse_effect_decl(input, Span::empty());
        assert!(result.is_ok());
        let effects = result.unwrap();
        assert_eq!(effects.len(), 1);
        assert_eq!(effects[0], Effect::Dispatch("event_name".to_string()));
    }

    #[test]
    fn parse_effect_idempotency_key() {
        let input = "idempotency_key(required, arg=request_id)";
        let result = parse_effect_decl(input, Span::empty());
        assert!(result.is_ok());
        let effects = result.unwrap();
        assert_eq!(effects.len(), 1);
        match &effects[0] {
            Effect::IdempotencyKey { required, arg } => {
                assert!(*required);
                assert_eq!(arg, &Some("request_id".to_string()));
            }
            _ => panic!("Expected IdempotencyKey effect"),
        }
    }

    #[test]
    fn parse_effect_replay() {
        let input1 = "replay_safe";
        let input2 = "replay_unsafe";

        let result1 = parse_effect_decl(input1, Span::empty()).unwrap();
        assert_eq!(result1, vec![Effect::ReplaySafe]);

        let result2 = parse_effect_decl(input2, Span::empty()).unwrap();
        assert_eq!(result2, vec![Effect::ReplayUnsafe]);
    }

    #[test]
    fn parse_effect_on_action() {
        let mcl_text = r#"
# Actions
- attack
  effect: [state_mutating, external(io, network)]
  requires: [has_item(weapon)]
  target: enum(state.combat)
"#;

        let lines = lex(mcl_text);
        let actions = parse_actions(&lines).expect("Failed to parse actions");

        assert!(actions.contains_key("attack"));
        let attack = actions.get("attack").unwrap();
        assert_eq!(attack.effects.len(), 2);
        assert_eq!(attack.effects[0], Effect::StateMutating);
        assert!(matches!(&attack.effects[1], Effect::External(kinds) if kinds.len() == 2));
    }

    #[test]
    fn parse_action_without_effect() {
        let mcl_text = r#"
# Actions
- simple_action
  requires: [has_item(key)]
"#;

        let lines = lex(mcl_text);
        let actions = parse_actions(&lines).expect("Failed to parse actions");

        assert!(actions.contains_key("simple_action"));
        let action = actions.get("simple_action").unwrap();
        assert!(action.effects.is_empty());
    }

    #[test]
    fn parse_effect_unknown() {
        let input = "unknown_effect";
        let result = parse_effect_decl(input, Span::empty());
        assert!(result.is_err());
    }

    #[test]
    fn parse_effect_unclosed_parens() {
        let input = "external(io";
        let result = parse_effect_decl(input, Span::empty());
        assert!(result.is_err());
    }

    #[test]
    fn parse_effect_empty_list() {
        let input = "[]";
        let result = parse_effect_decl(input, Span::empty());
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn parse_effect_cacheable() {
        let input = "cacheable";
        let result = parse_effect_decl(input, Span::empty());
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), vec![Effect::Cacheable]);
    }

    #[test]
    fn parse_effect_audit_minimal() {
        let input = "audit_minimal";
        let result = parse_effect_decl(input, Span::empty());
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), vec![Effect::AuditMinimal]);
    }

    #[test]
    fn parse_effect_external_single_kind() {
        let input = "external(network)";
        let result = parse_effect_decl(input, Span::empty());
        assert!(result.is_ok());
        let effects = result.unwrap();
        assert_eq!(effects.len(), 1);
        assert!(
            matches!(&effects[0], Effect::External(kinds) if kinds.len() == 1
            && kinds.contains(&EffectKind::Network))
        );
    }

    #[test]
    fn parse_effect_idempotency_key_optional() {
        let input = "idempotency_key(required)";
        let result = parse_effect_decl(input, Span::empty());
        assert!(result.is_ok());
        let effects = result.unwrap();
        assert_eq!(effects.len(), 1);
        match &effects[0] {
            Effect::IdempotencyKey { required, arg } => {
                assert!(*required);
                assert!(arg.is_none());
            }
            _ => panic!("Expected IdempotencyKey effect"),
        }
    }

    #[test]
    fn parse_effect_idempotency_key_no_required() {
        let input = "idempotency_key(arg=custom_key)";
        let result = parse_effect_decl(input, Span::empty());
        assert!(result.is_ok());
        let effects = result.unwrap();
        assert_eq!(effects.len(), 1);
        match &effects[0] {
            Effect::IdempotencyKey { required, arg } => {
                assert!(!*required);
                assert_eq!(arg, &Some("custom_key".to_string()));
            }
            _ => panic!("Expected IdempotencyKey effect"),
        }
    }
}
