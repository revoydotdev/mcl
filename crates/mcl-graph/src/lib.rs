//! Node-graph engine: Node trait, channels, and Pregel executor.
//!
//! Executes node graphs with deterministic super-step processing.

pub mod channel;
pub mod context;
pub mod error;
pub mod executor;
pub mod factory;
pub mod graph_def;
pub mod id;
pub mod interceptor;
pub mod lifecycle;
pub mod message;
pub mod node;
pub mod scheduler;

pub use context::NodeContext;
pub use error::GraphError;
pub use executor::GraphExecutor;
pub use factory::NodeFactory;
pub use graph_def::{export_dot, GraphDef, GraphDefParser, NodeDef, TomlGraphDefParser};
pub use id::NodeId;
pub use interceptor::{InterceptorVerdict, PacketInterceptor};
pub use lifecycle::{LifecycleHook, MemoryPopulationHook};
pub use message::{Packet, PacketPolicyMeta, PacketStatus};
pub use node::{ChannelKind, ChannelSpec, GovernanceKind, GoverningNode, Node, NodeCapability};
