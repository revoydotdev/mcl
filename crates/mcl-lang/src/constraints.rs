use indexmap::IndexMap;

/// Inline constraint tag parsed from XML-style annotations in prose.
///
/// MCL uses XML-style tags to annotate dialogue and narrative prose with
/// tonal, practical, and affective constraints that compile into LLM
/// system prompt directives.
///
/// # Syntax
///
/// ```mcl
/// <tone mood="dread" intensity="high">
///     Prose text with this tonal quality.
/// </tone>
///
/// <affect empathy="low" hostility="guarded">
///     Text delivered with this emotional register.
/// </affect>
///
/// <practical goal="prevent_passage" fallback="attack">
///     NPC behavioral directive.
/// </practical>
///
/// <voice register="archaic" pace="slow" volume="whisper">
///     Delivery style for this passage.
/// </voice>
///
/// <constraint>
///     Never reveal the location of the treasure.
/// </constraint>
/// ```
///
/// Tags can be:
/// - **Block**: wraps prose, applies to enclosed text
/// - **Self-closing**: `<tone mood="dread" />` applies to the enclosing scope
/// - **Nested**: tags compose (inner inherits outer, overrides on conflict)
#[derive(Debug, Clone)]
pub struct ConstraintTag {
    pub tag_type: ConstraintTagType,
    pub attrs: IndexMap<String, String>,
    pub body: Option<String>,
    pub children: Vec<ConstraintTag>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConstraintTagType {
    /// Emotional mood/atmosphere: mood, intensity, temperature
    Tone,
    /// Emotional delivery: empathy, hostility, warmth, detachment, urgency
    Affect,
    /// Behavioral goals: goal, fallback, priority, boundaries
    Practical,
    /// Delivery style: register, pace, volume, formality, dialect
    Voice,
    /// Hard rule (prose body is the constraint text)
    Constraint,
    /// Custom/extensible tag type
    Custom(String),
}

impl std::str::FromStr for ConstraintTagType {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "tone" => Self::Tone,
            "affect" => Self::Affect,
            "practical" => Self::Practical,
            "voice" => Self::Voice,
            "constraint" => Self::Constraint,
            other => Self::Custom(other.to_string()),
        })
    }
}

impl ConstraintTagType {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Tone => "tone",
            Self::Affect => "affect",
            Self::Practical => "practical",
            Self::Voice => "voice",
            Self::Constraint => "constraint",
            Self::Custom(s) => s,
        }
    }
}

/// Parse constraint tags from a prose string.
/// Returns the cleaned prose (tags stripped) and extracted tags.
pub fn parse_constraint_tags(input: &str) -> (String, Vec<ConstraintTag>) {
    let mut tags = Vec::new();
    let mut clean = String::new();
    let mut pos = 0;
    let bytes = input.as_bytes();

    while pos < bytes.len() {
        if bytes[pos] == b'<' && pos + 1 < bytes.len() && bytes[pos + 1] != b'/' {
            // Try to parse an opening tag
            if let Some((tag, end_pos)) = try_parse_tag(input, pos) {
                tags.push(tag);
                pos = end_pos;
                continue;
            }
        }

        if bytes[pos] == b'<' && pos + 1 < bytes.len() && bytes[pos + 1] == b'/' {
            // Skip closing tags
            if let Some(end) = input[pos..].find('>') {
                pos += end + 1;
                continue;
            }
        }

        clean.push(bytes[pos] as char);
        pos += 1;
    }

    // Trim excessive whitespace from cleaned prose
    let clean = clean
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n");

    (clean, tags)
}

fn try_parse_tag(input: &str, start: usize) -> Option<(ConstraintTag, usize)> {
    let rest = &input[start..];

    // Must start with <
    if !rest.starts_with('<') {
        return None;
    }

    // Find the end of the opening tag
    let tag_end = rest.find('>')?;
    let tag_content = &rest[1..tag_end];

    // Self-closing?
    let self_closing = tag_content.ends_with('/');
    let tag_content = if self_closing {
        tag_content[..tag_content.len() - 1].trim()
    } else {
        tag_content.trim()
    };

    // Parse tag name and attributes
    let (tag_name, attrs) = parse_tag_header(tag_content)?;
    // FromStr returns Infallible error — unknown names become Custom, so this never panics
    let tag_type: ConstraintTagType = tag_name.parse().unwrap_or_else(|_| unreachable!());

    if self_closing {
        return Some((
            ConstraintTag {
                tag_type,
                attrs,
                body: None,
                children: Vec::new(),
            },
            start + tag_end + 1,
        ));
    }

    // Find matching closing tag
    let closing = format!("</{tag_name}>");
    let body_start = start + tag_end + 1;

    if let Some(close_offset) = input[body_start..].find(&closing) {
        let body = input[body_start..body_start + close_offset]
            .trim()
            .to_string();
        let end_pos = body_start + close_offset + closing.len();

        // Recursively parse children tags from body
        let (clean_body, children) = parse_constraint_tags(&body);

        Some((
            ConstraintTag {
                tag_type,
                attrs,
                body: if clean_body.is_empty() {
                    None
                } else {
                    Some(clean_body)
                },
                children,
            },
            end_pos,
        ))
    } else {
        // No closing tag found — treat as self-closing
        Some((
            ConstraintTag {
                tag_type,
                attrs,
                body: None,
                children: Vec::new(),
            },
            start + tag_end + 1,
        ))
    }
}

fn parse_tag_header(content: &str) -> Option<(String, IndexMap<String, String>)> {
    let mut parts = content.splitn(2, |c: char| c.is_whitespace());
    let name = parts.next()?.trim().to_lowercase();

    if name.is_empty() || name.contains(|c: char| !c.is_alphanumeric() && c != '_' && c != '-') {
        return None;
    }

    let mut attrs = IndexMap::new();
    if let Some(attr_str) = parts.next() {
        for attr in split_attrs(attr_str) {
            if let Some((key, val)) = attr.split_once('=') {
                let key = key.trim().to_string();
                let val = val.trim().trim_matches('"').trim_matches('\'').to_string();
                attrs.insert(key, val);
            }
        }
    }

    Some((name, attrs))
}

fn split_attrs(s: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut in_quotes = false;

    for (i, ch) in s.char_indices() {
        match ch {
            '"' | '\'' => in_quotes = !in_quotes,
            ' ' | '\t' if !in_quotes => {
                let part = s[start..i].trim();
                if !part.is_empty() {
                    parts.push(part);
                }
                start = i + 1;
            }
            _ => {}
        }
    }

    let part = s[start..].trim();
    if !part.is_empty() {
        parts.push(part);
    }

    parts
}

/// Compile constraint tags into system prompt directives.
///
/// Each tag type produces a distinct, imperative instruction for the LLM:
///
/// - **Tone**: "You MUST maintain a [Tone: ...] throughout your narration."
/// - **Affect**: "The emotional register should convey [Affect: ...]."
/// - **Practical**: "You MUST follow this behavioral directive: [Behavioral: ...]"
/// - **Voice**: "Narrate using [Voice style: ...]."
/// - **Constraint**: "You MUST NOT violate this rule: [Hard constraint: ...]"
/// - **Custom**: "You MUST comply: [{name}: ...]"
///
/// The bracketed annotations are preserved as structured references so the
/// LLM can cite specific constraints when making decisions.
pub fn compile_constraints_to_prompt(tags: &[ConstraintTag]) -> String {
    let mut directives = Vec::new();

    for tag in tags {
        match &tag.tag_type {
            ConstraintTagType::Tone => {
                let mut parts = Vec::new();
                if let Some(mood) = tag.attrs.get("mood") {
                    parts.push(format!("mood: {mood}"));
                }
                if let Some(intensity) = tag.attrs.get("intensity") {
                    parts.push(format!("intensity: {intensity}"));
                }
                if let Some(temp) = tag.attrs.get("temperature") {
                    parts.push(format!("emotional temperature: {temp}"));
                }
                if !parts.is_empty() {
                    let annotation = format!("[Tone: {}]", parts.join(", "));
                    directives.push(format!(
                        "You MUST maintain a {annotation} throughout your narration."
                    ));
                }
                if let Some(ref body) = tag.body {
                    directives.push(format!("Apply the specified tone to this passage: {body}"));
                }
            }

            ConstraintTagType::Affect => {
                let mut parts = Vec::new();
                for (key, val) in &tag.attrs {
                    parts.push(format!("{key}: {val}"));
                }
                if !parts.is_empty() {
                    let annotation = format!("[Affect: {}]", parts.join(", "));
                    directives.push(format!(
                        "The emotional register should convey {annotation}."
                    ));
                }
                if let Some(ref body) = tag.body {
                    directives.push(format!(
                        "Deliver this text with the specified emotional register: {body}"
                    ));
                }
            }

            ConstraintTagType::Practical => {
                let mut parts = Vec::new();
                if let Some(goal) = tag.attrs.get("goal") {
                    parts.push(format!("primary goal: {goal}"));
                }
                if let Some(fallback) = tag.attrs.get("fallback") {
                    parts.push(format!("fallback behavior: {fallback}"));
                }
                if let Some(boundary) = tag.attrs.get("boundary") {
                    parts.push(format!("hard boundary: {boundary}"));
                }
                if !parts.is_empty() {
                    let annotation = format!("[Behavioral: {}]", parts.join(", "));
                    directives.push(format!(
                        "You MUST follow this behavioral directive: {annotation}"
                    ));
                }
                if let Some(ref body) = tag.body {
                    directives.push(format!("Behavioral note: {body}"));
                }
            }

            ConstraintTagType::Voice => {
                let mut parts = Vec::new();
                for (key, val) in &tag.attrs {
                    parts.push(format!("{key}: {val}"));
                }
                if !parts.is_empty() {
                    let annotation = format!("[Voice style: {}]", parts.join(", "));
                    directives.push(format!("Narrate using {annotation}."));
                }
                if let Some(ref body) = tag.body {
                    directives.push(format!(
                        "Deliver this passage in the specified voice: {body}"
                    ));
                }
            }

            ConstraintTagType::Constraint => {
                if let Some(ref body) = tag.body {
                    let annotation = format!("[Hard constraint: {body}]");
                    directives.push(format!("You MUST NOT violate this rule: {annotation}"));
                }
            }

            ConstraintTagType::Custom(name) => {
                let mut parts = Vec::new();
                for (key, val) in &tag.attrs {
                    parts.push(format!("{key}={val}"));
                }
                let attrs_str = if parts.is_empty() {
                    String::new()
                } else {
                    format!(" ({})", parts.join(", "))
                };
                if let Some(ref body) = tag.body {
                    directives.push(format!("You MUST comply: [{name}{attrs_str}: {body}]"));
                } else {
                    directives.push(format!("You MUST comply: [{name}{attrs_str}]"));
                }
            }
        }

        // Recurse into children
        if !tag.children.is_empty() {
            directives.push(compile_constraints_to_prompt(&tag.children));
        }
    }

    directives.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_tone_tag() {
        let input = r#"<tone mood="dread" intensity="high">Dark and foreboding prose.</tone>"#;
        let (_, tags) = parse_constraint_tags(input);
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].tag_type, ConstraintTagType::Tone);
        assert_eq!(tags[0].attrs.get("mood"), Some(&"dread".to_string()));
        assert_eq!(tags[0].attrs.get("intensity"), Some(&"high".to_string()));
        assert_eq!(tags[0].body, Some("Dark and foreboding prose.".to_string()));
    }

    #[test]
    fn parse_self_closing_tag() {
        let input = r#"<affect empathy="low" hostility="guarded" /> Some text."#;
        let (clean, tags) = parse_constraint_tags(input);
        assert_eq!(clean, "Some text.");
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].tag_type, ConstraintTagType::Affect);
        assert!(tags[0].body.is_none());
    }

    #[test]
    fn parse_practical_tag() {
        let input = r#"<practical goal="prevent_passage" fallback="attack">Guard the door at all costs.</practical>"#;
        let (_, tags) = parse_constraint_tags(input);
        assert_eq!(tags[0].tag_type, ConstraintTagType::Practical);
        assert_eq!(
            tags[0].attrs.get("goal"),
            Some(&"prevent_passage".to_string())
        );
        assert_eq!(tags[0].attrs.get("fallback"), Some(&"attack".to_string()));
        assert_eq!(
            tags[0].body,
            Some("Guard the door at all costs.".to_string())
        );
    }

    #[test]
    fn parse_constraint_tag() {
        let input = "<constraint>Never reveal the location of the treasure.</constraint>";
        let (clean, tags) = parse_constraint_tags(input);
        assert_eq!(clean, "");
        assert_eq!(tags[0].tag_type, ConstraintTagType::Constraint);
        assert_eq!(
            tags[0].body,
            Some("Never reveal the location of the treasure.".to_string())
        );
    }

    #[test]
    fn parse_nested_tags() {
        let input = r#"<tone mood="dread"><voice register="archaic">Speak ye not of the dead.</voice></tone>"#;
        let (clean, tags) = parse_constraint_tags(input);
        assert_eq!(clean, "");
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].tag_type, ConstraintTagType::Tone);
        assert_eq!(tags[0].children.len(), 1);
        assert_eq!(tags[0].children[0].tag_type, ConstraintTagType::Voice);
        assert_eq!(
            tags[0].children[0].body,
            Some("Speak ye not of the dead.".to_string())
        );
    }

    #[test]
    fn parse_mixed_prose_and_tags() {
        let input = r#"You enter the crypt.
<tone mood="dread" intensity="high">
The air is thick with decay.
</tone>
A skeleton stands guard."#;

        let (clean, tags) = parse_constraint_tags(input);
        assert!(clean.contains("You enter the crypt."));
        assert!(clean.contains("A skeleton stands guard."));
        assert_eq!(tags.len(), 1);
        assert_eq!(
            tags[0].body,
            Some("The air is thick with decay.".to_string())
        );
    }

    #[test]
    fn compile_to_prompt() {
        let tags = vec![
            ConstraintTag {
                tag_type: ConstraintTagType::Tone,
                attrs: IndexMap::from([
                    ("mood".into(), "dread".into()),
                    ("intensity".into(), "high".into()),
                ]),
                body: None,
                children: Vec::new(),
            },
            ConstraintTag {
                tag_type: ConstraintTagType::Practical,
                attrs: IndexMap::from([("goal".into(), "prevent_passage".into())]),
                body: None,
                children: Vec::new(),
            },
            ConstraintTag {
                tag_type: ConstraintTagType::Constraint,
                attrs: IndexMap::new(),
                body: Some("Never reveal the treasure location.".into()),
                children: Vec::new(),
            },
        ];

        let prompt = compile_constraints_to_prompt(&tags);
        assert!(prompt.contains("[Tone: mood: dread, intensity: high]"));
        assert!(prompt.contains("[Behavioral: primary goal: prevent_passage]"));
        assert!(prompt.contains("[Hard constraint: Never reveal the treasure location.]"));
    }

    #[test]
    fn tone_tag_compiles_to_directive() {
        let tags = vec![ConstraintTag {
            tag_type: ConstraintTagType::Tone,
            attrs: IndexMap::from([("mood".into(), "ominous".into())]),
            body: None,
            children: Vec::new(),
        }];
        let prompt = compile_constraints_to_prompt(&tags);
        assert!(
            prompt.contains("ominous"),
            "tone directive must include the mood value — got:\n{prompt}"
        );
        assert!(
            prompt.contains("maintain"),
            "tone directive must be an imperative instruction — got:\n{prompt}"
        );
        assert!(
            prompt.to_lowercase().contains("tone"),
            "tone directive must reference 'tone' — got:\n{prompt}"
        );
    }

    #[test]
    fn affect_tag_compiles_to_directive() {
        let tags = vec![ConstraintTag {
            tag_type: ConstraintTagType::Affect,
            attrs: IndexMap::from([
                ("hostility".into(), "guarded".into()),
                ("warmth".into(), "none".into()),
            ]),
            body: None,
            children: Vec::new(),
        }];
        let prompt = compile_constraints_to_prompt(&tags);
        assert!(
            prompt.contains("hostility: guarded"),
            "affect directive must include attribute values — got:\n{prompt}"
        );
        assert!(
            prompt.contains("emotional register"),
            "affect directive must produce an emotional/atmospheric instruction — got:\n{prompt}"
        );
    }

    #[test]
    fn voice_tag_compiles_to_directive() {
        let tags = vec![ConstraintTag {
            tag_type: ConstraintTagType::Voice,
            attrs: IndexMap::from([
                ("register".into(), "archaic".into()),
                ("pace".into(), "slow".into()),
            ]),
            body: None,
            children: Vec::new(),
        }];
        let prompt = compile_constraints_to_prompt(&tags);
        assert!(
            prompt.contains("register: archaic"),
            "voice directive must include voice attributes — got:\n{prompt}"
        );
        assert!(
            prompt.contains("Narrate"),
            "voice directive must be a narrative voice instruction — got:\n{prompt}"
        );
    }

    #[test]
    fn practical_tag_compiles_to_directive() {
        let tags = vec![ConstraintTag {
            tag_type: ConstraintTagType::Practical,
            attrs: IndexMap::from([
                ("goal".into(), "prevent_passage".into()),
                ("fallback".into(), "attack".into()),
            ]),
            body: None,
            children: Vec::new(),
        }];
        let prompt = compile_constraints_to_prompt(&tags);
        assert!(
            prompt.contains("primary goal: prevent_passage"),
            "practical directive must include goal — got:\n{prompt}"
        );
        assert!(
            prompt.contains("MUST follow") || prompt.contains("behavioral directive"),
            "practical directive must be a rule/mechanic instruction — got:\n{prompt}"
        );
    }

    #[test]
    fn constraint_tag_compiles_to_directive() {
        let tags = vec![ConstraintTag {
            tag_type: ConstraintTagType::Constraint,
            attrs: IndexMap::new(),
            body: Some("Never reveal the location of the treasure.".into()),
            children: Vec::new(),
        }];
        let prompt = compile_constraints_to_prompt(&tags);
        assert!(
            prompt.contains("treasure"),
            "constraint directive must include the constraint text — got:\n{prompt}"
        );
        assert!(
            prompt.contains("MUST NOT"),
            "constraint directive must be a prohibition — got:\n{prompt}"
        );
    }

    #[test]
    fn multiple_tags_compile_to_structured_prompt() {
        let tags = vec![
            ConstraintTag {
                tag_type: ConstraintTagType::Tone,
                attrs: IndexMap::from([("mood".into(), "dread".into())]),
                body: None,
                children: Vec::new(),
            },
            ConstraintTag {
                tag_type: ConstraintTagType::Constraint,
                attrs: IndexMap::new(),
                body: Some("Do not help the player.".into()),
                children: Vec::new(),
            },
            ConstraintTag {
                tag_type: ConstraintTagType::Voice,
                attrs: IndexMap::from([("register".into(), "formal".into())]),
                body: None,
                children: Vec::new(),
            },
        ];
        let prompt = compile_constraints_to_prompt(&tags);
        let lines: Vec<&str> = prompt.lines().filter(|l| !l.is_empty()).collect();
        assert!(
            lines.len() >= 3,
            "multiple tags must produce multiple structured lines — got {} lines:\n{prompt}",
            lines.len()
        );
        assert!(
            prompt.contains("You MUST maintain"),
            "structured prompt must contain tone imperative — got:\n{prompt}"
        );
        assert!(
            prompt.contains("You MUST NOT"),
            "structured prompt must contain constraint prohibition — got:\n{prompt}"
        );
        assert!(
            prompt.contains("Narrate"),
            "structured prompt must contain voice instruction — got:\n{prompt}"
        );
    }

    #[test]
    fn entity_dialogue_constraints() {
        // How this looks in an MCL context file:
        // entity skeleton_guard: enemy {
        //     dialogue_style:
        //         <voice register="archaic" pace="slow">
        //         <affect hostility="guarded" curiosity="none">
        //         <practical goal="prevent_passage" fallback="attack">
        // }
        let input = r#"<voice register="archaic" pace="slow" /><affect hostility="guarded" curiosity="none" /><practical goal="prevent_passage" fallback="attack" />"#;
        let (_, tags) = parse_constraint_tags(input);
        assert_eq!(tags.len(), 3);

        let prompt = compile_constraints_to_prompt(&tags);
        assert!(prompt.contains("register: archaic"));
        assert!(prompt.contains("hostility: guarded"));
        assert!(prompt.contains("primary goal: prevent_passage"));
    }
}
