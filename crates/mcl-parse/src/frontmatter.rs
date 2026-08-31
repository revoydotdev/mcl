use indexmap::IndexMap;
use mcl_spec::document::{ContextType, Frontmatter};
use mcl_spec::{MclError, Span};

use crate::inline::parse_value;
use crate::lexer::{Line, LineKind};

/// Parse frontmatter from the line stream. Expects:
/// - lines[start] is FrontmatterDelim
/// - key: value pairs follow
/// - lines[end] is FrontmatterDelim
///
/// Returns (Frontmatter, index of line after closing delim)
pub fn parse_frontmatter(lines: &[Line], start: usize) -> Result<(Frontmatter, usize), MclError> {
    if start >= lines.len() || lines[start].kind != LineKind::FrontmatterDelim {
        return Err(MclError::parse(
            "expected `---` frontmatter delimiter",
            lines.first().map_or(Span::empty(), |l| l.span),
        ));
    }

    let fm_start_span = lines[start].span;
    let mut pos = start + 1;
    let mut kv: IndexMap<String, (String, Span)> = IndexMap::new();

    while pos < lines.len() {
        match &lines[pos].kind {
            LineKind::FrontmatterDelim => {
                pos += 1;
                break;
            }
            LineKind::KeyValue(key, val) => {
                kv.insert(key.clone(), (val.clone(), lines[pos].span));
                pos += 1;
            }
            LineKind::Blank | LineKind::Comment(_) => {
                pos += 1;
            }
            _ => {
                return Err(MclError::parse(
                    format!(
                        "unexpected line in frontmatter: `{}`",
                        lines[pos].raw.trim()
                    ),
                    lines[pos].span,
                ));
            }
        }
    }

    // Extract required fields
    let context = kv.get("context").map(|(v, _)| v.clone()).ok_or_else(|| {
        MclError::parse(
            "frontmatter missing required `context` field",
            fm_start_span,
        )
    })?;

    let type_str = kv.get("type").map(|(v, _)| v.clone()).ok_or_else(|| {
        MclError::parse("frontmatter missing required `type` field", fm_start_span)
    })?;

    let context_type = type_str.parse::<ContextType>().map_err(|e| {
        let span = kv.get("type").map_or(fm_start_span, |(_, s)| *s);
        MclError::parse(e, span)
    })?;

    // Optional fields
    let domain = kv.get("domain").map(|(v, _)| v.clone());
    let version = kv.get("version").and_then(|(v, _)| v.parse::<u32>().ok());
    let extends = kv.get("extends").map(|(v, _)| v.clone());

    let tags = if let Some((val, span)) = kv.get("tags") {
        if val.starts_with('[') {
            crate::inline::parse_string_list(val, *span)?
        } else {
            vec![val.clone()]
        }
    } else {
        Vec::new()
    };

    // Collect remaining keys as extra
    let known_keys = ["context", "type", "domain", "version", "tags", "extends"];
    let mut extra = IndexMap::new();
    for (key, (val, span)) in &kv {
        if !known_keys.contains(&key.as_str()) {
            extra.insert(key.clone(), parse_value(val, *span)?);
        }
    }

    Ok((
        Frontmatter {
            context,
            context_type,
            domain,
            version,
            tags,
            extends,
            extra,
            span: fm_start_span,
        },
        pos,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::lex;

    #[test]
    fn parse_basic_frontmatter() {
        let lines = lex("---\ncontext: crypt_antechamber\ntype: room\ndomain: rpg\n---");
        let (fm, end) = parse_frontmatter(&lines, 0).unwrap();
        assert_eq!(fm.context, "crypt_antechamber");
        assert_eq!(fm.context_type, ContextType::Room);
        assert_eq!(fm.domain, Some("rpg".to_string()));
        assert_eq!(end, 5);
    }

    #[test]
    fn parse_frontmatter_with_tags() {
        let lines = lex("---\ncontext: test\ntype: scene\ntags: [dungeon, dark]\n---");
        let (fm, _) = parse_frontmatter(&lines, 0).unwrap();
        assert_eq!(fm.tags, vec!["dungeon", "dark"]);
    }

    #[test]
    fn missing_context_errors() {
        let lines = lex("---\ntype: room\n---");
        assert!(parse_frontmatter(&lines, 0).is_err());
    }
}
