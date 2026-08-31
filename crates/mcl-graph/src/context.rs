use indexmap::IndexMap;
use mcl_spec::value::MclValue;

use crate::id::NodeId;
use crate::message::{Packet, PacketStatus};

pub struct NodeContext {
    pub node_id: NodeId,
    pub config: IndexMap<String, MclValue>,
    memory: Option<mcl_memory::NodeMemory>,
    outbox: Vec<(String, Packet)>,
    seq: u64,
}

impl NodeContext {
    pub fn new(node_id: NodeId, config: IndexMap<String, MclValue>) -> Self {
        Self {
            node_id,
            config,
            memory: None,
            outbox: Vec::new(),
            seq: 0,
        }
    }

    pub fn memory(&self) -> Option<&mcl_memory::NodeMemory> {
        self.memory.as_ref()
    }

    pub fn memory_mut(&mut self) -> Option<&mut mcl_memory::NodeMemory> {
        self.memory.as_mut()
    }

    pub fn set_memory(&mut self, memory: Option<mcl_memory::NodeMemory>) {
        self.memory = memory;
    }

    /// Take ownership of the memory, leaving `None` in its place.
    pub fn take_memory(&mut self) -> Option<mcl_memory::NodeMemory> {
        self.memory.take()
    }

    /// Emit a packet on the named output channel
    pub fn emit(&mut self, channel: &str, payload: MclValue) {
        self.seq += 1;
        let packet = Packet::new(channel, self.node_id.clone(), self.seq, payload);
        self.outbox.push((channel.to_string(), packet));
    }

    /// Emit a text packet (convenience)
    pub fn emit_text(&mut self, channel: &str, text: impl Into<String>) {
        self.seq += 1;
        let packet = Packet::text(channel, self.node_id.clone(), self.seq, text);
        self.outbox.push((channel.to_string(), packet));
    }

    /// Emit a full packet with custom fields
    pub fn emit_packet(&mut self, channel: &str, mut packet: Packet) {
        self.seq += 1;
        packet.id = self.seq;
        packet.origin = self.node_id.clone();
        self.outbox.push((channel.to_string(), packet));
    }

    /// Emit an error packet
    pub fn emit_error(&mut self, channel: &str, error: impl Into<String>) {
        self.seq += 1;
        let mut packet = Packet::text(channel, self.node_id.clone(), self.seq, error);
        packet.status = PacketStatus::Failed;
        self.outbox.push((channel.to_string(), packet));
    }

    /// Drain all outbound packets (called by executor after process())
    pub fn drain_outbox(&mut self) -> Vec<(String, Packet)> {
        std::mem::take(&mut self.outbox)
    }

    /// Get a config value by key
    pub fn config_str(&self, key: &str) -> Option<&str> {
        self.config.get(key).and_then(|v| v.as_str())
    }

    /// Get a config value as bool
    pub fn config_bool(&self, key: &str) -> Option<bool> {
        self.config.get(key).and_then(|v| v.as_bool())
    }
}
