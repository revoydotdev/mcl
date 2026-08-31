use async_trait::async_trait;

use crate::context::NodeContext;
use crate::error::GraphError;
use crate::message::Packet;

/// Whether a channel subscription targets a named channel or all channels.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub enum ChannelKind {
    #[default]
    Named,
    Wildcard,
}

/// Describes a channel subscription or publication.
///
/// When `kind` is `ChannelKind::Wildcard`, the node subscribes to every
/// published channel regardless of name.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChannelSpec {
    pub name: String,
    pub schema: Option<serde_json::Value>,
    pub kind: ChannelKind,
}

impl ChannelSpec {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            schema: None,
            kind: ChannelKind::Named,
        }
    }

    pub fn wildcard() -> Self {
        Self {
            name: String::new(),
            schema: None,
            kind: ChannelKind::Wildcard,
        }
    }

    pub fn is_wildcard(&self) -> bool {
        self.kind == ChannelKind::Wildcard
    }
}

/// A2A-inspired capability card declaring what a node can do
#[derive(Debug, Clone)]
pub struct NodeCapability {
    /// Node type name (e.g., "llm", "combat_resolver")
    pub type_name: String,
    /// Human-readable description
    pub description: String,
    /// Input channels this node subscribes to
    pub subscriptions: Vec<ChannelSpec>,
    /// Output channels this node publishes to
    pub publications: Vec<ChannelSpec>,
}

/// Kind of governance a [`GoverningNode`] performs.
///
/// Governing nodes are special nodes that oversee the graph's operation
/// rather than participating in a single domain.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum GovernanceKind {
    /// Summarises graph activity into condensed outputs.
    Summarizer,
    /// Schedules or re-orders work across nodes.
    Scheduler,
    /// Maintains a registry of nodes, channels, or capabilities.
    Registry,
}

/// A node that governs the graph rather than processing a single domain.
///
/// Governing nodes typically hold wildcard subscriptions so they observe
/// every packet flowing through the graph.
#[async_trait]
pub trait GoverningNode: Node {
    /// Which flavour of governance this node provides.
    fn governance_kind(&self) -> GovernanceKind;
}

/// The core node trait. All nodes in the graph implement this.
#[async_trait]
pub trait Node: Send + Sync {
    /// Declare this node's capabilities and channel requirements
    fn capability(&self) -> NodeCapability;

    /// Initialize the node. Called once before the graph starts processing.
    async fn init(&mut self, ctx: &mut NodeContext) -> Result<(), GraphError>;

    /// Process a batch of packets arriving on a channel.
    /// Called during a super-step when the channel has pending data.
    /// The node emits output via `ctx.emit()`.
    async fn process(
        &mut self,
        channel: &str,
        packets: &[Packet],
        ctx: &mut NodeContext,
    ) -> Result<(), GraphError>;

    /// Called when the graph is shutting down. Flush state, close resources.
    async fn shutdown(&mut self, _ctx: &mut NodeContext) -> Result<(), GraphError> {
        Ok(())
    }

    /// Whether this node is a "source" — produces packets without receiving them.
    /// Source nodes get polled by the executor between super-steps.
    fn is_source(&self) -> bool {
        false
    }

    /// Poll a source node for new packets (only called if is_source() returns true).
    /// Returns None if no input is available yet.
    async fn poll_source(&mut self, _ctx: &mut NodeContext) -> Result<Option<Packet>, GraphError> {
        Ok(None)
    }
}
