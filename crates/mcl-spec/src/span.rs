use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    pub offset: usize,
    pub len: usize,
    pub line: u32,
    pub col: u32,
}

impl Span {
    pub fn new(offset: usize, len: usize, line: u32, col: u32) -> Self {
        Self {
            offset,
            len,
            line,
            col,
        }
    }

    pub fn empty() -> Self {
        Self {
            offset: 0,
            len: 0,
            line: 0,
            col: 0,
        }
    }

    pub fn merge(self, other: Span) -> Span {
        let start = self.offset.min(other.offset);
        let end = (self.offset + self.len).max(other.offset + other.len);
        Span {
            offset: start,
            len: end - start,
            line: self.line.min(other.line),
            col: if self.line <= other.line {
                self.col
            } else {
                other.col
            },
        }
    }
}

impl std::fmt::Display for Span {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.line, self.col)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn span_display() {
        let span = Span::new(10, 5, 3, 7);
        assert_eq!(span.to_string(), "3:7");
    }

    #[test]
    fn span_empty() {
        let s = Span::empty();
        assert_eq!(s.offset, 0);
        assert_eq!(s.len, 0);
        assert_eq!(s.line, 0);
        assert_eq!(s.col, 0);
    }

    #[test]
    fn span_merge() {
        let a = Span::new(5, 3, 1, 5);
        let b = Span::new(10, 4, 2, 1);
        let merged = a.merge(b);
        assert_eq!(merged.offset, 5);
        assert_eq!(merged.len, 9);
        assert_eq!(merged.line, 1);
        assert_eq!(merged.col, 5);
    }

    #[test]
    fn span_serde_round_trip() {
        let span = Span::new(42, 10, 5, 3);
        let json = serde_json::to_string(&span).unwrap();
        let back: Span = serde_json::from_str(&json).unwrap();
        assert_eq!(span, back);
    }
}
