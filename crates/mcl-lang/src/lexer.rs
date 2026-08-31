use mcl_spec::Span;

/// Token types for the MCL language
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // Structural
    LBrace,    // {
    RBrace,    // }
    LBracket,  // [
    RBracket,  // ]
    LParen,    // (
    RParen,    // )
    Colon,     // :
    Equals,    // =
    Comma,     // ,
    Arrow,     // ->
    Dot,       // .
    Ampersand, // &
    Pipe,      // |
    Question,  // ?
    Bang,      // !

    // Comparison operators
    EqEq,  // ==
    NotEq, // !=
    Gt,    // >
    Lt,    // <
    GtEq,  // >=
    LtEq,  // <=

    // Keywords
    // Graph-structural keywords (always keywords, never valid as names)
    Graph,
    Node,
    Channel,
    Memory,
    Via,
    Domain,
    Template,
    Import,
    Extends,
    // Content keywords (used in context files, also valid as identifiers in graph node names)
    // entity, item, action, transition, narrator, type → lexed as Ident, parser disambiguates
    Signals,
    Resolver,
    Schema,
    When,
    Requires,
    Mutates,
    Resolution,
    Target,
    Mode,
    Knows,
    Ignores,
    Where,

    // Primitives
    Ident(String),     // bare identifier
    StringLit(String), // "quoted string"
    IntLit(i64),
    FloatLit(f64),
    True,
    False,
    Null,

    // Special context-mode tokens
    FrontmatterDelim,      // ---
    SectionHeader(String), // # State, # Entities, etc.
    ProseBlock(String),    // indented multi-line prose

    // Meta
    Comment(String), // // comment
    Newline,
    Indent(u32), // indentation level (spaces)
    Eof,
}

impl Token {
    pub fn is_keyword(&self) -> bool {
        matches!(
            self,
            Token::Graph
                | Token::Node
                | Token::Channel
                | Token::Memory
                | Token::Via
                | Token::Domain
                | Token::Template
                | Token::Import
                | Token::Extends
                | Token::Signals
                | Token::Resolver
                | Token::Schema
                | Token::When
                | Token::Requires
                | Token::Mutates
                | Token::Resolution
                | Token::Target
                | Token::Mode
                | Token::Knows
                | Token::Ignores
                | Token::Where
        )
    }
}

#[derive(Debug, Clone)]
pub struct SpannedToken {
    pub token: Token,
    pub span: Span,
}

/// Lexer for the MCL language
pub struct Lexer<'a> {
    input: &'a str,
    pos: usize,
    line: u32,
    col: u32,
}

impl<'a> Lexer<'a> {
    pub fn new(input: &'a str) -> Self {
        Self {
            input,
            pos: 0,
            line: 1,
            col: 1,
        }
    }

    pub fn tokenize(&mut self) -> Vec<SpannedToken> {
        let mut tokens = Vec::new();

        while self.pos < self.input.len() {
            // Skip whitespace within a line (but track indentation at line start)
            if self.at_line_start() {
                let indent = self.count_indent();
                if indent > 0 {
                    tokens.push(self.make_token(Token::Indent(indent), indent as usize));
                }
            } else {
                self.skip_spaces();
            }

            if self.pos >= self.input.len() {
                break;
            }

            let ch = self.peek_char();

            match ch {
                '\n' => {
                    tokens.push(self.make_token(Token::Newline, 1));
                    self.advance();
                    self.line += 1;
                    self.col = 1;
                    continue;
                }
                '\r' => {
                    self.advance();
                    if self.peek_char() == '\n' {
                        self.advance();
                    }
                    tokens.push(self.make_token(Token::Newline, 1));
                    self.line += 1;
                    self.col = 1;
                    continue;
                }
                '/' if self.peek_ahead(1) == '/' => {
                    let comment = self.read_line_comment();
                    tokens.push(self.make_token_at(
                        Token::Comment(comment.clone()),
                        self.pos - comment.len() - 2,
                    ));
                }
                '#' if self.col == 1 || self.at_section_header_pos(&tokens) => {
                    let header = self.read_section_header();
                    if let Some(name) = header {
                        tokens.push(self.make_token_at(Token::SectionHeader(name), self.pos));
                    }
                }
                '-' if self.peek_ahead(1) == '-' && self.peek_ahead(2) == '-' && self.col <= 1 => {
                    tokens.push(self.make_token(Token::FrontmatterDelim, 3));
                    self.advance_n(3);
                }
                '-' if self.peek_ahead(1) == '>' => {
                    tokens.push(self.make_token(Token::Arrow, 2));
                    self.advance_n(2);
                }
                '-' => {
                    // Could be negative number or just a dash
                    if self.peek_ahead(1).is_ascii_digit() {
                        let tok = self.read_number();
                        tokens.push(tok);
                    } else {
                        // Treat as part of list item in context mode — emit as Ident("-")
                        tokens.push(self.make_token(Token::Ident("-".into()), 1));
                        self.advance();
                    }
                }
                '{' => {
                    tokens.push(self.make_token(Token::LBrace, 1));
                    self.advance();
                }
                '}' => {
                    tokens.push(self.make_token(Token::RBrace, 1));
                    self.advance();
                }
                '[' => {
                    tokens.push(self.make_token(Token::LBracket, 1));
                    self.advance();
                }
                ']' => {
                    tokens.push(self.make_token(Token::RBracket, 1));
                    self.advance();
                }
                '(' => {
                    tokens.push(self.make_token(Token::LParen, 1));
                    self.advance();
                }
                ')' => {
                    tokens.push(self.make_token(Token::RParen, 1));
                    self.advance();
                }
                ':' => {
                    tokens.push(self.make_token(Token::Colon, 1));
                    self.advance();
                }
                ',' => {
                    tokens.push(self.make_token(Token::Comma, 1));
                    self.advance();
                }
                '.' => {
                    tokens.push(self.make_token(Token::Dot, 1));
                    self.advance();
                }
                '&' => {
                    tokens.push(self.make_token(Token::Ampersand, 1));
                    self.advance();
                }
                '|' => {
                    tokens.push(self.make_token(Token::Pipe, 1));
                    self.advance();
                }
                '?' => {
                    tokens.push(self.make_token(Token::Question, 1));
                    self.advance();
                }
                '!' if self.peek_ahead(1) == '=' => {
                    tokens.push(self.make_token(Token::NotEq, 2));
                    self.advance_n(2);
                }
                '!' => {
                    tokens.push(self.make_token(Token::Bang, 1));
                    self.advance();
                }
                '=' if self.peek_ahead(1) == '=' => {
                    tokens.push(self.make_token(Token::EqEq, 2));
                    self.advance_n(2);
                }
                '=' => {
                    tokens.push(self.make_token(Token::Equals, 1));
                    self.advance();
                }
                '>' if self.peek_ahead(1) == '=' => {
                    tokens.push(self.make_token(Token::GtEq, 2));
                    self.advance_n(2);
                }
                '>' => {
                    tokens.push(self.make_token(Token::Gt, 1));
                    self.advance();
                }
                '<' if self.peek_ahead(1) == '=' => {
                    tokens.push(self.make_token(Token::LtEq, 2));
                    self.advance_n(2);
                }
                '<' => {
                    tokens.push(self.make_token(Token::Lt, 1));
                    self.advance();
                }
                '"' => {
                    let s = self.read_string();
                    tokens.push(s);
                }
                c if c.is_ascii_digit() => {
                    let tok = self.read_number();
                    tokens.push(tok);
                }
                c if c.is_ascii_alphabetic() || c == '_' => {
                    let tok = self.read_ident_or_keyword();
                    tokens.push(tok);
                }
                ' ' | '\t' => {
                    self.advance(); // skip remaining whitespace
                }
                other => {
                    // Unknown char — skip
                    log::warn!(
                        "{}:{}: unexpected character '{}'",
                        self.line,
                        self.col,
                        other
                    );
                    self.advance();
                }
            }
        }

        tokens.push(SpannedToken {
            token: Token::Eof,
            span: Span::new(self.pos, 0, self.line, self.col),
        });

        tokens
    }

    fn peek_char(&self) -> char {
        self.input[self.pos..].chars().next().unwrap_or('\0')
    }

    fn peek_ahead(&self, n: usize) -> char {
        self.input[self.pos..].chars().nth(n).unwrap_or('\0')
    }

    fn advance(&mut self) {
        if self.pos < self.input.len() {
            self.pos += 1;
            self.col += 1;
        }
    }

    fn advance_n(&mut self, n: usize) {
        for _ in 0..n {
            self.advance();
        }
    }

    fn skip_spaces(&mut self) {
        while self.pos < self.input.len() {
            let ch = self.peek_char();
            if ch == ' ' || ch == '\t' {
                self.advance();
            } else {
                break;
            }
        }
    }

    fn at_line_start(&self) -> bool {
        self.col == 1
    }

    fn at_section_header_pos(&self, _tokens: &[SpannedToken]) -> bool {
        // # at the start of a line (after optional whitespace)
        self.col <= 2 && self.peek_char() == '#'
    }

    fn count_indent(&mut self) -> u32 {
        let mut count = 0u32;
        while self.pos < self.input.len() {
            match self.peek_char() {
                ' ' => {
                    count += 1;
                    self.advance();
                }
                '\t' => {
                    count += 4;
                    self.advance();
                }
                _ => break,
            }
        }
        count
    }

    fn make_token(&self, token: Token, len: usize) -> SpannedToken {
        SpannedToken {
            token,
            span: Span::new(self.pos, len, self.line, self.col),
        }
    }

    fn make_token_at(&self, token: Token, offset: usize) -> SpannedToken {
        SpannedToken {
            token,
            span: Span::new(offset, 0, self.line, self.col),
        }
    }

    fn read_line_comment(&mut self) -> String {
        self.advance_n(2); // skip //
        let start = self.pos;
        while self.pos < self.input.len() && self.peek_char() != '\n' {
            self.advance();
        }
        self.input[start..self.pos].trim().to_string()
    }

    fn read_section_header(&mut self) -> Option<String> {
        self.advance(); // skip #
        self.skip_spaces();
        let start = self.pos;
        while self.pos < self.input.len() && self.peek_char() != '\n' {
            self.advance();
        }
        let name = self.input[start..self.pos].trim().to_string();
        if name.is_empty() {
            None
        } else {
            Some(name)
        }
    }

    fn read_string(&mut self) -> SpannedToken {
        let start_pos = self.pos;
        let start_col = self.col;
        self.advance(); // skip opening "
        let start = self.pos;
        while self.pos < self.input.len() && self.peek_char() != '"' {
            if self.peek_char() == '\\' {
                self.advance(); // skip escape
            }
            self.advance();
        }
        let s = self.input[start..self.pos].to_string();
        if self.pos < self.input.len() {
            self.advance(); // skip closing "
        }
        SpannedToken {
            token: Token::StringLit(s),
            span: Span::new(start_pos, self.pos - start_pos, self.line, start_col),
        }
    }

    fn read_number(&mut self) -> SpannedToken {
        let start = self.pos;
        let start_col = self.col;
        let negative = self.peek_char() == '-';
        if negative {
            self.advance();
        }
        while self.pos < self.input.len() && self.peek_char().is_ascii_digit() {
            self.advance();
        }
        let mut is_float = false;
        if self.pos < self.input.len()
            && self.peek_char() == '.'
            && self.peek_ahead(1).is_ascii_digit()
        {
            is_float = true;
            self.advance(); // skip .
            while self.pos < self.input.len() && self.peek_char().is_ascii_digit() {
                self.advance();
            }
        }
        let text = &self.input[start..self.pos];
        let token = if is_float {
            Token::FloatLit(text.parse().unwrap_or(0.0))
        } else {
            Token::IntLit(text.parse().unwrap_or(0))
        };
        SpannedToken {
            token,
            span: Span::new(start, self.pos - start, self.line, start_col),
        }
    }

    fn read_ident_or_keyword(&mut self) -> SpannedToken {
        let start = self.pos;
        let start_col = self.col;
        while self.pos < self.input.len() {
            let ch = self.peek_char();
            if ch.is_ascii_alphanumeric() || ch == '_' {
                self.advance();
            } else {
                break;
            }
        }
        let text = &self.input[start..self.pos];
        let token = match text {
            "graph" => Token::Graph,
            "node" => Token::Node,
            "channel" => Token::Channel,
            "memory" => Token::Memory,
            "via" => Token::Via,
            "domain" => Token::Domain,
            "template" => Token::Template,
            "import" => Token::Import,
            "extends" => Token::Extends,
            // entity, item, action, transition, narrator, type → Ident (parser handles context)
            "signals" => Token::Signals,
            "resolver" => Token::Resolver,
            "schema" => Token::Schema,
            "when" => Token::When,
            "requires" => Token::Requires,
            "mutates" => Token::Mutates,
            "resolution" => Token::Resolution,
            "target" => Token::Target,
            "mode" => Token::Mode,
            "knows" => Token::Knows,
            "ignores" => Token::Ignores,
            "where" => Token::Where,
            "true" => Token::True,
            "false" => Token::False,
            "null" => Token::Null,
            "bool" | "int" | "float" | "string" | "any" | "map" | "signal" => {
                Token::Ident(text.to_string()) // type names as identifiers
            }
            _ => Token::Ident(text.to_string()),
        };
        SpannedToken {
            token,
            span: Span::new(start, self.pos - start, self.line, start_col),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lex(input: &str) -> Vec<Token> {
        Lexer::new(input)
            .tokenize()
            .into_iter()
            .map(|st| st.token)
            .filter(|t| !matches!(t, Token::Newline | Token::Eof | Token::Comment(_)))
            .collect()
    }

    #[test]
    fn lex_graph_header() {
        let tokens = lex(r#"graph "test" {"#);
        assert_eq!(
            tokens,
            vec![Token::Graph, Token::StringLit("test".into()), Token::LBrace,]
        );
    }

    #[test]
    fn lex_node_declaration() {
        let tokens = lex("node narrator: llm {");
        assert_eq!(
            tokens,
            vec![
                Token::Node,
                Token::Ident("narrator".into()),
                Token::Colon,
                Token::Ident("llm".into()),
                Token::LBrace,
            ]
        );
    }

    #[test]
    fn lex_arrow_edge() {
        let tokens = lex("client -> narrator via user_input");
        assert_eq!(
            tokens,
            vec![
                Token::Ident("client".into()),
                Token::Arrow,
                Token::Ident("narrator".into()),
                Token::Via,
                Token::Ident("user_input".into()),
            ]
        );
    }

    #[test]
    fn lex_constraint_type() {
        let tokens = lex("hp: int & >=0 & <=100 = 12");
        assert_eq!(
            tokens,
            vec![
                Token::Ident("hp".into()),
                Token::Colon,
                Token::Ident("int".into()),
                Token::Ampersand,
                Token::GtEq,
                Token::IntLit(0),
                Token::Ampersand,
                Token::LtEq,
                Token::IntLit(100),
                Token::Equals,
                Token::IntLit(12),
            ]
        );
    }

    #[test]
    fn lex_enum_constraint() {
        let tokens = lex(r#"door: "locked" | "unlocked" = locked"#);
        assert_eq!(
            tokens,
            vec![
                Token::Ident("door".into()),
                Token::Colon,
                Token::StringLit("locked".into()),
                Token::Pipe,
                Token::StringLit("unlocked".into()),
                Token::Equals,
                Token::Ident("locked".into()),
            ]
        );
    }

    #[test]
    fn lex_frontmatter() {
        let tokens = lex("---\ncontext: crypt\ntype: room\n---");
        assert_eq!(
            tokens,
            vec![
                Token::FrontmatterDelim,
                Token::Ident("context".into()),
                Token::Colon,
                Token::Ident("crypt".into()),
                Token::Ident("type".into()),
                Token::Colon,
                Token::Ident("room".into()),
                Token::FrontmatterDelim,
            ]
        );
    }

    #[test]
    fn lex_entity_block() {
        let tokens = lex("entity skeleton_guard: enemy {");
        assert_eq!(
            tokens,
            vec![
                Token::Ident("entity".into()),
                Token::Ident("skeleton_guard".into()),
                Token::Colon,
                Token::Ident("enemy".into()),
                Token::LBrace,
            ]
        );
    }

    #[test]
    fn lex_action_block() {
        let tokens = lex("action attack {\n    target: enum(entities)\n}");
        // Filter indents too for cleaner assertion
        let tokens: Vec<Token> = tokens
            .into_iter()
            .filter(|t| !matches!(t, Token::Indent(_)))
            .collect();
        assert_eq!(
            tokens,
            vec![
                Token::Ident("action".into()),
                Token::Ident("attack".into()),
                Token::LBrace,
                Token::Target,
                Token::Colon,
                Token::Ident("enum".into()),
                Token::LParen,
                Token::Ident("entities".into()),
                Token::RParen,
                Token::RBrace,
            ]
        );
    }

    #[test]
    fn lex_knows_ignores() {
        let tokens = lex("knows: guards_tomb\nignores: secret_passage");
        assert_eq!(
            tokens,
            vec![
                Token::Knows,
                Token::Colon,
                Token::Ident("guards_tomb".into()),
                Token::Ignores,
                Token::Colon,
                Token::Ident("secret_passage".into()),
            ]
        );
    }

    #[test]
    fn lex_comparison() {
        let tokens = lex("when: door_north == unlocked");
        assert_eq!(
            tokens,
            vec![
                Token::When,
                Token::Colon,
                Token::Ident("door_north".into()),
                Token::EqEq,
                Token::Ident("unlocked".into()),
            ]
        );
    }

    #[test]
    fn lex_channel_schema() {
        let tokens = lex("channel user_input { payload: { text: string } }");
        assert_eq!(
            tokens,
            vec![
                Token::Channel,
                Token::Ident("user_input".into()),
                Token::LBrace,
                Token::Ident("payload".into()),
                Token::Colon,
                Token::LBrace,
                Token::Ident("text".into()),
                Token::Colon,
                Token::Ident("string".into()),
                Token::RBrace,
                Token::RBrace,
            ]
        );
    }

    #[test]
    fn lex_memory_block() {
        let tokens = lex("memory narrator {\n    capacity = 500\n}");
        let tokens: Vec<Token> = tokens
            .into_iter()
            .filter(|t| !matches!(t, Token::Indent(_)))
            .collect();
        assert_eq!(
            tokens,
            vec![
                Token::Memory,
                Token::Ident("narrator".into()),
                Token::LBrace,
                Token::Ident("capacity".into()),
                Token::Equals,
                Token::IntLit(500),
                Token::RBrace,
            ]
        );
    }
}
