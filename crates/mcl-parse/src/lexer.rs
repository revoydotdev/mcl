use mcl_spec::Span;

#[derive(Debug, Clone, PartialEq)]
pub enum LineKind {
    /// `---` frontmatter delimiter
    FrontmatterDelim,
    /// `# SectionName`
    SectionHeader(String),
    /// `key: value`
    KeyValue(String, String),
    /// `- name: rest` or `- name`
    ListItem(String, Option<String>),
    /// Indented continuation line (indent level in spaces, content)
    Indented(u32, String),
    /// Empty or whitespace-only
    Blank,
    /// `// comment` or `# comment` (only when not a section header)
    Comment(String),
}

#[derive(Debug, Clone)]
pub struct Line {
    pub kind: LineKind,
    pub span: Span,
    pub raw: String,
}

/// Section names we recognize as structural headers
const KNOWN_SECTIONS: &[&str] = &[
    "State",
    "Entities",
    "Items",
    "Actions",
    "Transitions",
    "Narrator",
    "Knowledge",
    "Constraints",
    "Includes",
    "Sources",
    "Labels",
    "Degrade",
    "Policies",
];

fn is_section_header(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if !trimmed.starts_with("# ") {
        return None;
    }
    let rest = trimmed[2..].trim();
    // Plain: "# State" — rest is exactly a known name
    if KNOWN_SECTIONS.iter().any(|s| s.eq_ignore_ascii_case(rest)) {
        return Some(rest.to_string());
    }
    // Decorated: "# ── State ────" — non-alphabetic chars wrap the name.
    // Must NOT match prose like "# The narrator must never..." where a known
    // word appears mid-sentence.
    let first_alpha = rest
        .split(|c: char| !c.is_ascii_alphabetic())
        .find(|s| !s.is_empty())?;
    if !KNOWN_SECTIONS
        .iter()
        .any(|s| s.eq_ignore_ascii_case(first_alpha))
    {
        return None;
    }
    // Reject if the prefix before the section name is only spaces (prose, not decoration).
    // Decorated headers have non-space, non-alpha chars before the name (e.g. "── ").
    let idx = rest.find(first_alpha)?;
    let prefix = &rest[..idx];
    let has_decoration = prefix.chars().any(|c| !c.is_ascii_whitespace());
    if !has_decoration {
        return None;
    }
    Some(first_alpha.to_string())
}

pub fn lex(input: &str) -> Vec<Line> {
    let mut lines = Vec::new();
    let mut offset = 0;

    for (line_num, raw_line) in input.lines().enumerate() {
        let line_num = line_num as u32 + 1;
        let span = Span::new(offset, raw_line.len(), line_num, 1);
        let raw = raw_line.to_string();

        let trimmed = raw_line.trim();

        let kind = if trimmed == "---" {
            LineKind::FrontmatterDelim
        } else if trimmed.is_empty() {
            LineKind::Blank
        } else if let Some(comment) = trimmed.strip_prefix("//") {
            LineKind::Comment(comment.trim().to_string())
        } else if let Some(name) = is_section_header(trimmed) {
            LineKind::SectionHeader(name)
        } else if trimmed.starts_with("- ") {
            parse_list_item(trimmed)
        } else {
            let indent = raw_line.len() - raw_line.trim_start().len();
            if indent > 0 {
                LineKind::Indented(indent as u32, trimmed.to_string())
            } else if let Some((key, val)) = split_key_value(trimmed) {
                LineKind::KeyValue(key, val)
            } else {
                // Treat as indented content at level 0
                LineKind::Indented(0, trimmed.to_string())
            }
        };

        lines.push(Line { kind, span, raw });
        offset += raw_line.len() + 1; // +1 for newline
    }

    lines
}

fn parse_list_item(trimmed: &str) -> LineKind {
    let content = &trimmed[2..]; // strip "- "
    if let Some((name, rest)) = split_key_value(content) {
        if rest.is_empty() {
            LineKind::ListItem(name, None)
        } else {
            LineKind::ListItem(name, Some(rest))
        }
    } else {
        // bare list item like `- fact_name`
        LineKind::ListItem(content.trim().to_string(), None)
    }
}

fn split_key_value(s: &str) -> Option<(String, String)> {
    // Find first `:` not inside quotes or braces
    let mut in_quotes = false;
    let mut brace_depth = 0;
    let mut bracket_depth = 0;

    for (i, ch) in s.char_indices() {
        match ch {
            '"' => in_quotes = !in_quotes,
            '{' if !in_quotes => brace_depth += 1,
            '}' if !in_quotes => brace_depth -= 1,
            '[' if !in_quotes => bracket_depth += 1,
            ']' if !in_quotes => bracket_depth -= 1,
            ':' if !in_quotes && brace_depth == 0 && bracket_depth == 0 => {
                let key = s[..i].trim();
                if key.is_empty() || !is_valid_key(key) {
                    return None;
                }
                let val = s[i + 1..].trim();
                let val = strip_inline_comment(val);
                return Some((key.to_string(), val.to_string()));
            }
            _ => {}
        }
    }
    None
}

fn is_valid_key(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
}

fn strip_inline_comment(s: &str) -> &str {
    let mut in_quotes = false;
    let mut brace_depth = 0u32;
    let mut bracket_depth = 0u32;
    for (i, ch) in s.char_indices() {
        match ch {
            '"' => in_quotes = !in_quotes,
            '{' if !in_quotes => brace_depth += 1,
            '}' if !in_quotes => brace_depth = brace_depth.saturating_sub(1),
            '[' if !in_quotes => bracket_depth += 1,
            ']' if !in_quotes => bracket_depth = bracket_depth.saturating_sub(1),
            '#' if !in_quotes && brace_depth == 0 && bracket_depth == 0 => {
                return s[..i].trim_end();
            }
            _ => {}
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lex_frontmatter() {
        let lines = lex("---\ncontext: foo\ntype: room\n---");
        assert_eq!(lines.len(), 4);
        assert_eq!(lines[0].kind, LineKind::FrontmatterDelim);
        assert!(matches!(&lines[1].kind, LineKind::KeyValue(k, v) if k == "context" && v == "foo"));
        assert_eq!(lines[3].kind, LineKind::FrontmatterDelim);
    }

    #[test]
    fn lex_section_header() {
        let lines = lex("# State\nlit: false");
        assert!(matches!(&lines[0].kind, LineKind::SectionHeader(n) if n == "State"));
        assert!(matches!(&lines[1].kind, LineKind::KeyValue(k, v) if k == "lit" && v == "false"));
    }

    #[test]
    fn lex_list_items() {
        let lines = lex("- skeleton_guard:\n    type: enemy\n- rusty_key: { on: floor }");
        assert!(matches!(&lines[0].kind, LineKind::ListItem(n, None) if n == "skeleton_guard"));
        assert!(matches!(&lines[1].kind, LineKind::Indented(4, ref s) if s == "type: enemy"));
        assert!(
            matches!(&lines[2].kind, LineKind::ListItem(n, Some(v)) if n == "rusty_key" && v == "{ on: floor }")
        );
    }
}
