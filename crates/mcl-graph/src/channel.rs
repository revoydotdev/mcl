use indexmap::IndexMap;

use crate::id::NodeId;
use crate::message::Packet;

/// A named message channel between nodes (Pregel-style)
pub struct Channel {
    pub name: String,
    /// Packets pending for the current super-step
    pub buffer: Vec<Packet>,
    /// Nodes that read from this channel
    pub subscribers: Vec<NodeId>,
    /// Nodes that write to this channel
    pub publishers: Vec<NodeId>,
}

impl Channel {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            buffer: Vec::new(),
            subscribers: Vec::new(),
            publishers: Vec::new(),
        }
    }

    pub fn has_pending(&self) -> bool {
        !self.buffer.is_empty()
    }

    pub fn drain(&mut self) -> Vec<Packet> {
        std::mem::take(&mut self.buffer)
    }

    pub fn push(&mut self, packet: Packet) {
        self.buffer.push(packet);
    }
}

/// Registry of all channels in a graph
pub struct ChannelRegistry {
    channels: IndexMap<String, Channel>,
}

impl ChannelRegistry {
    pub fn new() -> Self {
        Self {
            channels: IndexMap::new(),
        }
    }

    /// Get or create a channel by name
    pub fn get_or_create(&mut self, name: &str) -> &mut Channel {
        self.channels
            .entry(name.to_string())
            .or_insert_with(|| Channel::new(name))
    }

    /// Get a channel by name
    pub fn get(&self, name: &str) -> Option<&Channel> {
        self.channels.get(name)
    }

    /// Get a mutable channel by name
    pub fn get_mut(&mut self, name: &str) -> Option<&mut Channel> {
        self.channels.get_mut(name)
    }

    /// Get all channels with pending packets
    pub fn pending_channels(&self) -> Vec<&str> {
        self.channels
            .iter()
            .filter(|(_, ch)| ch.has_pending())
            .map(|(name, _)| name.as_str())
            .collect()
    }

    /// Check if any channel has pending data
    pub fn has_pending(&self) -> bool {
        self.channels.values().any(|ch| ch.has_pending())
    }

    /// Push a packet into a channel
    pub fn push(&mut self, channel_name: &str, packet: Packet) {
        if let Some(ch) = self.channels.get_mut(channel_name) {
            ch.push(packet);
        }
    }
}

impl Default for ChannelRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_or_create_returns_same_channel() {
        let mut reg = ChannelRegistry::new();
        let ch1 = reg.get_or_create("test");
        ch1.push(Packet::text("test", NodeId::new("a"), 1, "hello"));

        let ch2 = reg.get_or_create("test");
        assert_eq!(
            ch2.buffer.len(),
            1,
            "get_or_create should return the same channel"
        );
    }

    #[test]
    fn test_get_or_create_creates_new_channel() {
        let mut reg = ChannelRegistry::new();
        let ch = reg.get_or_create("new_ch");
        assert_eq!(ch.name, "new_ch");
        assert!(ch.buffer.is_empty());
    }
}
