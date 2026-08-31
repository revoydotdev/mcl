use std::collections::HashSet;

use crate::ast::{ContextFile, DomainFile, GraphFile, MclFile, TemplateFile};

/// Validation error
#[derive(Debug)]
pub struct ValidationError {
    pub message: String,
    pub span: mcl_spec::Span,
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.span, self.message)
    }
}

/// Validate a parsed MCL file, returning all errors found
pub fn validate(file: &MclFile) -> Vec<ValidationError> {
    match file {
        MclFile::Graph(g) => validate_graph(g),
        MclFile::Context(c) => validate_context(c),
        MclFile::Domain(d) => validate_domain(d),
        MclFile::Template(t) => validate_template(t),
    }
}

fn validate_graph(graph: &GraphFile) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    // Check for duplicate node names
    let mut seen_nodes = HashSet::new();
    for node in &graph.nodes {
        if !seen_nodes.insert(&node.name) {
            errors.push(ValidationError {
                message: format!("duplicate node name `{}`", node.name),
                span: node.span,
            });
        }
    }

    // Check for duplicate channel names
    let mut seen_channels = HashSet::new();
    for ch in &graph.channels {
        if !seen_channels.insert(&ch.name) {
            errors.push(ValidationError {
                message: format!("duplicate channel name `{}`", ch.name),
                span: ch.span,
            });
        }
    }

    // Validate edges reference existing nodes
    let node_names: HashSet<&str> = graph.nodes.iter().map(|n| n.name.as_str()).collect();
    for edge in &graph.edges {
        if !node_names.contains(edge.from.as_str()) {
            errors.push(ValidationError {
                message: format!("edge references unknown node `{}`", edge.from),
                span: edge.span,
            });
        }
        if !node_names.contains(edge.to.as_str()) {
            errors.push(ValidationError {
                message: format!("edge references unknown node `{}`", edge.to),
                span: edge.span,
            });
        }
    }

    // Validate memory declarations reference existing nodes
    for mem in &graph.memories {
        if !node_names.contains(mem.node.as_str()) {
            errors.push(ValidationError {
                message: format!("memory config references unknown node `{}`", mem.node),
                span: mem.span,
            });
        }
    }

    // Check for self-loops
    for edge in &graph.edges {
        if edge.from == edge.to {
            errors.push(ValidationError {
                message: format!("self-loop on node `{}`", edge.from),
                span: edge.span,
            });
        }
    }

    errors
}

fn validate_context(ctx: &ContextFile) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    // Required frontmatter fields
    if ctx.frontmatter.get("context").is_none() {
        errors.push(ValidationError {
            message: "frontmatter missing required `context` field".into(),
            span: ctx.frontmatter.span,
        });
    }
    if ctx.frontmatter.get("type").is_none() {
        errors.push(ValidationError {
            message: "frontmatter missing required `type` field".into(),
            span: ctx.frontmatter.span,
        });
    }

    // Check for duplicate entity names
    let mut seen = HashSet::new();
    for entity in &ctx.entities {
        if !seen.insert(&entity.name) {
            errors.push(ValidationError {
                message: format!("duplicate entity `{}`", entity.name),
                span: entity.span,
            });
        }
    }

    // Check for duplicate action names
    seen.clear();
    for action in &ctx.actions {
        if !seen.insert(&action.name) {
            errors.push(ValidationError {
                message: format!("duplicate action `{}`", action.name),
                span: action.span,
            });
        }
    }

    // Check for duplicate state variable names
    let mut seen_state = HashSet::new();
    for var in &ctx.state {
        if !seen_state.insert(&var.name) {
            errors.push(ValidationError {
                message: format!("duplicate state variable `{}`", var.name),
                span: var.span,
            });
        }
    }

    // Actions with resolution must not also have mutates
    for action in &ctx.actions {
        if action.resolution.is_some() && !action.mutates.is_empty() {
            errors.push(ValidationError {
                message: format!(
                    "action `{}` has both `resolution` and `mutates` — use one or the other",
                    action.name
                ),
                span: action.span,
            });
        }
    }

    errors
}

fn validate_domain(domain: &DomainFile) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    // Check for duplicate resolver names
    let mut seen = HashSet::new();
    for r in &domain.resolvers {
        if !seen.insert(r) {
            errors.push(ValidationError {
                message: format!("duplicate resolver `{r}`"),
                span: domain.span,
            });
        }
    }

    // Check for duplicate type names
    seen.clear();
    for t in &domain.types {
        if !seen.insert(&t.name) {
            errors.push(ValidationError {
                message: format!("duplicate type `{}`", t.name),
                span: t.span,
            });
        }
    }

    errors
}

fn validate_template(tmpl: &TemplateFile) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    // Check for duplicate template names
    let mut seen = HashSet::new();
    for t in &tmpl.templates {
        if !seen.insert(&t.name) {
            errors.push(ValidationError {
                message: format!("duplicate template `{}`", t.name),
                span: t.span,
            });
        }
    }

    // Check extends references exist within the file
    let names: HashSet<&str> = tmpl.templates.iter().map(|t| t.name.as_str()).collect();
    for t in &tmpl.templates {
        if let Some(ref base) = t.extends {
            if !names.contains(base.as_str()) {
                errors.push(ValidationError {
                    message: format!("template `{}` extends unknown template `{base}`", t.name),
                    span: t.span,
                });
            }
        }
    }

    errors
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Parser;

    #[test]
    fn validate_valid_graph() {
        let file = Parser::parse(
            r#"
graph "test" {
    node a: client {}
    node b: llm {}
    a -> b via user_input
}
"#,
        )
        .unwrap();
        assert!(validate(&file).is_empty());
    }

    #[test]
    fn catch_duplicate_nodes() {
        let file = Parser::parse(
            r#"
graph "test" {
    node a: client {}
    node a: llm {}
}
"#,
        )
        .unwrap();
        let errors = validate(&file);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("duplicate node"));
    }

    #[test]
    fn catch_dangling_edge() {
        let file = Parser::parse(
            r#"
graph "test" {
    node a: client {}
    a -> phantom via data
}
"#,
        )
        .unwrap();
        let errors = validate(&file);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("unknown node `phantom`"));
    }

    #[test]
    fn catch_self_loop() {
        let file = Parser::parse(
            r#"
graph "test" {
    node a: client {}
    a -> a via loopback
}
"#,
        )
        .unwrap();
        let errors = validate(&file);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("self-loop"));
    }

    #[test]
    fn catch_missing_frontmatter() {
        let file = Parser::parse("---\ndomain: rpg\n---\n").unwrap();
        let errors = validate(&file);
        assert!(errors.iter().any(|e| e.message.contains("context")));
        assert!(errors.iter().any(|e| e.message.contains("type")));
    }

    #[test]
    fn catch_resolution_with_mutates() {
        let file = Parser::parse(
            r#"---
context: test
type: room
---
# Actions
action bad {
    resolution: combat_system
    mutates: { hp: 0 }
}
"#,
        )
        .unwrap();
        let errors = validate(&file);
        assert_eq!(errors.len(), 1);
        assert!(errors[0]
            .message
            .contains("both `resolution` and `mutates`"));
    }

    #[test]
    fn catch_broken_extends() {
        let file = Parser::parse(
            r#"
template child extends nonexistent {
    hp: 10
}
"#,
        )
        .unwrap();
        let errors = validate(&file);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("unknown template"));
    }
}
