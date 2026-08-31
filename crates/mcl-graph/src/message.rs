use std::time::SystemTime;

use indexmap::IndexMap;
use mcl_spec::signal::MclSignal;
use mcl_spec::value::MclValue;
use serde::{Deserialize, Serialize};

use crate::id::NodeId;

/// Status of a packet in the A2A-inspired task lifecycle
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PacketStatus {
    /// Task in progress, more data may follow
    Pending,
    /// Final result
    Complete,
    /// Error occurred
    Failed,
}

/// Policy metadata attached to a packet by an interceptor verdict.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PacketPolicyMeta {
    pub policy_id: String,
    pub effect_flags: Vec<String>,
}

/// The unit of data flowing between nodes via channels
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Packet {
    /// Unique packet identifier
    pub id: u64,
    /// Channel this packet travels on
    pub channel: String,
    /// Source node
    pub origin: NodeId,
    /// Optional task lifecycle tracking
    pub task_id: Option<String>,
    /// Packet status
    pub status: PacketStatus,
    /// Structured payload data
    pub payload: MclValue,
    /// MCL signals extracted from this packet's content
    pub signals: Vec<MclSignal>,
    /// Freeform metadata
    pub meta: IndexMap<String, MclValue>,
    /// Policy metadata attached by interceptor
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_meta: Option<PacketPolicyMeta>,
    /// Creation timestamp
    pub timestamp: SystemTime,
}

impl Packet {
    /// Create a simple complete packet with a payload
    pub fn new(channel: impl Into<String>, origin: NodeId, id: u64, payload: MclValue) -> Self {
        Self {
            id,
            channel: channel.into(),
            origin,
            task_id: None,
            status: PacketStatus::Complete,
            payload,
            signals: Vec::new(),
            meta: IndexMap::new(),
            policy_meta: None,
            timestamp: SystemTime::now(),
        }
    }

    /// Create a text payload packet (convenience)
    pub fn text(
        channel: impl Into<String>,
        origin: NodeId,
        id: u64,
        text: impl Into<String>,
    ) -> Self {
        Self::new(channel, origin, id, MclValue::String(text.into()))
    }

    /// Extract text from payload, checking common keys
    pub fn text_content(&self) -> Option<&str> {
        match &self.payload {
            MclValue::String(s) => Some(s),
            MclValue::Map(m) => {
                for key in &["text", "content", "message"] {
                    if let Some(MclValue::String(s)) = m.get(*key) {
                        return Some(s);
                    }
                }
                None
            }
            _ => None,
        }
    }
}
