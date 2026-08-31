pub fn normalize_response(text: &str) -> String {
    let text = strip_thinking_tags(text);
    let text = text.trim();
    collapse_newlines(text)
}

fn strip_thinking_tags(text: &str) -> String {
    let open = "<thinking>";
    let close = "</thinking>";
    let mut result = String::with_capacity(text.len());
    let mut chars = text.char_indices().peekable();

    while let Some(&(i, _)) = chars.peek() {
        if text[i..].starts_with(open) {
            let remaining = &text[i + open.len()..];
            if let Some(end_offset) = remaining.find(close) {
                for _ in 0..(open.len() + end_offset + close.len()) {
                    chars.next();
                }
            } else {
                result.push_str(&text[i..]);
                break;
            }
        } else {
            // SAFETY: peek() returned Some so next() must also return Some
            let Some((_, ch)) = chars.next() else {
                break;
            };
            result.push(ch);
        }
    }

    result
}

fn collapse_newlines(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut newline_count: usize = 0;

    for ch in text.chars() {
        if ch == '\n' {
            newline_count += 1;
            if newline_count <= 2 {
                result.push(ch);
            }
        } else {
            newline_count = 0;
            result.push(ch);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_whitespace() {
        assert_eq!(normalize_response("  hello  "), "hello");
    }

    #[test]
    fn strips_thinking_tags() {
        let input = "<thinking>I need to plan this</thinking>Here is the answer";
        assert_eq!(normalize_response(input), "Here is the answer");
    }

    #[test]
    fn strips_multiline_thinking_tags() {
        let input = "<thinking>\nline1\nline2\n</thinking>Actual response";
        assert_eq!(normalize_response(input), "Actual response");
    }

    #[test]
    fn handles_no_thinking_tags() {
        assert_eq!(normalize_response("plain response"), "plain response");
    }

    #[test]
    fn handles_empty_string() {
        assert_eq!(normalize_response(""), "");
    }

    #[test]
    fn handles_only_thinking_tags() {
        assert_eq!(normalize_response("<thinking>hmm</thinking>"), "");
    }

    #[test]
    fn collapses_excessive_newlines() {
        let input = "line1\n\n\n\nline2";
        assert_eq!(normalize_response(input), "line1\n\nline2");
    }

    #[test]
    fn handles_unclosed_thinking_tag() {
        let input = "<thinking>still thinking";
        assert_eq!(normalize_response(input), "<thinking>still thinking");
    }

    #[test]
    fn handles_multiple_thinking_blocks() {
        let input = "<thinking>a</thinking>mid<thinking>b</thinking>end";
        assert_eq!(normalize_response(input), "midend");
    }
}
