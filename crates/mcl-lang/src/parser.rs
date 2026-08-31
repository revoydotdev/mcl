use indexmap::IndexMap;
use mcl_spec::Span;

use crate::ast::{
    ActionDecl, ChannelDecl, CmpOp, ConditionExpr, ContextFile, DomainFile, EdgeDecl, EntityDecl,
    Frontmatter, GraphFile, ItemDecl, KnowledgeEntry, MclFile, MemoryDecl, NarratorDecl, NodeDecl,
    SchemaDecl, StateVar, TargetExpr, TemplateDecl, TemplateFile, TransitionDecl, TypeDecl,
    TypeExpr, ValueExpr,
};
use crate::lexer::{Lexer, SpannedToken, Token};

/// Parse error
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("{span}: {message}")]
    Error { message: String, span: Span },
}

impl ParseError {
    fn at(message: impl Into<String>, span: Span) -> Self {
        Self::Error {
            message: message.into(),
            span,
        }
    }
}

/// MCL parser — recursive descent over token stream
pub struct Parser {
    tokens: Vec<SpannedToken>,
    pos: usize,
}

impl Parser {
    pub fn new(tokens: Vec<SpannedToken>) -> Self {
        Self { tokens, pos: 0 }
    }

    pub fn parse(source: &str) -> Result<MclFile, ParseError> {
        let tokens = Lexer::new(source).tokenize();
        let mut parser = Parser::new(tokens);
        parser.parse_file()
    }

    // ─── File dispatch ──────────────────────────────────────────────

    fn parse_file(&mut self) -> Result<MclFile, ParseError> {
        self.skip_whitespace();

        // Detect file type from first significant token
        match self.peek() {
            Token::Graph => Ok(MclFile::Graph(self.parse_graph_file()?)),
            Token::Domain => Ok(MclFile::Domain(self.parse_domain_file()?)),
            Token::Template => Ok(MclFile::Template(self.parse_template_file()?)),
            Token::FrontmatterDelim => Ok(MclFile::Context(Box::new(self.parse_context_file()?))),
            _ => Err(self.error("expected `graph`, `domain`, `template`, or `---` frontmatter")),
        }
    }

    // ─── Graph file ─────────────────────────────────────────────────

    fn parse_graph_file(&mut self) -> Result<GraphFile, ParseError> {
        let span = self.current_span();
        self.expect(&Token::Graph)?;
        let name = self.expect_string_or_ident()?;
        self.expect(&Token::LBrace)?;

        let mut domain = None;
        let mut nodes = Vec::new();
        let mut channels = Vec::new();
        let mut edges = Vec::new();
        let mut memories = Vec::new();

        while !self.check(&Token::RBrace) && !self.check(&Token::Eof) {
            self.skip_whitespace();
            match self.peek() {
                Token::Domain => {
                    self.advance();
                    domain = Some(self.expect_ident()?);
                }
                Token::Node => {
                    nodes.push(self.parse_node_decl()?);
                }
                Token::Channel => {
                    channels.push(self.parse_channel_decl()?);
                }
                Token::Memory => {
                    memories.push(self.parse_memory_decl()?);
                }
                Token::Ident(_) => {
                    // Could be an edge: ident -> ident via ident
                    if self.is_edge_ahead() {
                        edges.push(self.parse_edge_decl()?);
                    } else {
                        self.advance(); // skip unknown
                    }
                }
                Token::RBrace => break,
                _ => {
                    self.advance();
                }
            }
        }

        self.expect(&Token::RBrace)?;

        Ok(GraphFile {
            name,
            domain,
            nodes,
            channels,
            edges,
            memories,
            span,
        })
    }

    fn parse_node_decl(&mut self) -> Result<NodeDecl, ParseError> {
        let span = self.current_span();
        self.expect(&Token::Node)?;
        let name = self.expect_ident()?;
        self.expect(&Token::Colon)?;
        let node_type = self.expect_ident()?;

        let config = if self.check(&Token::LBrace) {
            self.parse_config_block()?
        } else {
            IndexMap::new()
        };

        Ok(NodeDecl {
            name,
            node_type,
            config,
            span,
        })
    }

    fn parse_channel_decl(&mut self) -> Result<ChannelDecl, ParseError> {
        let span = self.current_span();
        self.expect(&Token::Channel)?;
        let name = self.expect_ident()?;

        let payload_schema = if self.check(&Token::LBrace) {
            self.advance(); // {
            self.skip_whitespace();
            let mut schema = None;
            while !self.check(&Token::RBrace) && !self.check(&Token::Eof) {
                self.skip_whitespace();
                let key = self.expect_ident()?;
                self.expect(&Token::Colon)?;
                if key == "payload" {
                    schema = Some(self.parse_type_struct()?);
                }
                self.skip_whitespace();
            }
            self.expect(&Token::RBrace)?;
            schema
        } else {
            None
        };

        Ok(ChannelDecl {
            name,
            payload_schema,
            span,
        })
    }

    fn parse_edge_decl(&mut self) -> Result<EdgeDecl, ParseError> {
        let span = self.current_span();
        let from = self.expect_ident()?;
        self.expect(&Token::Arrow)?;
        let to = self.expect_ident()?;
        self.expect(&Token::Via)?;
        let channel = self.expect_ident()?;

        Ok(EdgeDecl {
            from,
            to,
            channel,
            span,
        })
    }

    fn parse_memory_decl(&mut self) -> Result<MemoryDecl, ParseError> {
        let span = self.current_span();
        self.expect(&Token::Memory)?;
        let node = self.expect_ident()?;
        let config = self.parse_config_block()?;

        Ok(MemoryDecl { node, config, span })
    }

    // ─── Context file ───────────────────────────────────────────────

    fn parse_context_file(&mut self) -> Result<ContextFile, ParseError> {
        let span = self.current_span();

        // Parse frontmatter
        let frontmatter = self.parse_frontmatter()?;

        let mut state = Vec::new();
        let mut entities = Vec::new();
        let mut items = Vec::new();
        let mut actions = Vec::new();
        let mut transitions = Vec::new();
        let mut narrator = None;
        let mut knowledge = Vec::new();
        let mut constraints = Vec::new();

        // Parse sections
        while !self.check(&Token::Eof) {
            self.skip_whitespace();
            if let Token::SectionHeader(ref name) = self.peek() {
                let section_name = name.clone();
                self.advance(); // consume section header

                match section_name.as_str() {
                    "State" => state = self.parse_state_section()?,
                    "Entities" => entities = self.parse_entities_section()?,
                    "Items" => items = self.parse_items_section()?,
                    "Actions" => actions = self.parse_actions_section()?,
                    "Transitions" => transitions = self.parse_transitions_section()?,
                    "Narrator" => narrator = Some(self.parse_narrator_section()?),
                    "Knowledge" => knowledge = self.parse_knowledge_section()?,
                    "Constraints" => constraints = self.parse_constraints_section()?,
                    _ => { /* skip unknown sections */ }
                }
            } else {
                self.advance();
            }
        }

        Ok(ContextFile {
            frontmatter,
            state,
            entities,
            items,
            actions,
            transitions,
            narrator,
            knowledge,
            constraints,
            span,
        })
    }

    fn parse_frontmatter(&mut self) -> Result<Frontmatter, ParseError> {
        let span = self.current_span();
        self.expect(&Token::FrontmatterDelim)?;
        self.skip_newlines();

        let mut entries = IndexMap::new();

        while !self.check(&Token::FrontmatterDelim) && !self.check(&Token::Eof) {
            self.skip_whitespace();
            if self.check(&Token::FrontmatterDelim) {
                break;
            }
            let key = self.expect_ident()?;
            self.expect(&Token::Colon)?;
            let value = self.expect_string_or_ident()?;
            entries.insert(key, value);
            self.skip_newlines();
        }

        self.expect(&Token::FrontmatterDelim)?;

        Ok(Frontmatter { entries, span })
    }

    fn parse_state_section(&mut self) -> Result<Vec<StateVar>, ParseError> {
        let mut vars = Vec::new();
        while self.at_section_content() {
            self.skip_whitespace();
            if !self.at_section_content() {
                break;
            }
            let span = self.current_span();
            let name = match self.try_ident() {
                Some(n) => n,
                None => break,
            };
            self.expect(&Token::Colon)?;

            // Parse optional type constraint and default value
            let (type_constraint, default) = self.parse_typed_value()?;

            vars.push(StateVar {
                name,
                type_constraint,
                default,
                span,
            });
            self.skip_newlines();
        }
        Ok(vars)
    }

    fn parse_entities_section(&mut self) -> Result<Vec<EntityDecl>, ParseError> {
        let mut entities = Vec::new();
        while self.at_section_content() {
            self.skip_whitespace();
            if !self.at_section_content() {
                break;
            }
            // entity name: type { ... } OR entity name { ... } OR - name: \n indented
            if self.check_ident("entity") {
                entities.push(self.parse_entity_block()?);
            } else if self.check(&Token::Ident("-".into())) {
                // Legacy list-item style
                entities.push(self.parse_entity_list_item()?);
            } else {
                break;
            }
            self.skip_newlines();
        }
        Ok(entities)
    }

    fn parse_entity_block(&mut self) -> Result<EntityDecl, ParseError> {
        let span = self.current_span();
        self.advance(); // skip "entity"
        let name = self.expect_ident()?;

        let entity_type = if self.check(&Token::Colon) {
            self.advance();
            Some(self.expect_ident()?)
        } else {
            None
        };

        let extends = if self.check(&Token::Extends) {
            self.advance();
            Some(self.expect_ident()?)
        } else {
            None
        };

        let mut properties = IndexMap::new();
        let mut knows = Vec::new();

        if self.check(&Token::LBrace) {
            self.advance();
            self.skip_whitespace();
            while !self.check(&Token::RBrace) && !self.check(&Token::Eof) {
                self.skip_whitespace();
                if self.check(&Token::RBrace) {
                    break;
                }

                if self.check(&Token::Knows) {
                    self.advance();
                    self.expect(&Token::Colon)?;
                    knows = self.parse_ident_list()?;
                } else {
                    let key = self.expect_ident()?;
                    self.expect(&Token::Colon)?;
                    let val = self.parse_value_expr()?;
                    properties.insert(key, val);
                }
                self.skip_newlines();
            }
            self.expect(&Token::RBrace)?;
        }

        Ok(EntityDecl {
            name,
            entity_type,
            extends,
            properties,
            knows,
            constraint_tags: Vec::new(),
            span,
        })
    }

    fn parse_entity_list_item(&mut self) -> Result<EntityDecl, ParseError> {
        let span = self.current_span();
        self.advance(); // skip "-"
        let name = self.expect_ident()?;
        self.expect(&Token::Colon)?;

        // Could be inline map or type name
        let entity_type = if self.check(&Token::LBrace) {
            None
        } else {
            self.try_ident()
        };

        let mut properties = IndexMap::new();
        if self.check(&Token::LBrace) {
            properties = self.parse_inline_map()?;
        }

        Ok(EntityDecl {
            name,
            entity_type,
            extends: None,
            properties,
            knows: Vec::new(),
            constraint_tags: Vec::new(),
            span,
        })
    }

    fn parse_items_section(&mut self) -> Result<Vec<ItemDecl>, ParseError> {
        let mut items = Vec::new();
        while self.at_section_content() {
            self.skip_whitespace();
            if !self.at_section_content() {
                break;
            }

            if self.check_ident("item") {
                items.push(self.parse_item_block()?);
            } else if self.check(&Token::Ident("-".into())) {
                items.push(self.parse_item_list()?);
            } else {
                break;
            }
            self.skip_newlines();
        }
        Ok(items)
    }

    fn parse_item_block(&mut self) -> Result<ItemDecl, ParseError> {
        let span = self.current_span();
        self.advance(); // skip "item"
        let name = self.expect_ident()?;
        let properties = if self.check(&Token::LBrace) {
            self.parse_inline_map()?
        } else {
            IndexMap::new()
        };
        Ok(ItemDecl {
            name,
            properties,
            span,
        })
    }

    fn parse_item_list(&mut self) -> Result<ItemDecl, ParseError> {
        let span = self.current_span();
        self.advance(); // skip "-"
        let name = self.expect_ident()?;
        self.expect(&Token::Colon)?;
        let properties = if self.check(&Token::LBrace) {
            self.parse_inline_map()?
        } else {
            IndexMap::new()
        };
        Ok(ItemDecl {
            name,
            properties,
            span,
        })
    }

    fn parse_actions_section(&mut self) -> Result<Vec<ActionDecl>, ParseError> {
        let mut actions = Vec::new();
        while self.at_section_content() {
            self.skip_whitespace();
            if !self.at_section_content() {
                break;
            }

            if self.check_ident("action") {
                actions.push(self.parse_action_block()?);
            } else if self.check(&Token::Ident("-".into())) {
                actions.push(self.parse_action_list_item()?);
            } else {
                break;
            }
            self.skip_newlines();
        }
        Ok(actions)
    }

    fn parse_action_block(&mut self) -> Result<ActionDecl, ParseError> {
        let span = self.current_span();
        self.advance(); // skip "action"
        let name = self.expect_ident()?;
        self.expect(&Token::LBrace)?;

        let mut action = ActionDecl {
            name,
            requires: Vec::new(),
            mutates: IndexMap::new(),
            resolution: None,
            target: None,
            mode: None,
            description: None,
            span,
        };

        self.skip_whitespace();
        while !self.check(&Token::RBrace) && !self.check(&Token::Eof) {
            self.skip_whitespace();
            if self.check(&Token::RBrace) {
                break;
            }

            match self.peek() {
                Token::Requires => {
                    self.advance();
                    self.expect(&Token::Colon)?;
                    action.requires = self.parse_condition_list()?;
                }
                Token::Mutates => {
                    self.advance();
                    self.expect(&Token::Colon)?;
                    action.mutates = self.parse_inline_map()?;
                }
                Token::Resolution => {
                    self.advance();
                    self.expect(&Token::Colon)?;
                    action.resolution = Some(self.expect_ident()?);
                }
                Token::Target => {
                    self.advance();
                    self.expect(&Token::Colon)?;
                    action.target = Some(self.parse_target_expr()?);
                }
                Token::Mode => {
                    self.advance();
                    self.expect(&Token::Colon)?;
                    action.mode = Some(self.expect_ident()?);
                }
                Token::StringLit(s) => {
                    action.description = Some(s.clone());
                    self.advance();
                }
                _ => {
                    self.advance();
                }
            }
            self.skip_newlines();
        }

        self.expect(&Token::RBrace)?;
        Ok(action)
    }

    fn parse_action_list_item(&mut self) -> Result<ActionDecl, ParseError> {
        let span = self.current_span();
        self.advance(); // skip "-"
        let name = self.expect_ident()?;
        self.expect(&Token::Colon)?;
        // Skip to next line, consume indented properties
        // Simplified: just create empty action for now
        Ok(ActionDecl {
            name,
            requires: Vec::new(),
            mutates: IndexMap::new(),
            resolution: None,
            target: None,
            mode: None,
            description: None,
            span,
        })
    }

    fn parse_transitions_section(&mut self) -> Result<Vec<TransitionDecl>, ParseError> {
        let mut transitions = Vec::new();
        while self.at_section_content() {
            self.skip_whitespace();
            if !self.at_section_content() {
                break;
            }

            if self.check_ident("transition") {
                transitions.push(self.parse_transition_block()?);
            } else {
                break;
            }
            self.skip_newlines();
        }
        Ok(transitions)
    }

    fn parse_transition_block(&mut self) -> Result<TransitionDecl, ParseError> {
        let span = self.current_span();
        self.advance(); // skip "transition"
        let target = self.expect_ident()?;
        self.expect(&Token::LBrace)?;

        let mut when = None;
        let mut direction = None;
        let mut auto_ = false;

        self.skip_whitespace();
        while !self.check(&Token::RBrace) && !self.check(&Token::Eof) {
            self.skip_whitespace();
            if self.check(&Token::RBrace) {
                break;
            }

            match self.peek() {
                Token::When => {
                    self.advance();
                    self.expect(&Token::Colon)?;
                    when = Some(self.parse_condition()?);
                }
                Token::Ident(ref s) if s == "direction" => {
                    self.advance();
                    self.expect(&Token::Colon)?;
                    direction = Some(self.expect_ident()?);
                }
                Token::Ident(ref s) if s == "auto" => {
                    self.advance();
                    self.expect(&Token::Colon)?;
                    auto_ = self.check(&Token::True);
                    self.advance();
                }
                _ => {
                    self.advance();
                }
            }
            self.skip_newlines();
        }

        self.expect(&Token::RBrace)?;

        Ok(TransitionDecl {
            target,
            when,
            direction,
            auto_,
            span,
        })
    }

    fn parse_narrator_section(&mut self) -> Result<NarratorDecl, ParseError> {
        self.skip_whitespace();
        let span = self.current_span();
        let mut narrator = NarratorDecl {
            tone: None,
            voice: None,
            tense: None,
            forbidden: Vec::new(),
            context: None,
            style: None,
            constraint_tags: Vec::new(),
            span,
        };

        // Could be `narrator { }` block or key: value pairs
        if self.check_ident("narrator") {
            self.advance();
            if self.check(&Token::LBrace) {
                self.advance();
                self.skip_whitespace();
                while !self.check(&Token::RBrace) && !self.check(&Token::Eof) {
                    self.skip_whitespace();
                    if self.check(&Token::RBrace) {
                        break;
                    }
                    self.parse_narrator_field(&mut narrator)?;
                    self.skip_newlines();
                }
                self.expect(&Token::RBrace)?;
                return Ok(narrator);
            }
        }

        // Key-value pairs until next section
        while self.at_section_content() {
            self.skip_whitespace();
            if !self.at_section_content() {
                break;
            }
            self.parse_narrator_field(&mut narrator)?;
            self.skip_newlines();
        }

        Ok(narrator)
    }

    fn parse_narrator_field(&mut self, narrator: &mut NarratorDecl) -> Result<(), ParseError> {
        let key = match self.try_ident() {
            Some(k) => k,
            None => return Ok(()),
        };
        self.expect(&Token::Colon)?;

        match key.as_str() {
            "tone" => narrator.tone = Some(self.expect_string_or_ident()?),
            "voice" => narrator.voice = Some(self.expect_string_or_ident()?),
            "tense" => narrator.tense = Some(self.expect_string_or_ident()?),
            "forbidden" => narrator.forbidden = self.parse_string_list()?,
            "context" => narrator.context = Some(self.expect_string_or_ident()?),
            "style" | "style_notes" => narrator.style = Some(self.expect_string_or_ident()?),
            _ => {
                let _ = self.parse_value_expr();
            }
        }
        Ok(())
    }

    fn parse_knowledge_section(&mut self) -> Result<Vec<KnowledgeEntry>, ParseError> {
        let mut entries = Vec::new();
        while self.at_section_content() {
            self.skip_whitespace();
            if !self.at_section_content() {
                break;
            }

            match self.peek() {
                Token::Knows => {
                    self.advance();
                    self.expect(&Token::Colon)?;
                    let fact = self.expect_ident()?;
                    entries.push(KnowledgeEntry::Knows(fact));
                }
                Token::Ignores => {
                    self.advance();
                    self.expect(&Token::Colon)?;
                    let fact = self.expect_ident()?;
                    entries.push(KnowledgeEntry::Ignores(fact));
                }
                Token::Ident(ref s) if s == "-" => {
                    self.advance();
                    if self.check(&Token::Bang) {
                        self.advance();
                        let fact = self.expect_ident()?;
                        entries.push(KnowledgeEntry::Ignores(fact));
                    } else {
                        let fact = self.expect_ident()?;
                        entries.push(KnowledgeEntry::Knows(fact));
                    }
                }
                _ => break,
            }
            self.skip_newlines();
        }
        Ok(entries)
    }

    fn parse_constraints_section(&mut self) -> Result<Vec<String>, ParseError> {
        let mut constraints = Vec::new();
        while self.at_section_content() {
            self.skip_whitespace();
            if !self.at_section_content() {
                break;
            }

            if self.check(&Token::Ident("-".into())) {
                self.advance(); // skip "-"
                                // Collect all tokens until newline as a constraint string
                let mut text = String::new();
                while !self.check(&Token::Newline) && !self.check(&Token::Eof) {
                    if !text.is_empty() {
                        text.push(' ');
                    }
                    text.push_str(&self.token_to_string());
                    self.advance();
                }
                if !text.is_empty() {
                    constraints.push(text);
                }
            } else {
                break;
            }
            self.skip_newlines();
        }
        Ok(constraints)
    }

    // ─── Domain file ────────────────────────────────────────────────

    fn parse_domain_file(&mut self) -> Result<DomainFile, ParseError> {
        let span = self.current_span();
        self.expect(&Token::Domain)?;
        let name = self.expect_ident()?;
        self.expect(&Token::LBrace)?;

        let mut resolvers = Vec::new();
        let mut types = Vec::new();
        let mut schemas = Vec::new();

        self.skip_whitespace();
        while !self.check(&Token::RBrace) && !self.check(&Token::Eof) {
            self.skip_whitespace();
            match self.peek() {
                Token::Resolver => {
                    self.advance();
                    resolvers.push(self.expect_ident()?);
                }
                Token::Ident(ref s) if s == "type" => {
                    types.push(self.parse_type_decl()?);
                }
                Token::Schema => {
                    schemas.push(self.parse_schema_decl()?);
                }
                Token::RBrace => break,
                _ => {
                    self.advance();
                }
            }
            self.skip_newlines();
        }

        self.expect(&Token::RBrace)?;

        Ok(DomainFile {
            name,
            resolvers,
            types,
            schemas,
            span,
        })
    }

    fn parse_type_decl(&mut self) -> Result<TypeDecl, ParseError> {
        let span = self.current_span();
        self.advance(); // skip "type"
        let name = self.expect_ident()?;
        self.expect(&Token::LBrace)?;

        let mut fields = IndexMap::new();
        self.skip_whitespace();
        while !self.check(&Token::RBrace) && !self.check(&Token::Eof) {
            self.skip_whitespace();
            if self.check(&Token::RBrace) {
                break;
            }
            let key = self.expect_ident()?;
            self.expect(&Token::Colon)?;
            let ty = self.parse_type_expr()?;
            fields.insert(key, ty);
            self.skip_comma_or_newline();
        }
        self.expect(&Token::RBrace)?;

        Ok(TypeDecl { name, fields, span })
    }

    fn parse_schema_decl(&mut self) -> Result<SchemaDecl, ParseError> {
        let span = self.current_span();
        self.expect(&Token::Schema)?;
        let name = self.expect_ident()?;
        self.expect(&Token::LBrace)?;

        let mut fields = Vec::new();
        self.skip_whitespace();
        while !self.check(&Token::RBrace) && !self.check(&Token::Eof) {
            self.skip_whitespace();
            if self.check(&Token::RBrace) {
                break;
            }
            let fspan = self.current_span();
            let fname = self.expect_ident()?;
            self.expect(&Token::Colon)?;
            let (type_constraint, default) = self.parse_typed_value()?;
            fields.push(StateVar {
                name: fname,
                type_constraint,
                default,
                span: fspan,
            });
            self.skip_comma_or_newline();
        }
        self.expect(&Token::RBrace)?;

        Ok(SchemaDecl { name, fields, span })
    }

    // ─── Template file ──────────────────────────────────────────────

    fn parse_template_file(&mut self) -> Result<TemplateFile, ParseError> {
        let span = self.current_span();
        let mut templates = Vec::new();

        while self.check(&Token::Template) {
            templates.push(self.parse_template_decl()?);
            self.skip_whitespace();
        }

        Ok(TemplateFile { templates, span })
    }

    fn parse_template_decl(&mut self) -> Result<TemplateDecl, ParseError> {
        let span = self.current_span();
        self.expect(&Token::Template)?;
        let name = self.expect_ident()?;

        let extends = if self.check(&Token::Extends) {
            self.advance();
            Some(self.expect_ident()?)
        } else {
            None
        };

        self.expect(&Token::LBrace)?;
        let mut properties = IndexMap::new();
        self.skip_whitespace();
        while !self.check(&Token::RBrace) && !self.check(&Token::Eof) {
            self.skip_whitespace();
            if self.check(&Token::RBrace) {
                break;
            }
            let key = self.expect_ident()?;
            self.expect(&Token::Colon)?;
            let val = self.parse_value_expr()?;
            properties.insert(key, val);
            self.skip_comma_or_newline();
        }
        self.expect(&Token::RBrace)?;

        Ok(TemplateDecl {
            name,
            extends,
            properties,
            span,
        })
    }

    // ─── Expression parsers ─────────────────────────────────────────

    fn parse_value_expr(&mut self) -> Result<ValueExpr, ParseError> {
        match self.peek() {
            Token::IntLit(n) => {
                self.advance();
                Ok(ValueExpr::Int(n))
            }
            Token::FloatLit(n) => {
                self.advance();
                Ok(ValueExpr::Float(n))
            }
            Token::StringLit(ref s) => {
                let s = s.clone();
                self.advance();
                Ok(ValueExpr::String(s))
            }
            Token::True => {
                self.advance();
                Ok(ValueExpr::Bool(true))
            }
            Token::False => {
                self.advance();
                Ok(ValueExpr::Bool(false))
            }
            Token::Null => {
                self.advance();
                Ok(ValueExpr::Null)
            }
            Token::LBracket => self.parse_list_expr(),
            Token::LBrace => Ok(ValueExpr::Map(self.parse_inline_map()?)),
            Token::Ident(ref s) => {
                let s = s.clone();
                self.advance();
                Ok(ValueExpr::Ident(s))
            }
            // Keywords that can appear as values
            _ => {
                let s = self.token_to_string();
                self.advance();
                Ok(ValueExpr::Ident(s))
            }
        }
    }

    fn parse_list_expr(&mut self) -> Result<ValueExpr, ParseError> {
        self.expect(&Token::LBracket)?;
        let mut items = Vec::new();
        while !self.check(&Token::RBracket) && !self.check(&Token::Eof) {
            self.skip_whitespace();
            if self.check(&Token::RBracket) {
                break;
            }
            items.push(self.parse_value_expr()?);
            if self.check(&Token::Comma) {
                self.advance();
            }
            self.skip_whitespace();
        }
        self.expect(&Token::RBracket)?;
        Ok(ValueExpr::List(items))
    }

    fn parse_inline_map(&mut self) -> Result<IndexMap<String, ValueExpr>, ParseError> {
        self.expect(&Token::LBrace)?;
        let mut map = IndexMap::new();
        self.skip_whitespace();
        while !self.check(&Token::RBrace) && !self.check(&Token::Eof) {
            self.skip_whitespace();
            if self.check(&Token::RBrace) {
                break;
            }
            let key = self.expect_ident()?;
            self.expect(&Token::Colon)?;
            let val = self.parse_value_expr()?;
            map.insert(key, val);
            if self.check(&Token::Comma) {
                self.advance();
            }
            self.skip_whitespace();
        }
        self.expect(&Token::RBrace)?;
        Ok(map)
    }

    fn parse_type_expr(&mut self) -> Result<TypeExpr, ParseError> {
        let mut base = self.parse_type_atom()?;

        // Handle & (intersection) and | (union)
        loop {
            match self.peek() {
                Token::Ampersand => {
                    self.advance();
                    let right = self.parse_type_atom()?;
                    base = match base {
                        TypeExpr::Intersection(mut v) => {
                            v.push(right);
                            TypeExpr::Intersection(v)
                        }
                        _ => TypeExpr::Intersection(vec![base, right]),
                    };
                }
                Token::Pipe => {
                    self.advance();
                    let right = self.parse_type_atom()?;
                    base = match base {
                        TypeExpr::Union(mut v) => {
                            v.push(right);
                            TypeExpr::Union(v)
                        }
                        _ => TypeExpr::Union(vec![base, right]),
                    };
                }
                Token::Question => {
                    self.advance();
                    base = TypeExpr::Optional(Box::new(base));
                }
                _ => break,
            }
        }

        Ok(base)
    }

    fn parse_type_atom(&mut self) -> Result<TypeExpr, ParseError> {
        match self.peek() {
            Token::Ident(ref s) => {
                let s = s.clone();
                self.advance();
                Ok(TypeExpr::Named(s))
            }
            Token::StringLit(ref s) => {
                let s = s.clone();
                self.advance();
                Ok(TypeExpr::StringLiteral(s))
            }
            Token::LBracket => {
                self.advance();
                let inner = self.parse_type_expr()?;
                self.expect(&Token::RBracket)?;
                Ok(TypeExpr::ListOf(Box::new(inner)))
            }
            Token::LBrace => Ok(TypeExpr::Struct(self.parse_type_struct()?)),
            Token::GtEq => {
                self.advance();
                let n = self.expect_int()?;
                Ok(TypeExpr::RangeGe(n))
            }
            Token::LtEq => {
                self.advance();
                let n = self.expect_int()?;
                Ok(TypeExpr::RangeLe(n))
            }
            _ => Err(self.error("expected type expression")),
        }
    }

    fn parse_type_struct(&mut self) -> Result<IndexMap<String, TypeExpr>, ParseError> {
        self.expect(&Token::LBrace)?;
        let mut fields = IndexMap::new();
        self.skip_whitespace();
        while !self.check(&Token::RBrace) && !self.check(&Token::Eof) {
            self.skip_whitespace();
            if self.check(&Token::RBrace) {
                break;
            }
            let key = self.expect_ident()?;
            self.expect(&Token::Colon)?;
            let ty = self.parse_type_expr()?;
            fields.insert(key, ty);
            if self.check(&Token::Comma) {
                self.advance();
            }
            self.skip_whitespace();
        }
        self.expect(&Token::RBrace)?;
        Ok(fields)
    }

    fn parse_typed_value(&mut self) -> Result<(Option<TypeExpr>, Option<ValueExpr>), ParseError> {
        // Could be: value, type = value, type, or just value
        // Heuristic: if we see a type keyword (int, float, bool, string) followed by & or =, it's typed
        let is_type_start = matches!(self.peek(), Token::Ident(ref s) if matches!(s.as_str(), "int" | "float" | "bool" | "string" | "any"))
            || self.check(&Token::LBracket);

        if is_type_start {
            let ty = self.parse_type_expr()?;
            if self.check(&Token::Equals) {
                self.advance();
                let val = self.parse_value_expr()?;
                Ok((Some(ty), Some(val)))
            } else {
                Ok((Some(ty), None))
            }
        } else if self.check(&Token::StringLit("".into())) && self.peek_ahead_is_pipe() {
            // Enum constraint: "locked" | "unlocked" = value
            let ty = self.parse_type_expr()?;
            if self.check(&Token::Equals) {
                self.advance();
                let val = self.parse_value_expr()?;
                Ok((Some(ty), Some(val)))
            } else {
                Ok((Some(ty), None))
            }
        } else {
            let val = self.parse_value_expr()?;
            Ok((None, Some(val)))
        }
    }

    fn parse_condition(&mut self) -> Result<ConditionExpr, ParseError> {
        if self.check(&Token::Bang) {
            self.advance();
            let path = self.expect_field_path()?;
            return Ok(ConditionExpr::Falsy(path));
        }

        let left = self.expect_field_path()?;

        let op = match self.peek() {
            Token::EqEq => Some(CmpOp::Eq),
            Token::NotEq => Some(CmpOp::Ne),
            Token::Gt => Some(CmpOp::Gt),
            Token::Lt => Some(CmpOp::Lt),
            Token::GtEq => Some(CmpOp::Ge),
            Token::LtEq => Some(CmpOp::Le),
            _ => None,
        };

        if let Some(op) = op {
            self.advance();
            let right = self.parse_value_expr()?;
            Ok(ConditionExpr::Compare { left, op, right })
        } else {
            Ok(ConditionExpr::Truthy(left))
        }
    }

    fn parse_condition_list(&mut self) -> Result<Vec<ConditionExpr>, ParseError> {
        if self.check(&Token::LBracket) {
            self.advance();
            let mut conds = Vec::new();
            while !self.check(&Token::RBracket) && !self.check(&Token::Eof) {
                self.skip_whitespace();
                if self.check(&Token::RBracket) {
                    break;
                }
                conds.push(self.parse_condition()?);
                if self.check(&Token::Comma) {
                    self.advance();
                }
                self.skip_whitespace();
            }
            self.expect(&Token::RBracket)?;
            Ok(conds)
        } else {
            Ok(vec![self.parse_condition()?])
        }
    }

    fn parse_target_expr(&mut self) -> Result<TargetExpr, ParseError> {
        if self.check_ident("enum") {
            self.advance();
            self.expect(&Token::LParen)?;
            let source = self.expect_ident()?;
            let filter = if self.check(&Token::Where) {
                self.advance();
                let mut filter_text = String::new();
                while !self.check(&Token::RParen) && !self.check(&Token::Eof) {
                    if !filter_text.is_empty() {
                        filter_text.push(' ');
                    }
                    filter_text.push_str(&self.token_to_string());
                    self.advance();
                }
                Some(filter_text)
            } else {
                None
            };
            self.expect(&Token::RParen)?;
            Ok(TargetExpr::Enum { source, filter })
        } else {
            let name = self.expect_ident()?;
            Ok(TargetExpr::Fixed(name))
        }
    }

    fn parse_ident_list(&mut self) -> Result<Vec<String>, ParseError> {
        if self.check(&Token::LBracket) {
            self.advance();
            let mut items = Vec::new();
            while !self.check(&Token::RBracket) && !self.check(&Token::Eof) {
                self.skip_whitespace();
                if self.check(&Token::RBracket) {
                    break;
                }
                items.push(self.expect_string_or_ident()?);
                if self.check(&Token::Comma) {
                    self.advance();
                }
                self.skip_whitespace();
            }
            self.expect(&Token::RBracket)?;
            Ok(items)
        } else {
            Ok(vec![self.expect_string_or_ident()?])
        }
    }

    fn parse_string_list(&mut self) -> Result<Vec<String>, ParseError> {
        self.parse_ident_list()
    }

    fn parse_config_block(&mut self) -> Result<IndexMap<String, ValueExpr>, ParseError> {
        self.expect(&Token::LBrace)?;
        let mut config = IndexMap::new();
        self.skip_whitespace();
        while !self.check(&Token::RBrace) && !self.check(&Token::Eof) {
            self.skip_whitespace();
            if self.check(&Token::RBrace) {
                break;
            }
            let key = self.expect_ident()?;
            self.expect(&Token::Equals)?;
            let val = self.parse_value_expr()?;
            config.insert(key, val);
            self.skip_newlines();
        }
        self.expect(&Token::RBrace)?;
        Ok(config)
    }

    // ─── Utility methods ────────────────────────────────────────────

    fn peek(&self) -> Token {
        self.tokens
            .get(self.pos)
            .map_or(Token::Eof, |t| t.token.clone())
    }

    fn current_span(&self) -> Span {
        self.tokens.get(self.pos).map_or(Span::empty(), |t| t.span)
    }

    fn advance(&mut self) {
        if self.pos < self.tokens.len() {
            self.pos += 1;
        }
    }

    fn check(&self, expected: &Token) -> bool {
        std::mem::discriminant(&self.peek()) == std::mem::discriminant(expected)
    }

    fn check_ident(&self, name: &str) -> bool {
        matches!(self.peek(), Token::Ident(ref s) if s == name)
    }

    fn expect(&mut self, expected: &Token) -> Result<(), ParseError> {
        if self.check(expected) {
            self.advance();
            Ok(())
        } else {
            Err(self.error(format!("expected {:?}, got {:?}", expected, self.peek())))
        }
    }

    fn expect_ident(&mut self) -> Result<String, ParseError> {
        match self.peek() {
            Token::Ident(s) => {
                self.advance();
                Ok(s)
            }
            // Allow keywords to be used as identifiers in name positions
            t if t.is_keyword() => {
                let s = self.token_to_string();
                self.advance();
                Ok(s)
            }
            _ => Err(self.error(format!("expected identifier, got {:?}", self.peek()))),
        }
    }

    fn expect_string_or_ident(&mut self) -> Result<String, ParseError> {
        match self.peek() {
            Token::StringLit(s) => {
                self.advance();
                Ok(s)
            }
            Token::Ident(s) => {
                self.advance();
                Ok(s)
            }
            t if t.is_keyword() => {
                let s = self.token_to_string();
                self.advance();
                Ok(s)
            }
            _ => Err(self.error(format!(
                "expected string or identifier, got {:?}",
                self.peek()
            ))),
        }
    }

    fn expect_int(&mut self) -> Result<i64, ParseError> {
        match self.peek() {
            Token::IntLit(n) => {
                self.advance();
                Ok(n)
            }
            _ => Err(self.error(format!("expected integer, got {:?}", self.peek()))),
        }
    }

    fn expect_field_path(&mut self) -> Result<String, ParseError> {
        let mut path = self.expect_ident()?;
        while self.check(&Token::Dot) {
            self.advance();
            path.push('.');
            path.push_str(&self.expect_ident()?);
        }
        Ok(path)
    }

    fn try_ident(&mut self) -> Option<String> {
        match self.peek() {
            Token::Ident(s) => {
                self.advance();
                Some(s)
            }
            _ => None,
        }
    }

    fn is_edge_ahead(&self) -> bool {
        // Look ahead for the pattern: ident -> ident via ident
        let mut i = self.pos + 1;
        // Skip whitespace
        while i < self.tokens.len()
            && matches!(self.tokens[i].token, Token::Newline | Token::Indent(_))
        {
            i += 1;
        }
        i < self.tokens.len() && matches!(self.tokens[i].token, Token::Arrow)
    }

    fn peek_ahead_is_pipe(&self) -> bool {
        // Look for | after the current token and possible whitespace
        let mut i = self.pos + 1;
        while i < self.tokens.len()
            && matches!(self.tokens[i].token, Token::Newline | Token::Indent(_))
        {
            i += 1;
        }
        i < self.tokens.len() && matches!(self.tokens[i].token, Token::Pipe)
    }

    fn at_section_content(&self) -> bool {
        !matches!(self.peek(), Token::SectionHeader(_) | Token::Eof)
    }

    fn skip_whitespace(&mut self) {
        while matches!(
            self.peek(),
            Token::Newline | Token::Indent(_) | Token::Comment(_)
        ) {
            self.advance();
        }
    }

    fn skip_newlines(&mut self) {
        while matches!(self.peek(), Token::Newline | Token::Indent(_)) {
            self.advance();
        }
    }

    fn skip_comma_or_newline(&mut self) {
        if self.check(&Token::Comma) {
            self.advance();
        }
        self.skip_newlines();
    }

    fn token_to_string(&self) -> String {
        match &self.peek() {
            Token::Ident(s) => s.clone(),
            Token::StringLit(s) => s.clone(),
            Token::IntLit(n) => n.to_string(),
            Token::FloatLit(n) => n.to_string(),
            Token::True => "true".into(),
            Token::False => "false".into(),
            Token::Null => "null".into(),
            Token::Graph => "graph".into(),
            Token::Node => "node".into(),
            Token::Channel => "channel".into(),
            Token::Memory => "memory".into(),
            Token::Via => "via".into(),
            Token::Domain => "domain".into(),
            Token::Template => "template".into(),
            Token::Import => "import".into(),
            Token::Extends => "extends".into(),
            Token::Signals => "signals".into(),
            Token::Resolver => "resolver".into(),
            Token::Schema => "schema".into(),
            Token::When => "when".into(),
            Token::Requires => "requires".into(),
            Token::Mutates => "mutates".into(),
            Token::Resolution => "resolution".into(),
            Token::Target => "target".into(),
            Token::Mode => "mode".into(),
            Token::Knows => "knows".into(),
            Token::Ignores => "ignores".into(),
            Token::Where => "where".into(),
            Token::Arrow => "->".into(),
            Token::EqEq => "==".into(),
            _ => format!("{:?}", self.peek()),
        }
    }

    fn error(&self, message: impl Into<String>) -> ParseError {
        ParseError::at(message, self.current_span())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_graph_file() {
        let source = r#"
graph "crypt_adventure" {
    domain rpg

    node client: client {}
    node narrator: llm {
        backend = "ollama"
        model = "llama3"
    }
    node engine: game_engine {}

    channel user_input { payload: { text: string } }

    client -> narrator via user_input
    narrator -> engine via narrative
    engine -> client via display

    memory narrator {
        capacity = 500
    }
}
"#;
        let file = Parser::parse(source).unwrap();
        let graph = match file {
            MclFile::Graph(g) => g,
            _ => panic!("expected graph file"),
        };

        assert_eq!(graph.name, "crypt_adventure");
        assert_eq!(graph.domain, Some("rpg".into()));
        assert_eq!(graph.nodes.len(), 3);
        assert_eq!(graph.nodes[0].name, "client");
        assert_eq!(graph.nodes[1].name, "narrator");
        assert_eq!(graph.nodes[1].node_type, "llm");
        assert_eq!(graph.channels.len(), 1);
        assert_eq!(graph.channels[0].name, "user_input");
        assert_eq!(graph.edges.len(), 3);
        assert_eq!(graph.edges[0].from, "client");
        assert_eq!(graph.edges[0].to, "narrator");
        assert_eq!(graph.edges[0].channel, "user_input");
        assert_eq!(graph.memories.len(), 1);
        assert_eq!(graph.memories[0].node, "narrator");
    }

    #[test]
    fn parse_context_file() {
        let source = r#"---
context: crypt_antechamber
type: room
domain: rpg
---

# State
lit: false
door_north: locked

# Entities
entity skeleton_guard: enemy {
    disposition: hostile
    hp: 12
    ac: 12
    knows: [guards_tomb, knows_nothing_of_key]
}

# Items
item rusty_key { on: floor }
item torch_sconce { on: wall }

# Actions
action light_torch {
    requires: [inventory.flint]
    mutates: { lit: true }
    "Light the torch sconce on the wall"
}

action attack {
    target: enum(entities where type == enemy)
    resolution: combat_system
    "Attack an enemy"
}

# Transitions
transition crypt_inner {
    when: door_north == unlocked
    direction: north
}

# Narrator
narrator {
    tone: dread
    voice: second_person
    tense: present
    forbidden: [epic, awesome, incredible]
    context: "Ancient crypt, smell of dust and decay."
}

# Knowledge
knows: guards_tomb
ignores: secret_passage

# Constraints
- The skeleton does not attack first
- The rusty key is only visible when lit
"#;
        let file = Parser::parse(source).unwrap();
        let ctx = match file {
            MclFile::Context(c) => *c,
            _ => panic!("expected context file"),
        };

        assert_eq!(ctx.frontmatter.get("context"), Some("crypt_antechamber"));
        assert_eq!(ctx.frontmatter.get("type"), Some("room"));
        assert_eq!(ctx.state.len(), 2);
        assert_eq!(ctx.state[0].name, "lit");
        assert_eq!(ctx.entities.len(), 1);
        assert_eq!(ctx.entities[0].name, "skeleton_guard");
        assert_eq!(ctx.entities[0].entity_type, Some("enemy".into()));
        assert_eq!(ctx.entities[0].knows.len(), 2);
        assert_eq!(ctx.items.len(), 2);
        assert_eq!(ctx.actions.len(), 2);
        assert_eq!(ctx.actions[0].name, "light_torch");
        assert_eq!(
            ctx.actions[0].description,
            Some("Light the torch sconce on the wall".into())
        );
        assert!(!ctx.actions[0].requires.is_empty());
        assert_eq!(ctx.actions[1].resolution, Some("combat_system".into()));
        assert_eq!(ctx.transitions.len(), 1);
        assert_eq!(ctx.transitions[0].target, "crypt_inner");
        assert!(ctx.narrator.is_some());
        let narrator = ctx.narrator.unwrap();
        assert_eq!(narrator.tone, Some("dread".into()));
        assert_eq!(narrator.forbidden.len(), 3);
        assert_eq!(ctx.knowledge.len(), 2);
        assert_eq!(ctx.constraints.len(), 2);
    }

    #[test]
    fn parse_domain_file() {
        let source = r#"
domain rpg {
    resolver combat_system
    resolver inventory_system

    type enemy { hp: int, ac: int, disposition: string }

    schema player {
        hp: int = 100
        inventory: [string] = []
    }
}
"#;
        let file = Parser::parse(source).unwrap();
        let dom = match file {
            MclFile::Domain(d) => d,
            _ => panic!("expected domain file"),
        };

        assert_eq!(dom.name, "rpg");
        assert_eq!(dom.resolvers, vec!["combat_system", "inventory_system"]);
        assert_eq!(dom.types.len(), 1);
        assert_eq!(dom.types[0].name, "enemy");
        assert_eq!(dom.schemas.len(), 1);
        assert_eq!(dom.schemas[0].name, "player");
    }

    #[test]
    fn parse_template_file() {
        let source = r#"
template skeleton_warrior {
    type: enemy
    disposition: hostile
    hp: 12
    ac: 12
}

template undead_caster extends skeleton_warrior {
    hp: 8
    abilities: [dark_bolt, shield]
}
"#;
        let file = Parser::parse(source).unwrap();
        let tmpl = match file {
            MclFile::Template(t) => t,
            _ => panic!("expected template file"),
        };

        assert_eq!(tmpl.templates.len(), 2);
        assert_eq!(tmpl.templates[0].name, "skeleton_warrior");
        assert_eq!(tmpl.templates[0].extends, None);
        assert_eq!(tmpl.templates[1].name, "undead_caster");
        assert_eq!(tmpl.templates[1].extends, Some("skeleton_warrior".into()));
    }

    #[test]
    fn parse_empty_string_returns_error() {
        let result = Parser::parse("");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("expected"),
            "error should mention what was expected: {err}"
        );
    }

    #[test]
    fn parse_garbage_returns_error() {
        let result = Parser::parse("this is not valid MCL at all !!!{}{{");
        assert!(result.is_err());
    }

    #[test]
    fn parse_incomplete_graph_returns_error() {
        let result = Parser::parse("graph \"test\" {");
        assert!(result.is_err());
    }

    #[test]
    fn parse_incomplete_context_still_parses() {
        let result = Parser::parse("---\ncontext: test\n---\n# State\nlit:");
        assert!(
            result.is_ok(),
            "parser is lenient: incomplete input still produces a context file"
        );
    }

    #[test]
    fn parse_malformed_frontmatter_returns_error() {
        let result = Parser::parse("---\ninvalid yaml {{{\n---\n");
        assert!(result.is_err());
    }

    #[test]
    fn parse_missing_context_type_returns_file() {
        let result = Parser::parse("---\ncontext: test_room\n---\n");
        assert!(result.is_ok());
        match result.unwrap() {
            MclFile::Context(_) => {}
            other => panic!("expected context file, got {other:?}"),
        }
    }

    #[test]
    fn parse_unclosed_brace_in_entity_still_parses() {
        let result = Parser::parse(
            r#"---
context: test
type: room
---
entity guard: enemy {
    hp: 12
"#,
        );
        assert!(
            result.is_ok(),
            "parser is lenient: unclosed braces still produce a file"
        );
    }

    #[test]
    fn parse_bogus_action_syntax_still_parses() {
        let result = Parser::parse(
            r#"---
context: test
type: room
---
action !bad name {
    target: enum(entities)
"#,
        );
        assert!(
            result.is_ok(),
            "parser is lenient: unknown tokens in actions are skipped"
        );
    }
}
