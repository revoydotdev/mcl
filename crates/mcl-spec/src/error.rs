use crate::span::Span;

const CONTEXT_TYPE_CANDIDATES: &[&str] = &["room", "scene", "step", "dialogue", "encounter"];
const SUGGESTION_THRESHOLD: usize = 2;

#[derive(Debug, Clone, thiserror::Error)]
pub enum MclError {
    #[error("{span}: parse error: {message}")]
    Parse { message: String, span: Span },

    #[error("{span}: validation error: {message}")]
    Validation { message: String, span: Span },

    #[error("signal parse error at offset {offset}: {message}")]
    SignalParse { message: String, offset: usize },

    #[error("state error: {0}")]
    State(String),

    #[error("resolver error: {0}")]
    Resolver(String),

    #[error("context not found: {0}")]
    ContextNotFound(String),

    #[error("transition error: {0}")]
    Transition(String),

    #[error("llm error: {0}")]
    Llm(String),

    #[error("io error: {0}")]
    Io(String),
}

impl MclError {
    pub fn parse(message: impl Into<String>, span: Span) -> Self {
        Self::Parse {
            message: message.into(),
            span,
        }
    }

    pub fn validation(message: impl Into<String>, span: Span) -> Self {
        Self::Validation {
            message: message.into(),
            span,
        }
    }

    pub fn signal(message: impl Into<String>, offset: usize) -> Self {
        Self::SignalParse {
            message: message.into(),
            offset,
        }
    }
}

impl From<std::io::Error> for MclError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e.to_string())
    }
}

fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut matrix = vec![vec![0; b.len() + 1]; a.len() + 1];

    for (i, row) in matrix.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in matrix[0].iter_mut().enumerate().take(b.len() + 1) {
        *cell = j;
    }
    for (i, ca) in a.iter().enumerate() {
        for (j, cb) in b.iter().enumerate() {
            let cost = if ca == cb { 0 } else { 1 };
            matrix[i + 1][j + 1] = (matrix[i][j + 1] + 1)
                .min(matrix[i + 1][j] + 1)
                .min(matrix[i][j] + cost);
        }
    }

    matrix[a.len()][b.len()]
}

pub(crate) fn suggest(input: &str, candidates: &[&'static str]) -> Option<&'static str> {
    let input_lower = input.to_lowercase();
    let mut best: Option<(&str, usize)> = None;

    for &c in candidates {
        let dist = levenshtein(&input_lower, c);
        if dist == 0 {
            return None;
        }
        if dist <= SUGGESTION_THRESHOLD {
            best = match best {
                None => Some((c, dist)),
                Some((_, prev_dist)) if dist < prev_dist => Some((c, dist)),
                _ => best,
            };
        }
    }

    best.map(|(s, _)| s)
}

pub(crate) fn suggest_context_type(input: &str) -> Option<&'static str> {
    suggest(input, CONTEXT_TYPE_CANDIDATES)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_error_display() {
        let span = Span::new(10, 5, 2, 3);
        let err = MclError::parse("unexpected token", span);
        let s = err.to_string();
        assert!(s.contains("parse error"));
        assert!(s.contains("unexpected token"));
        assert!(s.contains("2:3"));
    }

    #[test]
    fn validation_error_display() {
        let span = Span::new(0, 1, 1, 1);
        let err = MclError::validation("field required", span);
        let s = err.to_string();
        assert!(s.contains("validation error"));
        assert!(s.contains("field required"));
    }

    #[test]
    fn signal_parse_error_display() {
        let err = MclError::signal("bad signal", 42);
        let s = err.to_string();
        assert!(s.contains("signal parse error"));
        assert!(s.contains("offset 42"));
        assert!(s.contains("bad signal"));
    }

    #[test]
    fn state_error_display() {
        let err = MclError::State("invalid state".into());
        assert_eq!(err.to_string(), "state error: invalid state");
    }

    #[test]
    fn context_not_found_display() {
        let err = MclError::ContextNotFound("dungeon_5".into());
        assert_eq!(err.to_string(), "context not found: dungeon_5");
    }

    #[test]
    fn llm_error_display() {
        let err = MclError::Llm("timeout".into());
        assert_eq!(err.to_string(), "llm error: timeout");
    }

    #[test]
    fn error_reports_line_and_column() {
        use crate::document::ContextType;
        let result = "romo".parse::<ContextType>();
        assert!(result.is_err());
        let err_msg = result.unwrap_err();
        assert!(
            err_msg.contains("room"),
            "error should list valid types including 'room': {err_msg}"
        );
    }

    #[test]
    fn error_suggests_fix_for_common_mistakes() {
        let candidates = ["room", "scene", "step", "dialogue", "encounter"];

        assert_eq!(suggest("romo", &candidates), Some("room"));
        assert_eq!(suggest("scen", &candidates), Some("scene"));
        assert_eq!(suggest("dialogeu", &candidates), Some("dialogue"));
        assert!(suggest("room", &candidates).is_none());
        assert!(suggest("banana", &candidates).is_none());
    }
}
