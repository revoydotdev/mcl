use crate::ast::{self, CmpOp, ConditionExpr, ContextFile, GraphFile, TargetExpr, ValueExpr};
use indexmap::IndexMap;
use mcl_graph::graph_def::{EdgeDef as GraphEdgeDef, GraphDef, NodeDef as GraphNodeDef};
use mcl_spec::document::{
    ActionDef, ContextType, EntityDef, Frontmatter, ItemDef, MclDocument, NarratorDef, TargetSpec,
    TransitionDef,
};
use mcl_spec::expression::{CmpOp as SpecCmpOp, Condition, FieldPath};
use mcl_spec::value::MclValue;

/// Convert a parsed graph file AST into a runtime GraphDef
pub fn emit_graph_def(graph: &GraphFile) -> GraphDef {
    let mut config = IndexMap::new();
    if let Some(ref d) = graph.domain {
        config.insert("domain".into(), MclValue::String(d.clone()));
    }

    let nodes = graph
        .nodes
        .iter()
        .map(|n| {
            let node_config = n
                .config
                .iter()
                .map(|(k, v)| (k.clone(), value_expr_to_mcl(v)))
                .collect();
            (
                n.name.clone(),
                GraphNodeDef {
                    node_type: n.node_type.clone(),
                    config: node_config,
                    priority: 0,
                },
            )
        })
        .collect();

    let edges = graph
        .edges
        .iter()
        .map(|e| GraphEdgeDef {
            from: e.from.clone(),
            to: e.to.clone(),
            channel: e.channel.clone(),
        })
        .collect();

    GraphDef {
        name: graph.name.clone(),
        config,
        nodes,
        edges,
    }
}

/// Convert a parsed context file AST into a runtime MclDocument
pub fn emit_mcl_document(ctx: &ContextFile) -> MclDocument {
    let frontmatter = Frontmatter {
        context: ctx.frontmatter.get("context").unwrap_or("").to_string(),
        context_type: ctx
            .frontmatter
            .get("type")
            .and_then(|s| s.parse::<ContextType>().ok())
            .unwrap_or(ContextType::Room),
        domain: ctx.frontmatter.get("domain").map(|s| s.to_string()),
        version: ctx.frontmatter.get("version").and_then(|s| s.parse().ok()),
        tags: Vec::new(),
        extends: ctx.frontmatter.get("extends").map(|s| s.to_string()),
        extra: IndexMap::new(),
        span: ctx.frontmatter.span,
    };

    let state = ctx
        .state
        .iter()
        .map(|v| {
            let val = v
                .default
                .as_ref()
                .map(value_expr_to_mcl)
                .unwrap_or(MclValue::Null);
            (v.name.clone(), val)
        })
        .collect();

    let entities = ctx
        .entities
        .iter()
        .map(|e| {
            let props = e
                .properties
                .iter()
                .map(|(k, v)| (k.clone(), value_expr_to_mcl(v)))
                .collect();
            (
                e.name.clone(),
                EntityDef {
                    name: e.name.clone(),
                    entity_type: e.entity_type.clone(),
                    disposition: e.properties.get("disposition").map(value_expr_to_string),
                    knowledge: e.knows.clone(),
                    properties: props,
                    span: e.span,
                },
            )
        })
        .collect();

    let items = ctx
        .items
        .iter()
        .map(|i| {
            let props = i
                .properties
                .iter()
                .map(|(k, v)| (k.clone(), value_expr_to_mcl(v)))
                .collect();
            (
                i.name.clone(),
                ItemDef {
                    name: i.name.clone(),
                    properties: props,
                    span: i.span,
                },
            )
        })
        .collect();

    let actions = ctx
        .actions
        .iter()
        .map(|a| {
            let requires = a.requires.iter().map(condition_to_spec).collect();
            let mutates = a
                .mutates
                .iter()
                .map(|(k, v)| (k.clone(), value_expr_to_mcl(v)))
                .collect();
            let target = a.target.as_ref().map(|t| match t {
                TargetExpr::Fixed(s) => TargetSpec::Fixed(s.clone()),
                TargetExpr::Enum { source, .. } => TargetSpec::Enum(FieldPath::new(
                    source.split('.').map(|s| s.to_string()).collect(),
                )),
            });
            (
                a.name.clone(),
                ActionDef {
                    name: a.name.clone(),
                    requires,
                    requires_any: Vec::new(),
                    target,
                    mutates,
                    resolution: a.resolution.clone(),
                    mode: a.mode.clone(),
                    args: IndexMap::new(),
                    cooldown: None,
                    description: a.description.clone(),
                    trigger_phrases: Vec::new(),
                    effects: Vec::new(),
                    span: a.span,
                },
            )
        })
        .collect();

    let transitions = ctx
        .transitions
        .iter()
        .map(|t| TransitionDef {
            target: t.target.clone(),
            when: t.when.as_ref().map(condition_to_spec),
            direction: t.direction.clone(),
            auto: t.auto_,
            narrate: None,
            span: t.span,
        })
        .collect();

    let narrator = ctx
        .narrator
        .as_ref()
        .map_or_else(NarratorDef::default, |n| NarratorDef {
            tone: n.tone.clone(),
            voice: n.voice.clone(),
            tense: n.tense.clone(),
            context: n.context.clone(),
            style_notes: n.style.clone(),
            forbidden: n.forbidden.clone(),
        });

    let knowledge = ctx
        .knowledge
        .iter()
        .map(|k| match k {
            ast::KnowledgeEntry::Knows(s) => mcl_spec::document::KnowledgeEntry::Known(s.clone()),
            ast::KnowledgeEntry::Ignores(s) => {
                mcl_spec::document::KnowledgeEntry::Unknown(s.clone())
            }
        })
        .collect();

    MclDocument {
        frontmatter,
        state,
        entities,
        items,
        actions,
        transitions,
        narrator,
        narrator_constraints: None,
        knowledge,
        constraints: ctx.constraints.clone(),
        sources: std::collections::BTreeMap::new(),
        labels: mcl_spec::label::Lattice::default(),
        degrade_rules: Vec::new(),
        policies: IndexMap::new(),
        effects: Vec::new(),
        span: ctx.span,
    }
}

fn value_expr_to_mcl(expr: &ValueExpr) -> MclValue {
    match expr {
        ValueExpr::Null => MclValue::Null,
        ValueExpr::Bool(b) => MclValue::Bool(*b),
        ValueExpr::Int(n) => MclValue::Int(*n),
        ValueExpr::Float(n) => MclValue::Float(*n),
        ValueExpr::String(s) => MclValue::String(s.clone()),
        ValueExpr::Ident(s) => MclValue::String(s.clone()),
        ValueExpr::List(items) => MclValue::List(items.iter().map(value_expr_to_mcl).collect()),
        ValueExpr::Map(m) => MclValue::Map(
            m.iter()
                .map(|(k, v)| (k.clone(), value_expr_to_mcl(v)))
                .collect(),
        ),
    }
}

fn value_expr_to_string(expr: &ValueExpr) -> String {
    match expr {
        ValueExpr::String(s) | ValueExpr::Ident(s) => s.clone(),
        other => format!("{other:?}"),
    }
}

fn condition_to_spec(cond: &ConditionExpr) -> Condition {
    match cond {
        ConditionExpr::Truthy(path) => Condition::Truthy(parse_field_path(path)),
        ConditionExpr::Falsy(path) => Condition::Falsy(parse_field_path(path)),
        ConditionExpr::Compare { left, op, right } => Condition::Compare {
            left: parse_field_path(left),
            op: match op {
                CmpOp::Eq => SpecCmpOp::Eq,
                CmpOp::Ne => SpecCmpOp::Ne,
                CmpOp::Gt => SpecCmpOp::Gt,
                CmpOp::Lt => SpecCmpOp::Lt,
                CmpOp::Ge => SpecCmpOp::Ge,
                CmpOp::Le => SpecCmpOp::Le,
            },
            right: value_expr_to_mcl(right),
        },
    }
}

fn parse_field_path(s: &str) -> FieldPath {
    FieldPath::new(s.split('.').map(|s| s.to_string()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::MclFile;
    use crate::Parser;

    #[test]
    fn emit_graph() {
        let file = Parser::parse(
            r#"
graph "test" {
    node client: client {}
    node narrator: llm {
        backend = "ollama"
    }
    client -> narrator via user_input
}
"#,
        )
        .unwrap();

        let graph = match file {
            MclFile::Graph(g) => g,
            _ => panic!("expected graph"),
        };

        let def = emit_graph_def(&graph);
        assert_eq!(def.name, "test");
        assert_eq!(def.nodes.len(), 2);
        assert_eq!(def.edges.len(), 1);
        assert_eq!(def.nodes["narrator"].node_type, "llm");
        assert_eq!(
            def.nodes["narrator"].config.get("backend"),
            Some(&MclValue::String("ollama".into()))
        );
    }

    #[test]
    fn emit_context() {
        let file = Parser::parse(
            r#"---
context: test_room
type: room
---
# State
lit: false

# Entities
entity guard: enemy {
    hp: 12
    knows: [guards_door]
}

# Actions
action attack {
    target: enum(entities)
    resolution: combat_system
    "Attack an enemy"
}

# Transitions
transition next_room {
    when: door == unlocked
    direction: north
}
"#,
        )
        .unwrap();

        let ctx = match file {
            MclFile::Context(ref c) => c.as_ref(),
            _ => panic!("expected context"),
        };

        let doc = emit_mcl_document(ctx);
        assert_eq!(doc.frontmatter.context, "test_room");
        assert_eq!(doc.frontmatter.context_type, ContextType::Room);
        assert_eq!(doc.state.len(), 1);
        assert_eq!(doc.state.get("lit"), Some(&MclValue::Bool(false)));
        assert_eq!(doc.entities.len(), 1);
        assert_eq!(doc.entities["guard"].entity_type, Some("enemy".into()));
        assert_eq!(doc.actions.len(), 1);
        assert_eq!(
            doc.actions["attack"].resolution,
            Some("combat_system".into())
        );
        assert_eq!(doc.transitions.len(), 1);
    }
}
