use std::sync::Arc;

use crate::context::NodeContext;
use crate::message::Packet;

/// Trait for hooks that observe packet flow through the graph.
///
/// Implementations are called by the executor during each super-step:
/// - `on_packet_in`: after a node successfully processes incoming packets
/// - `on_packet_out`: after a node's outbox is drained and packets are routed
///
/// Hooks are stored on [`GraphExecutor`] and shared across all nodes.
/// They receive a mutable reference to the node's context so they can
/// inspect or modify state (e.g., populate memory stores).
pub trait LifecycleHook: Send + Sync {
    /// Called for each incoming packet after a node processes it successfully.
    fn on_packet_in(&self, node_id: &str, channel: &str, packet: &Packet, ctx: &mut NodeContext);

    /// Called for each outgoing packet after the node's outbox is drained.
    fn on_packet_out(&self, node_id: &str, channel: &str, packet: &Packet, ctx: &mut NodeContext);
}

/// A hook that auto-populates node memory stores when packets arrive and depart.
///
/// - `on_packet_in`: stores incoming packet text in `memory.incoming`
/// - `on_packet_out`: stores outgoing packet text in `memory.outgoing`
///
/// No-op when the node's memory is not configured (i.e., `ctx.memory()` returns `None`).
pub struct MemoryPopulationHook;

impl MemoryPopulationHook {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MemoryPopulationHook {
    fn default() -> Self {
        Self::new()
    }
}

impl LifecycleHook for MemoryPopulationHook {
    fn on_packet_in(&self, node_id: &str, channel: &str, packet: &Packet, ctx: &mut NodeContext) {
        let Some(memory) = ctx.memory_mut() else {
            return;
        };
        let text = match packet.text_content() {
            Some(t) => t.to_string(),
            None => return,
        };
        memory.incoming.insert(
            packet.payload.clone(),
            &text,
            vec![
                mcl_memory::MemoryTag::NodeId(node_id.to_string()),
                mcl_memory::MemoryTag::Scope(format!("in:{channel}")),
            ],
        );
    }

    fn on_packet_out(&self, node_id: &str, channel: &str, packet: &Packet, ctx: &mut NodeContext) {
        let Some(memory) = ctx.memory_mut() else {
            return;
        };
        let text = match packet.text_content() {
            Some(t) => t.to_string(),
            None => return,
        };
        memory.outgoing.insert(
            packet.payload.clone(),
            &text,
            vec![
                mcl_memory::MemoryTag::NodeId(node_id.to_string()),
                mcl_memory::MemoryTag::Scope(format!("out:{channel}")),
            ],
        );
    }
}

/// Convenience: wrap a hook in an `Arc`.
pub fn hook(h: impl LifecycleHook + 'static) -> Arc<dyn LifecycleHook> {
    Arc::new(h)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::NodeContext;
    use crate::id::NodeId;
    use crate::message::Packet;
    use indexmap::IndexMap;
    use mcl_spec::value::MclValue;

    /// A test hook that records all invocations.
    struct RecordingHook {
        incoming: std::sync::Mutex<Vec<(String, String, String)>>,
        outgoing: std::sync::Mutex<Vec<(String, String, String)>>,
    }

    impl RecordingHook {
        fn new() -> Self {
            Self {
                incoming: std::sync::Mutex::new(Vec::new()),
                outgoing: std::sync::Mutex::new(Vec::new()),
            }
        }

        fn incoming_count(&self) -> usize {
            self.incoming.lock().unwrap().len()
        }

        fn outgoing_count(&self) -> usize {
            self.outgoing.lock().unwrap().len()
        }

        fn last_incoming(&self) -> Option<(String, String, String)> {
            self.incoming.lock().unwrap().last().cloned()
        }

        fn last_outgoing(&self) -> Option<(String, String, String)> {
            self.outgoing.lock().unwrap().last().cloned()
        }
    }

    impl LifecycleHook for RecordingHook {
        fn on_packet_in(
            &self,
            node_id: &str,
            channel: &str,
            packet: &Packet,
            _ctx: &mut NodeContext,
        ) {
            let text = packet.text_content().unwrap_or("").to_string();
            self.incoming
                .lock()
                .unwrap()
                .push((node_id.to_string(), channel.to_string(), text));
        }

        fn on_packet_out(
            &self,
            node_id: &str,
            channel: &str,
            packet: &Packet,
            _ctx: &mut NodeContext,
        ) {
            let text = packet.text_content().unwrap_or("").to_string();
            self.outgoing
                .lock()
                .unwrap()
                .push((node_id.to_string(), channel.to_string(), text));
        }
    }

    #[test]
    fn recording_hook_tracks_incoming() {
        let h = RecordingHook::new();
        let mut ctx = NodeContext::new(NodeId::new("test"), IndexMap::new());
        let pkt = Packet::text("in", NodeId::new("src"), 1, "hello");

        h.on_packet_in("test", "in", &pkt, &mut ctx);

        assert_eq!(h.incoming_count(), 1);
        let (nid, ch, txt) = h.last_incoming().unwrap();
        assert_eq!(nid, "test");
        assert_eq!(ch, "in");
        assert_eq!(txt, "hello");
    }

    #[test]
    fn recording_hook_tracks_outgoing() {
        let h = RecordingHook::new();
        let mut ctx = NodeContext::new(NodeId::new("test"), IndexMap::new());
        let pkt = Packet::text("out", NodeId::new("test"), 1, "world");

        h.on_packet_out("test", "out", &pkt, &mut ctx);

        assert_eq!(h.outgoing_count(), 1);
        let (nid, ch, txt) = h.last_outgoing().unwrap();
        assert_eq!(nid, "test");
        assert_eq!(ch, "out");
        assert_eq!(txt, "world");
    }

    #[test]
    fn memory_population_hook_noop_without_memory() {
        let h = MemoryPopulationHook::new();
        let mut ctx = NodeContext::new(NodeId::new("test"), IndexMap::new());
        let pkt = Packet::text("in", NodeId::new("src"), 1, "hello");

        // Should not panic when memory is None
        h.on_packet_in("test", "in", &pkt, &mut ctx);
        h.on_packet_out("test", "out", &pkt, &mut ctx);
    }

    #[test]
    fn memory_population_hook_stores_incoming() {
        let h = MemoryPopulationHook::new();
        let mut ctx = NodeContext::new(NodeId::new("node_a"), IndexMap::new());

        // Attach mock memory
        let embedder = mcl_memory::create_mock_embedder(384);
        let memory = mcl_memory::NodeMemory::new(100, embedder, 384).unwrap();
        ctx.set_memory(Some(memory));

        let pkt = Packet::text("in", NodeId::new("src"), 1, "incoming text");
        h.on_packet_in("node_a", "in", &pkt, &mut ctx);

        let mem = ctx.memory().unwrap();
        assert_eq!(mem.incoming.len(), 1);
        assert_eq!(mem.incoming.recent(1)[0].text, "incoming text");
    }

    #[test]
    fn memory_population_hook_stores_outgoing() {
        let h = MemoryPopulationHook::new();
        let mut ctx = NodeContext::new(NodeId::new("node_a"), IndexMap::new());

        let embedder = mcl_memory::create_mock_embedder(384);
        let memory = mcl_memory::NodeMemory::new(100, embedder, 384).unwrap();
        ctx.set_memory(Some(memory));

        let pkt = Packet::text("out", NodeId::new("node_a"), 1, "outgoing text");
        h.on_packet_out("node_a", "out", &pkt, &mut ctx);

        let mem = ctx.memory().unwrap();
        assert_eq!(mem.outgoing.len(), 1);
        assert_eq!(mem.outgoing.recent(1)[0].text, "outgoing text");
    }

    #[test]
    fn memory_population_hook_tags_include_node_and_channel() {
        let h = MemoryPopulationHook::new();
        let mut ctx = NodeContext::new(NodeId::new("my_node"), IndexMap::new());

        let embedder = mcl_memory::create_mock_embedder(384);
        let memory = mcl_memory::NodeMemory::new(100, embedder, 384).unwrap();
        ctx.set_memory(Some(memory));

        let pkt = Packet::text("data_ch", NodeId::new("src"), 1, "payload");
        h.on_packet_in("my_node", "data_ch", &pkt, &mut ctx);

        let mem = ctx.memory().unwrap();
        let entry = &mem.incoming.recent(1)[0];
        assert!(entry
            .tags
            .iter()
            .any(|t| matches!(t, mcl_memory::MemoryTag::NodeId(n) if n == "my_node")));
        assert!(entry
            .tags
            .iter()
            .any(|t| matches!(t, mcl_memory::MemoryTag::Scope(s) if s == "in:data_ch")));
    }

    #[test]
    fn memory_population_hook_skips_non_text_packets() {
        let h = MemoryPopulationHook::new();
        let mut ctx = NodeContext::new(NodeId::new("test"), IndexMap::new());

        let embedder = mcl_memory::create_mock_embedder(384);
        let memory = mcl_memory::NodeMemory::new(100, embedder, 384).unwrap();
        ctx.set_memory(Some(memory));

        // Packet with numeric payload (no text_content)
        let pkt = Packet::new("in", NodeId::new("src"), 1, MclValue::Int(42));
        h.on_packet_in("test", "in", &pkt, &mut ctx);

        let mem = ctx.memory().unwrap();
        assert_eq!(mem.incoming.len(), 0, "non-text packets should be skipped");
    }

    #[test]
    fn memory_population_hook_incoming_and_outgoing_independent() {
        let h = MemoryPopulationHook::new();
        let mut ctx = NodeContext::new(NodeId::new("test"), IndexMap::new());

        let embedder = mcl_memory::create_mock_embedder(384);
        let memory = mcl_memory::NodeMemory::new(100, embedder, 384).unwrap();
        ctx.set_memory(Some(memory));

        let in_pkt = Packet::text("in", NodeId::new("src"), 1, "incoming");
        let out_pkt = Packet::text("out", NodeId::new("test"), 1, "outgoing");

        h.on_packet_in("test", "in", &in_pkt, &mut ctx);
        h.on_packet_out("test", "out", &out_pkt, &mut ctx);

        let mem = ctx.memory().unwrap();
        assert_eq!(mem.incoming.len(), 1);
        assert_eq!(mem.outgoing.len(), 1);
        assert_eq!(mem.local.len(), 0);
        assert_eq!(mem.incoming.recent(1)[0].text, "incoming");
        assert_eq!(mem.outgoing.recent(1)[0].text, "outgoing");
    }

    #[test]
    fn hook_helper_creates_arc() {
        let h: Arc<dyn LifecycleHook> = hook(RecordingHook::new());
        assert_eq!(Arc::strong_count(&h), 1);
    }
}
