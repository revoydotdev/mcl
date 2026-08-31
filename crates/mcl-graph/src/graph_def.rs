use std::path::Path;

use indexmap::IndexMap;
use mcl_spec::value::MclValue;
use serde::{Deserialize, Serialize};

use crate::error::GraphError;

/// The parsed graph definition — format-agnostic
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphDef {
    pub name: String,
    pub config: IndexMap<String, MclValue>,
    pub nodes: IndexMap<String, NodeDef>,
    pub edges: Vec<EdgeDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeDef {
    pub node_type: String,
    #[serde(default)]
    pub config: IndexMap<String, MclValue>,
    #[serde(default)]
    pub priority: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeDef {
    pub from: String,
    pub to: String,
    pub channel: String,
}

/// Pluggable parser trait for graph definitions.
/// The format is intentionally abstract — a bespoke *AML language will replace
/// the placeholder TOML loader.
pub trait GraphDefParser: Send + Sync {
    fn parse(&self, source: &str) -> Result<GraphDef, GraphError>;

    fn parse_file(&self, path: &Path) -> Result<GraphDef, GraphError> {
        let source = std::fs::read_to_string(path)
            .map_err(|e| GraphError::Io(format!("{}: {e}", path.display())))?;
        self.parse(&source)
    }
}

// ─── Placeholder TOML loader ────────────────────────────────────────────

/// Minimal TOML graph definition parser. Temporary placeholder until
/// the custom *AML language is designed.
pub struct TomlGraphDefParser;

#[derive(Deserialize)]
struct TomlGraphFile {
    graph: TomlGraphMeta,
    #[serde(default)]
    nodes: IndexMap<String, TomlNodeDef>,
    #[serde(default)]
    edges: Vec<TomlEdgeDef>,
}

#[derive(Deserialize)]
struct TomlGraphMeta {
    name: String,
    #[serde(flatten)]
    config: IndexMap<String, toml::Value>,
}

#[derive(Deserialize)]
struct TomlNodeDef {
    #[serde(rename = "type")]
    node_type: String,
    #[serde(default)]
    config: IndexMap<String, toml::Value>,
    #[serde(default)]
    priority: u32,
}

#[derive(Deserialize)]
struct TomlEdgeDef {
    from: String,
    to: String,
    channel: String,
}

impl GraphDefParser for TomlGraphDefParser {
    fn parse(&self, source: &str) -> Result<GraphDef, GraphError> {
        let file: TomlGraphFile = toml::from_str(source)
            .map_err(|e| GraphError::DefError(format!("TOML parse error: {e}")))?;

        let nodes = file
            .nodes
            .into_iter()
            .map(|(name, def)| {
                let config = def
                    .config
                    .into_iter()
                    .map(|(k, v)| (k, toml_to_mcl(v)))
                    .collect();
                (
                    name,
                    NodeDef {
                        node_type: def.node_type,
                        config,
                        priority: def.priority,
                    },
                )
            })
            .collect();

        let edges = file
            .edges
            .into_iter()
            .map(|e| EdgeDef {
                from: e.from,
                to: e.to,
                channel: e.channel,
            })
            .collect();

        let config = file
            .graph
            .config
            .into_iter()
            .map(|(k, v)| (k, toml_to_mcl(v)))
            .collect();

        Ok(GraphDef {
            name: file.graph.name,
            config,
            nodes,
            edges,
        })
    }
}

fn toml_to_mcl(v: toml::Value) -> MclValue {
    match v {
        toml::Value::String(s) => MclValue::String(s),
        toml::Value::Integer(n) => MclValue::Int(n),
        toml::Value::Float(f) => MclValue::Float(f),
        toml::Value::Boolean(b) => MclValue::Bool(b),
        toml::Value::Array(arr) => MclValue::List(arr.into_iter().map(toml_to_mcl).collect()),
        toml::Value::Table(tbl) => {
            MclValue::Map(tbl.into_iter().map(|(k, v)| (k, toml_to_mcl(v))).collect())
        }
        toml::Value::Datetime(dt) => MclValue::String(dt.to_string()),
    }
}

/// Export a graph definition as a DOT (Graphviz) string.
pub fn export_dot(def: &GraphDef) -> String {
    let mut lines = vec![format!("digraph {} {{", def.name)];

    for (name, node) in &def.nodes {
        lines.push(format!(
            r#"    "{}" [label="{} ({})", shape=box];"#,
            name, name, node.node_type
        ));
    }

    if !def.edges.is_empty() {
        lines.push(String::new());
    }

    for edge in &def.edges {
        lines.push(format!(
            r#"    "{}" -> "{}" [label="{}"];"#,
            edge.from, edge.to, edge.channel
        ));
    }

    lines.push("}".to_string());
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graph_exports_to_dot() {
        let toml = r#"
[graph]
name = "test"

[nodes.client]
type = "client"

[nodes.narrator]
type = "llm"

[nodes.engine]
type = "game_engine"

[[edges]]
from = "client"
to = "narrator"
channel = "user_input"

[[edges]]
from = "narrator"
to = "engine"
channel = "narrative"

[[edges]]
from = "engine"
to = "client"
channel = "display"
"#;
        let def = TomlGraphDefParser.parse(toml).unwrap();
        let dot = export_dot(&def);
        assert!(dot.contains("digraph"));
        assert!(dot.contains("client"));
        assert!(dot.contains("->"));
        assert!(dot.contains("user_input"));
    }

    #[test]
    fn parse_node_priority() {
        let toml = r#"
[graph]
name = "priority_test"

[nodes.narrator]
type = "llm"
priority = 1

[nodes.advisor]
type = "llm"
priority = 2

[nodes.client]
type = "client"

[[edges]]
from = "client"
to = "narrator"
channel = "user_input"
"#;
        let def = TomlGraphDefParser.parse(toml).unwrap();
        assert_eq!(def.nodes["narrator"].priority, 1);
        assert_eq!(def.nodes["advisor"].priority, 2);
        assert_eq!(def.nodes["client"].priority, 0); // default
    }

    #[test]
    fn parse_basic_toml_graph() {
        let toml = r#"
[graph]
name = "test_graph"

[nodes.client]
type = "client"

[nodes.narrator]
type = "llm"
config.backend = "ollama"
config.model = "llama3"

[[edges]]
from = "client"
to = "narrator"
channel = "user_input"

[[edges]]
from = "narrator"
to = "client"
channel = "display"
"#;

        let parser = TomlGraphDefParser;
        let def = parser.parse(toml).unwrap();

        assert_eq!(def.name, "test_graph");
        assert_eq!(def.nodes.len(), 2);
        assert_eq!(def.edges.len(), 2);

        assert_eq!(def.nodes["client"].node_type, "client");
        assert_eq!(def.nodes["narrator"].node_type, "llm");
        assert_eq!(
            def.nodes["narrator"].config.get("backend"),
            Some(&MclValue::String("ollama".into()))
        );

        assert_eq!(def.edges[0].from, "client");
        assert_eq!(def.edges[0].channel, "user_input");
    }
}
