//! Pratt expression parser for the MCL policy DSL (R2 §1.3).
//!
//! Parses expression text into `PolicyExpr` AST nodes defined in
//! `mcl_spec::policy`. Uses a top-down operator precedence (Pratt) parser
//! with its own mini-tokenizer — no external dependencies.

use mcl_spec::policy::{BinOp, Lit, PolicyExpr, UnOp};
use mcl_spec::{MclError, Span};

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Parse an expression string into a `PolicyExpr` AST.
pub fn parse_expression(input: &str) -> Result<PolicyExpr, MclError> {
    let mut parser = ExprParser::new(input);
    let expr = parser.parse_expr(0)?;
    parser.expect_end()?;
    Ok(expr)
}

// ---------------------------------------------------------------------------
// Token types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum Token {
    // Literals
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),

    // Identifiers & paths
    Ident(String),

    // Operators
    Plus,
    Minus,
    Star,
    Slash,
    Eq,
    Neq,
    Lt,
    Lte,
    Gt,
    Gte,
    And,
    Or,
    Not,
    Dot,

    // Punctuation
    LParen,
    RParen,
    Comma,
    Assign, // '='

    // Keywords
    Let,
    In,
}

// ---------------------------------------------------------------------------
// Tokenizer
// ---------------------------------------------------------------------------

struct Tokenizer<'a> {
    input: &'a str,
    pos: usize,
    line: u32,
    col: u32,
}

impl<'a> Tokenizer<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            input,
            pos: 0,
            line: 1,
            col: 1,
        }
    }

    fn span(&self) -> Span {
        Span::new(self.pos, 1, self.line, self.col)
    }

    fn skip_whitespace(&mut self) {
        while let Some(ch) = self.peek_char() {
            if ch == ' ' || ch == '\t' || ch == '\n' || ch == '\r' {
                if ch == '\n' {
                    self.line += 1;
                    self.col = 1;
                } else {
                    self.col += 1;
                }
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn peek_char(&self) -> Option<char> {
        self.input[self.pos..].chars().next()
    }

    fn advance_char(&mut self) -> Option<char> {
        let ch = self.peek_char()?;
        let len = ch.len_utf8();
        self.pos += len;
        self.col += 1;
        Some(ch)
    }

    fn peek_word(&self) -> &str {
        let rest = &self.input[self.pos..];
        let end = rest
            .char_indices()
            .take_while(|(_, ch)| ch.is_ascii_alphanumeric() || *ch == '_')
            .last()
            .map(|(i, ch)| i + ch.len_utf8())
            .unwrap_or(0);
        &rest[..end]
    }

    fn next_token(&mut self) -> Result<Option<(Token, Span)>, MclError> {
        self.skip_whitespace();

        if self.pos >= self.input.len() {
            return Ok(None);
        }

        let start_span = self.span();
        let ch = match self.peek_char() {
            Some(c) => c,
            None => return Ok(None),
        };

        // Single-char operators / punctuation
        let tok = match ch {
            '+' => Token::Plus,
            '-' => Token::Minus,
            '*' => Token::Star,
            '/' => Token::Slash,
            '<' => {
                self.advance_char();
                if self.peek_char() == Some('=') {
                    self.advance_char();
                    return Ok(Some((Token::Lte, start_span)));
                }
                return Ok(Some((Token::Lt, start_span)));
            }
            '>' => {
                self.advance_char();
                if self.peek_char() == Some('=') {
                    self.advance_char();
                    return Ok(Some((Token::Gte, start_span)));
                }
                return Ok(Some((Token::Gt, start_span)));
            }
            '=' => {
                self.advance_char();
                if self.peek_char() == Some('=') {
                    self.advance_char();
                    return Ok(Some((Token::Eq, start_span)));
                }
                return Ok(Some((Token::Assign, start_span)));
            }
            '!' => {
                self.advance_char();
                if self.peek_char() == Some('=') {
                    self.advance_char();
                    return Ok(Some((Token::Neq, start_span)));
                }
                return Ok(Some((Token::Not, start_span)));
            }
            '(' => Token::LParen,
            ')' => Token::RParen,
            ',' => Token::Comma,
            '.' => Token::Dot,
            _ => {
                // Number literals
                if ch.is_ascii_digit() {
                    return Ok(Some(self.read_number(start_span)?));
                }

                // String literals (double or single quoted)
                if ch == '"' || ch == '\'' {
                    return Ok(Some(self.read_string(start_span)?));
                }

                // Identifiers and keywords
                if ch.is_ascii_alphabetic() || ch == '_' {
                    return Ok(Some(self.read_ident(start_span)?));
                }

                return Err(MclError::parse(
                    format!("unexpected character `{ch}` in expression"),
                    start_span,
                ));
            }
        };

        self.advance_char();
        Ok(Some((tok, start_span)))
    }

    fn read_number(&mut self, start_span: Span) -> Result<(Token, Span), MclError> {
        let start = self.pos;
        while let Some(ch) = self.peek_char() {
            if ch.is_ascii_digit() || ch == '.' {
                self.advance_char();
            } else {
                break;
            }
        }
        let text = &self.input[start..self.pos];

        // Check for trailing dots that aren't part of the number (e.g., path separator)
        // A number ends with digits, not a trailing dot
        let trimmed = text.trim_end_matches('.');
        let trailing_dots = text.len() - trimmed.len();

        if trailing_dots > 0 {
            // Put back the dots — they're path separators
            self.pos = start + trimmed.len();
            for _ in 0..trailing_dots {
                // col was already advanced, we'll adjust by not advancing further
            }
        }

        let num_text = if trailing_dots > 0 { trimmed } else { text };

        if let Ok(n) = num_text.parse::<i64>() {
            return Ok((Token::Int(n), start_span));
        }
        if let Ok(n) = num_text.parse::<f64>() {
            return Ok((Token::Float(n), start_span));
        }
        Err(MclError::parse(
            format!("invalid number literal `{num_text}`"),
            start_span,
        ))
    }

    fn read_string(&mut self, start_span: Span) -> Result<(Token, Span), MclError> {
        let quote = self.advance_char(); // consume opening quote
        let quote = quote.unwrap_or('"');
        let mut buf = String::new();

        loop {
            match self.advance_char() {
                None => return Err(MclError::parse("unterminated string literal", start_span)),
                Some(c) if c == quote => break,
                Some('\\') => {
                    let escaped = match self.advance_char() {
                        None => {
                            return Err(MclError::parse(
                                "unterminated escape in string",
                                start_span,
                            ))
                        }
                        Some('n') => '\n',
                        Some('t') => '\t',
                        Some('r') => '\r',
                        Some('\\') => '\\',
                        Some('"') => '"',
                        Some('\'') => '\'',
                        Some(other) => other,
                    };
                    buf.push(escaped);
                }
                Some(c) => buf.push(c),
            }
        }

        Ok((Token::Str(buf), start_span))
    }

    fn read_ident(&mut self, start_span: Span) -> Result<(Token, Span), MclError> {
        let word = self.peek_word().to_string();
        for _ in word.chars() {
            self.advance_char();
        }

        let tok = match word.as_str() {
            "true" => Token::Bool(true),
            "false" => Token::Bool(false),
            "and" => Token::And,
            "or" => Token::Or,
            "not" => Token::Not,
            "let" => Token::Let,
            "in" => Token::In,
            _ => Token::Ident(word.to_string()),
        };
        Ok((tok, start_span))
    }
}

// ---------------------------------------------------------------------------
// Pratt parser
// ---------------------------------------------------------------------------

struct ExprParser<'a> {
    tokenizer: Tokenizer<'a>,
    current: Option<(Token, Span)>,
    #[allow(dead_code)]
    input: &'a str,
    error: Option<MclError>,
}

impl<'a> ExprParser<'a> {
    fn new(input: &'a str) -> Self {
        let mut tokenizer = Tokenizer::new(input);
        let current = match tokenizer.next_token() {
            Ok(tok) => tok,
            Err(e) => {
                return Self {
                    tokenizer,
                    current: None,
                    input,
                    error: Some(e),
                };
            }
        };
        Self {
            tokenizer,
            current,
            input,
            error: None,
        }
    }

    fn advance(&mut self) -> Result<Option<(Token, Span)>, MclError> {
        if let Some(e) = self.error.take() {
            return Err(e);
        }
        let prev = self.current.take();
        self.current = self.tokenizer.next_token()?;
        Ok(prev)
    }

    fn peek_token(&self) -> Option<&Token> {
        self.current.as_ref().map(|(t, _)| t)
    }

    fn expect(&mut self, expected: &Token) -> Result<(), MclError> {
        match self.advance()? {
            Some((ref tok, _span)) if tok == expected => Ok(()),
            Some((tok, span)) => Err(MclError::parse(
                format!("expected {:?}, got {:?}", expected, tok),
                span,
            )),
            None => Err(MclError::parse(
                format!("expected {:?}, got end of input", expected),
                Span::empty(),
            )),
        }
    }

    fn expect_end(&mut self) -> Result<(), MclError> {
        if let Some((tok, span)) = self.current.take() {
            return Err(MclError::parse(
                format!("unexpected token after expression: {:?}", tok),
                span,
            ));
        }
        Ok(())
    }

    // ----- Pratt core -----

    /// Pratt binding power. Returns `(left_bp, right_bp)`.
    fn infix_bp(tok: &Token) -> Option<(u8, u8)> {
        match tok {
            Token::Or => Some((1, 2)),
            Token::And => Some((3, 4)),
            Token::Eq | Token::Neq | Token::Lt | Token::Lte | Token::Gt | Token::Gte => {
                Some((5, 6))
            }
            Token::Plus | Token::Minus => Some((7, 8)),
            Token::Star | Token::Slash => Some((9, 10)),
            _ => None,
        }
    }

    /// Prefix binding power.
    fn prefix_bp(tok: &Token) -> Option<u8> {
        match tok {
            Token::Not => Some(11),
            Token::Minus => Some(11),
            _ => None,
        }
    }

    /// Postfix binding power (call).
    fn postfix_bp(tok: &Token) -> Option<u8> {
        match tok {
            Token::LParen => Some(12),
            _ => None,
        }
    }

    /// Parse an expression with minimum binding power `min_bp`.
    fn parse_expr(&mut self, min_bp: u8) -> Result<PolicyExpr, MclError> {
        // Parse the left side (nud — null denotation)
        let mut left = self.nud()?;

        // Parse infix/postfix operators while binding power allows
        loop {
            let Some((tok, span)) = self.current.clone() else {
                break;
            };

            // Check postfix (call)
            if let Some(right_bp) = Self::postfix_bp(&tok) {
                if right_bp > min_bp {
                    self.advance()?;
                    left = self.postfix(left, &tok, span)?;
                    continue;
                }
            }

            // Check infix
            if let Some((left_bp, right_bp)) = Self::infix_bp(&tok) {
                if left_bp >= min_bp {
                    self.advance()?;
                    let right = self.parse_expr(right_bp)?;
                    left = PolicyExpr::BinaryOp {
                        op: self.token_to_binop(&tok)?,
                        left: Box::new(left),
                        right: Box::new(right),
                    };
                    continue;
                }
            }

            // Check path continuation (dot)
            if matches!(tok, Token::Dot) {
                // Dot has bp 12 to bind tighter than operators but looser than call
                let dot_bp: u8 = 12;
                if dot_bp > min_bp {
                    self.advance()?;
                    left = self.parse_path_continuation(left)?;
                    continue;
                }
            }

            break;
        }

        Ok(left)
    }

    /// Null denotation — parse a primary expression.
    fn nud(&mut self) -> Result<PolicyExpr, MclError> {
        if let Some(e) = self.error.take() {
            return Err(e);
        }

        let Some((tok, span)) = self.current.clone() else {
            return Err(MclError::parse(
                "expected expression, got end of input",
                Span::empty(),
            ));
        };

        // Prefix operators (not, -)
        if let Some(bp) = Self::prefix_bp(&tok) {
            self.advance()?;
            let op = match tok {
                Token::Not => UnOp::Not,
                Token::Minus => UnOp::Neg,
                _ => unreachable!("prefix_bp only returns for Not/Minus"),
            };
            let operand = self.parse_expr(bp)?;
            return Ok(PolicyExpr::UnaryOp {
                op,
                operand: Box::new(operand),
            });
        }

        // Parenthesized expression
        if matches!(tok, Token::LParen) {
            self.advance()?;
            let expr = self.parse_expr(0)?;
            self.expect(&Token::RParen)?;
            return Ok(expr);
        }

        // Let binding
        if matches!(tok, Token::Let) {
            return self.parse_let();
        }

        // Literals
        match &tok {
            Token::Bool(b) => {
                self.advance()?;
                return Ok(PolicyExpr::Literal {
                    value: Lit::Bool(*b),
                });
            }
            Token::Int(n) => {
                self.advance()?;
                return Ok(PolicyExpr::Literal {
                    value: Lit::Int(*n),
                });
            }
            Token::Float(f) => {
                self.advance()?;
                return Ok(PolicyExpr::Literal {
                    value: Lit::Float(*f),
                });
            }
            Token::Str(s) => {
                self.advance()?;
                return Ok(PolicyExpr::Literal {
                    value: Lit::Str(s.clone()),
                });
            }
            Token::Ident(_) => {
                // Could be a path, a function call, or a bare identifier
                let ident = match tok {
                    Token::Ident(s) => s.clone(),
                    _ => unreachable!(),
                };
                self.advance()?;

                // Check for dot (path start)
                if matches!(self.peek_token(), Some(Token::Dot)) {
                    let mut segments = vec![ident];
                    while matches!(self.peek_token(), Some(Token::Dot)) {
                        self.advance()?; // consume dot
                        match self.advance()? {
                            Some((Token::Ident(name), _)) => segments.push(name),
                            Some((other, s)) => {
                                return Err(MclError::parse(
                                    format!("expected identifier after '.', got {:?}", other),
                                    s,
                                ))
                            }
                            None => {
                                return Err(MclError::parse(
                                    "expected identifier after '.', got end of input",
                                    Span::empty(),
                                ))
                            }
                        }
                    }
                    return Ok(PolicyExpr::Path { segments });
                }

                // Check for call
                if matches!(self.peek_token(), Some(Token::LParen)) {
                    self.advance()?; // consume '('
                    let args = self.parse_arg_list()?;
                    return Ok(PolicyExpr::Call { func: ident, args });
                }

                // Bare identifier becomes a single-segment path
                return Ok(PolicyExpr::Path {
                    segments: vec![ident],
                });
            }
            _ => {}
        }

        Err(MclError::parse(
            format!("expected expression, got {:?}", tok),
            span,
        ))
    }

    /// Handle postfix operations (function calls).
    fn postfix(
        &mut self,
        left: PolicyExpr,
        _tok: &Token,
        _span: Span,
    ) -> Result<PolicyExpr, MclError> {
        let args = self.parse_arg_list()?;
        let func_name = match &left {
            PolicyExpr::Path { segments } => segments.join("."),
            PolicyExpr::Call { func, .. } => func.clone(),
            other => {
                return Err(MclError::parse(
                    format!("cannot call non-function expression: {:?}", other),
                    Span::empty(),
                ))
            }
        };
        Ok(PolicyExpr::Call {
            func: func_name,
            args,
        })
    }

    /// Parse path continuation after a dot (e.g., after a call or another path).
    fn parse_path_continuation(&mut self, _left: PolicyExpr) -> Result<PolicyExpr, MclError> {
        // After dot, expect an identifier, then potentially more dots
        match self.advance()? {
            Some((Token::Ident(name), _)) => {
                let mut segments = vec![name];
                while matches!(self.peek_token(), Some(Token::Dot)) {
                    self.advance()?; // consume dot
                    match self.advance()? {
                        Some((Token::Ident(name), _)) => segments.push(name),
                        Some((other, s)) => {
                            return Err(MclError::parse(
                                format!("expected identifier after '.', got {:?}", other),
                                s,
                            ))
                        }
                        None => {
                            return Err(MclError::parse(
                                "expected identifier after '.', got end of input",
                                Span::empty(),
                            ))
                        }
                    }
                }
                Ok(PolicyExpr::Path { segments })
            }
            Some((other, span)) => Err(MclError::parse(
                format!("expected identifier after '.', got {:?}", other),
                span,
            )),
            None => Err(MclError::parse(
                "expected identifier after '.', got end of input",
                Span::empty(),
            )),
        }
    }

    /// Parse `let IDENT = expr in expr`.
    fn parse_let(&mut self) -> Result<PolicyExpr, MclError> {
        self.advance()?; // consume 'let'

        let name = match self.advance()? {
            Some((Token::Ident(name), _)) => name,
            Some((tok, span)) => {
                return Err(MclError::parse(
                    format!("expected identifier after 'let', got {:?}", tok),
                    span,
                ))
            }
            None => {
                return Err(MclError::parse(
                    "expected identifier after 'let', got end of input",
                    Span::empty(),
                ))
            }
        };

        self.expect(&Token::Assign)?;

        let value = self.parse_expr(0)?;

        // expect 'in'
        match self.advance()? {
            Some((Token::In, _)) => {}
            Some((tok, span)) => {
                return Err(MclError::parse(
                    format!("expected 'in' in let expression, got {:?}", tok),
                    span,
                ))
            }
            None => {
                return Err(MclError::parse(
                    "expected 'in' in let expression, got end of input",
                    Span::empty(),
                ))
            }
        }

        let body = self.parse_expr(0)?;

        Ok(PolicyExpr::Let {
            name,
            value: Box::new(value),
            body: Box::new(body),
        })
    }

    /// Parse comma-separated argument list after '('.
    fn parse_arg_list(&mut self) -> Result<Vec<PolicyExpr>, MclError> {
        let mut args = Vec::new();

        // Check for empty args ')'
        if matches!(self.peek_token(), Some(Token::RParen)) {
            self.advance()?;
            return Ok(args);
        }

        loop {
            let arg = self.parse_expr(0)?;
            args.push(arg);

            match self.peek_token() {
                Some(Token::Comma) => {
                    self.advance()?;
                }
                Some(Token::RParen) => {
                    self.advance()?;
                    break;
                }
                Some(tok) => {
                    return Err(MclError::parse(
                        format!("expected ',' or ')' in argument list, got {:?}", tok),
                        Span::empty(),
                    ))
                }
                None => {
                    return Err(MclError::parse(
                        "unexpected end of input in argument list",
                        Span::empty(),
                    ))
                }
            }
        }

        Ok(args)
    }

    fn token_to_binop(&self, tok: &Token) -> Result<BinOp, MclError> {
        match tok {
            Token::Eq => Ok(BinOp::Eq),
            Token::Neq => Ok(BinOp::Neq),
            Token::Lt => Ok(BinOp::Lt),
            Token::Lte => Ok(BinOp::Lte),
            Token::Gt => Ok(BinOp::Gt),
            Token::Gte => Ok(BinOp::Gte),
            Token::Plus => Ok(BinOp::Add),
            Token::Minus => Ok(BinOp::Sub),
            Token::Star => Ok(BinOp::Mul),
            Token::Slash => Ok(BinOp::Div),
            Token::And => Ok(BinOp::And),
            Token::Or => Ok(BinOp::Or),
            _ => Err(MclError::parse(
                format!("not a binary operator: {:?}", tok),
                Span::empty(),
            )),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // Helper to build paths quickly
    fn path(segs: &[&str]) -> PolicyExpr {
        PolicyExpr::Path {
            segments: segs.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn call(func: &str, args: Vec<PolicyExpr>) -> PolicyExpr {
        PolicyExpr::Call {
            func: func.to_string(),
            args,
        }
    }

    fn lit_int(n: i64) -> PolicyExpr {
        PolicyExpr::Literal { value: Lit::Int(n) }
    }

    fn lit_bool(b: bool) -> PolicyExpr {
        PolicyExpr::Literal {
            value: Lit::Bool(b),
        }
    }

    fn lit_float(f: f64) -> PolicyExpr {
        PolicyExpr::Literal {
            value: Lit::Float(f),
        }
    }

    fn lit_str(s: &str) -> PolicyExpr {
        PolicyExpr::Literal {
            value: Lit::Str(s.to_string()),
        }
    }

    fn binop(op: BinOp, left: PolicyExpr, right: PolicyExpr) -> PolicyExpr {
        PolicyExpr::BinaryOp {
            op,
            left: Box::new(left),
            right: Box::new(right),
        }
    }

    fn unary(op: UnOp, operand: PolicyExpr) -> PolicyExpr {
        PolicyExpr::UnaryOp {
            op,
            operand: Box::new(operand),
        }
    }

    fn let_expr(name: &str, value: PolicyExpr, body: PolicyExpr) -> PolicyExpr {
        PolicyExpr::Let {
            name: name.to_string(),
            value: Box::new(value),
            body: Box::new(body),
        }
    }

    #[test]
    fn parse_expression_precedence() {
        // `a == 1 and b > 2 or c == 3`
        // Should parse as: or(and(eq(a,1), gt(b,2)), eq(c,3))
        let expr = parse_expression("a == 1 and b > 2 or c == 3").unwrap();
        let expected = binop(
            BinOp::Or,
            binop(
                BinOp::And,
                binop(BinOp::Eq, path(&["a"]), lit_int(1)),
                binop(BinOp::Gt, path(&["b"]), lit_int(2)),
            ),
            binop(BinOp::Eq, path(&["c"]), lit_int(3)),
        );
        assert_eq!(expr, expected);
    }

    #[test]
    fn parse_expression_nested_calls() {
        // `contains(prose, labels(secret))`
        let expr = parse_expression("contains(prose, labels(secret))").unwrap();
        let expected = call(
            "contains",
            vec![path(&["prose"]), call("labels", vec![path(&["secret"])])],
        );
        assert_eq!(expr, expected);

        // Also test `not contains(prose, labels(secret))`
        let expr = parse_expression("not contains(prose, labels(secret))").unwrap();
        let expected = unary(
            UnOp::Not,
            call(
                "contains",
                vec![path(&["prose"]), call("labels", vec![path(&["secret"])])],
            ),
        );
        assert_eq!(expr, expected);
    }

    #[test]
    fn parse_expression_let_binding() {
        // `let x = 5 in x + 1`
        let expr = parse_expression("let x = 5 in x + 1").unwrap();
        let expected = let_expr("x", lit_int(5), binop(BinOp::Add, path(&["x"]), lit_int(1)));
        assert_eq!(expr, expected);
    }

    #[test]
    fn parse_expression_path() {
        // `context.daily_limit`
        let expr = parse_expression("context.daily_limit").unwrap();
        assert_eq!(expr, path(&["context", "daily_limit"]));

        // `signal.args.amount`
        let expr = parse_expression("signal.args.amount").unwrap();
        assert_eq!(expr, path(&["signal", "args", "amount"]));

        // Single-segment path
        let expr = parse_expression("foo").unwrap();
        assert_eq!(expr, path(&["foo"]));
    }

    #[test]
    fn parse_expression_literals() {
        // Bool
        assert_eq!(parse_expression("true").unwrap(), lit_bool(true));
        assert_eq!(parse_expression("false").unwrap(), lit_bool(false));

        // Int
        assert_eq!(parse_expression("42").unwrap(), lit_int(42));
        assert_eq!(parse_expression("0").unwrap(), lit_int(0));
        assert_eq!(
            parse_expression("-3").unwrap(),
            unary(UnOp::Neg, lit_int(3))
        );

        // Float
        assert_eq!(parse_expression("3.14").unwrap(), lit_float(3.14));

        // String (double-quoted)
        assert_eq!(parse_expression("\"hello\"").unwrap(), lit_str("hello"));

        // String (single-quoted)
        assert_eq!(parse_expression("'hello'").unwrap(), lit_str("hello"));

        // String with escapes
        let expr = parse_expression("\"hello\\nworld\"").unwrap();
        assert_eq!(expr, lit_str("hello\nworld"));
    }

    #[test]
    fn parse_expression_all_unary_ops() {
        // `not true`
        let expr = parse_expression("not true").unwrap();
        assert_eq!(expr, unary(UnOp::Not, lit_bool(true)));

        // `-5`
        let expr = parse_expression("-5").unwrap();
        assert_eq!(expr, unary(UnOp::Neg, lit_int(5)));

        // `not (a == b)`
        let expr = parse_expression("not (a == b)").unwrap();
        assert_eq!(
            expr,
            unary(UnOp::Not, binop(BinOp::Eq, path(&["a"]), path(&["b"])))
        );

        // Double negation
        let expr = parse_expression("--5").unwrap();
        assert_eq!(expr, unary(UnOp::Neg, unary(UnOp::Neg, lit_int(5))));
    }

    #[test]
    fn parse_expression_error_recovery() {
        // Empty input
        let err = parse_expression("").unwrap_err();
        assert!(err.to_string().contains("expected expression"));

        // Trailing operator
        let err = parse_expression("1 +").unwrap_err();
        assert!(err.to_string().contains("expected expression"));

        // Unclosed paren
        let err = parse_expression("(1 + 2").unwrap_err();
        assert!(err.to_string().contains("expected"));

        // Unclosed string
        let err = parse_expression("\"hello").unwrap_err();
        assert!(err.to_string().contains("unterminated"));

        // Let without 'in'
        let err = parse_expression("let x = 5").unwrap_err();
        assert!(err.to_string().contains("'in'"));

        // Unexpected token after expression
        let err = parse_expression("1 2").unwrap_err();
        assert!(err.to_string().contains("unexpected token"));

        // Bad character
        let err = parse_expression("1 @ 2").unwrap_err();
        assert!(err.to_string().contains("unexpected character"));
    }

    #[test]
    fn parse_arithmetic_precedence() {
        // `2 + 3 * 4` → `2 + (3 * 4)`
        let expr = parse_expression("2 + 3 * 4").unwrap();
        assert_eq!(
            expr,
            binop(
                BinOp::Add,
                lit_int(2),
                binop(BinOp::Mul, lit_int(3), lit_int(4))
            )
        );

        // `(2 + 3) * 4`
        let expr = parse_expression("(2 + 3) * 4").unwrap();
        assert_eq!(
            expr,
            binop(
                BinOp::Mul,
                binop(BinOp::Add, lit_int(2), lit_int(3)),
                lit_int(4)
            )
        );
    }

    #[test]
    fn parse_comparison_operators() {
        // `a <= b`
        let expr = parse_expression("a <= b").unwrap();
        assert_eq!(expr, binop(BinOp::Lte, path(&["a"]), path(&["b"])));

        // `a != b`
        let expr = parse_expression("a != b").unwrap();
        assert_eq!(expr, binop(BinOp::Neq, path(&["a"]), path(&["b"])));

        // `a >= b`
        let expr = parse_expression("a >= b").unwrap();
        assert_eq!(expr, binop(BinOp::Gte, path(&["a"]), path(&["b"])));
    }

    #[test]
    fn parse_nested_let() {
        // `let x = 1 in let y = 2 in x + y`
        let expr = parse_expression("let x = 1 in let y = 2 in x + y").unwrap();
        let expected = let_expr(
            "x",
            lit_int(1),
            let_expr(
                "y",
                lit_int(2),
                binop(BinOp::Add, path(&["x"]), path(&["y"])),
            ),
        );
        assert_eq!(expr, expected);
    }

    #[test]
    fn parse_stdlib_functions() {
        // `sum(items)`
        let expr = parse_expression("sum(items)").unwrap();
        assert_eq!(expr, call("sum", vec![path(&["items"])]));

        // `exists(items, x, x > 0)`
        let expr = parse_expression("exists(items, x, x > 0)").unwrap();
        assert_eq!(
            expr,
            call(
                "exists",
                vec![
                    path(&["items"]),
                    path(&["x"]),
                    binop(BinOp::Gt, path(&["x"]), lit_int(0))
                ]
            )
        );

        // `now()`
        let expr = parse_expression("now()").unwrap();
        assert_eq!(expr, call("now", vec![]));

        // `regex_match(text, "pattern")`
        let expr = parse_expression("regex_match(text, \"pattern\")").unwrap();
        assert_eq!(
            expr,
            call("regex_match", vec![path(&["text"]), lit_str("pattern")])
        );
    }

    #[test]
    fn parse_complex_expression() {
        // `count(signals.where(kind == "transfer")) > 0 and context.daily_limit <= 100`
        let expr = parse_expression(
            "count(signals.where(kind == \"transfer\")) > 0 and context.daily_limit <= 100",
        )
        .unwrap();
        let expected = binop(
            BinOp::And,
            binop(
                BinOp::Gt,
                call(
                    "count",
                    vec![call(
                        "signals.where",
                        vec![binop(BinOp::Eq, path(&["kind"]), lit_str("transfer"))],
                    )],
                ),
                lit_int(0),
            ),
            binop(BinOp::Lte, path(&["context", "daily_limit"]), lit_int(100)),
        );
        assert_eq!(expr, expected);
    }
}
