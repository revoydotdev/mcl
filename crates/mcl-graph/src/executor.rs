use std::sync::Arc;

use indexmap::IndexMap;

use crate::channel::ChannelRegistry;
use crate::context::NodeContext;
use crate::error::GraphError;
use crate::factory::NodeFactory;
use crate::graph_def::GraphDef;
use crate::id::NodeId;
use crate::interceptor::{InterceptorVerdict, PacketInterceptor};
use crate::lifecycle::LifecycleHook;
use crate::message::{Packet, PacketPolicyMeta, PacketStatus};
use crate::node::Node;

/// Maximum consecutive failures before a node is disabled
pub const MAX_CONSECUTIVE_FAILURES: u32 = 3;

/// Truncate a payload description for logging (first 80 chars + total byte count)
fn payload_summary(payload: &mcl_spec::value::MclValue) -> String {
    let text = match payload {
        mcl_spec::value::MclValue::String(s) => s.clone(),
        other => format!("{other:?}"),
    };
    let byte_len = text.len();
    if byte_len <= 80 {
        format!("{text} ({byte_len}b)")
    } else {
        format!("{}… ({byte_len}b)", &text[..80])
    }
}

/// Result of a single super-step
#[derive(Debug)]
pub struct StepResult {
    /// Nodes that processed packets this step
    pub fired: Vec<NodeId>,
    /// Whether any channels have new pending data (another step needed)
    pub has_pending: bool,
}

/// Pregel-inspired graph executor.
///
/// Execution model:
/// 1. Source nodes are polled for new input
/// 2. For each channel with pending packets, all subscriber nodes are fired
/// 3. Output packets are routed to destination channels
/// 4. Repeat until quiescent (no pending data)
pub struct GraphExecutor {
    pub(crate) nodes: IndexMap<NodeId, Box<dyn Node>>,
    pub(crate) contexts: IndexMap<NodeId, NodeContext>,
    pub(crate) channels: ChannelRegistry,
    pub(crate) source_nodes: Vec<NodeId>,
    pub(crate) step_count: u64,
    pub(crate) consecutive_failures: IndexMap<NodeId, u32>,
    pub(crate) node_priorities: IndexMap<NodeId, u32>,
    pub(crate) wildcard_subscribers: Vec<NodeId>,
    hooks: Vec<Arc<dyn LifecycleHook>>,
    interceptor: Option<Arc<dyn PacketInterceptor>>,
    replay_mode: bool,
}

impl GraphExecutor {
    /// Build and initialize a graph from a definition and factory,
    /// with an optional pre-built node injected for a specific type name.
    pub async fn build_with_override(
        def: &GraphDef,
        factory: &NodeFactory,
        override_type: &str,
        override_node: Box<dyn Node>,
    ) -> Result<Self, GraphError> {
        let mut nodes: IndexMap<NodeId, Box<dyn Node>> = IndexMap::new();
        let mut contexts: IndexMap<NodeId, NodeContext> = IndexMap::new();
        let mut channels = ChannelRegistry::new();
        let mut source_nodes = Vec::new();
        let mut override_node = Some(override_node);

        for (name, node_def) in &def.nodes {
            let node: Box<dyn Node> = if node_def.node_type == override_type {
                override_node.take().ok_or_else(|| {
                    GraphError::NodeError(format!(
                        "multiple nodes of type '{}' but only one override provided",
                        override_type
                    ))
                })?
            } else {
                factory.create(node_def)?
            };
            let node_id = NodeId::new(name);

            if node.is_source() {
                source_nodes.push(node_id.clone());
            }

            let ctx = NodeContext::new(node_id.clone(), node_def.config.clone());
            contexts.insert(node_id.clone(), ctx);
            nodes.insert(node_id, node);
        }

        // Wire up channels from edge definitions
        for edge in &def.edges {
            let from_id = NodeId::new(&edge.from);
            let to_id = NodeId::new(&edge.to);

            let ch = channels.get_or_create(&edge.channel);
            if !ch.publishers.contains(&from_id) {
                ch.publishers.push(from_id);
            }
            if !ch.subscribers.contains(&to_id) {
                ch.subscribers.push(to_id);
            }
        }

        // Initialize all nodes
        let node_priorities: IndexMap<NodeId, u32> = def
            .nodes
            .iter()
            .map(|(name, ndef)| (NodeId::new(name), ndef.priority))
            .collect();

        let mut executor = Self {
            nodes,
            contexts,
            channels,
            source_nodes,
            step_count: 0,
            consecutive_failures: IndexMap::new(),
            node_priorities,
            wildcard_subscribers: Vec::new(),
            hooks: Vec::new(),
            interceptor: None,
            replay_mode: false,
        };

        executor.detect_wildcard_subscribers();

        let node_ids: Vec<NodeId> = executor.nodes.keys().cloned().collect();
        for node_id in node_ids {
            let node = executor
                .nodes
                .get_mut(&node_id)
                .ok_or_else(|| GraphError::NodeNotFound(node_id.to_string()))?;
            let ctx = executor
                .contexts
                .get_mut(&node_id)
                .ok_or_else(|| GraphError::NodeNotFound(node_id.to_string()))?;
            node.init(ctx).await?;
        }

        log::debug!(
            "built graph '{}': {} nodes, {} sources, {} edges",
            def.name,
            executor.nodes.len(),
            executor.source_nodes.len(),
            def.edges.len(),
        );

        Ok(executor)
    }

    pub async fn build(def: &GraphDef, factory: &NodeFactory) -> Result<Self, GraphError> {
        let mut nodes: IndexMap<NodeId, Box<dyn Node>> = IndexMap::new();
        let mut contexts: IndexMap<NodeId, NodeContext> = IndexMap::new();
        let mut channels = ChannelRegistry::new();
        let mut source_nodes = Vec::new();

        for (name, node_def) in &def.nodes {
            let node = factory.create(node_def)?;
            let node_id = NodeId::new(name);

            if node.is_source() {
                source_nodes.push(node_id.clone());
            }

            let ctx = NodeContext::new(node_id.clone(), node_def.config.clone());
            contexts.insert(node_id.clone(), ctx);
            nodes.insert(node_id, node);
        }

        for edge in &def.edges {
            let from_id = NodeId::new(&edge.from);
            let to_id = NodeId::new(&edge.to);

            let ch = channels.get_or_create(&edge.channel);
            if !ch.publishers.contains(&from_id) {
                ch.publishers.push(from_id);
            }
            if !ch.subscribers.contains(&to_id) {
                ch.subscribers.push(to_id);
            }
        }

        let node_priorities: IndexMap<NodeId, u32> = def
            .nodes
            .iter()
            .map(|(name, ndef)| (NodeId::new(name), ndef.priority))
            .collect();

        let mut executor = Self {
            nodes,
            contexts,
            channels,
            source_nodes,
            step_count: 0,
            consecutive_failures: IndexMap::new(),
            node_priorities,
            wildcard_subscribers: Vec::new(),
            hooks: Vec::new(),
            interceptor: None,
            replay_mode: false,
        };

        executor.detect_wildcard_subscribers();

        let node_ids: Vec<NodeId> = executor.nodes.keys().cloned().collect();
        for node_id in node_ids {
            let node = executor
                .nodes
                .get_mut(&node_id)
                .ok_or_else(|| GraphError::NodeNotFound(node_id.to_string()))?;
            let ctx = executor
                .contexts
                .get_mut(&node_id)
                .ok_or_else(|| GraphError::NodeNotFound(node_id.to_string()))?;
            node.init(ctx).await?;
        }

        log::debug!(
            "built graph '{}': {} nodes, {} sources, {} edges",
            def.name,
            executor.nodes.len(),
            executor.source_nodes.len(),
            def.edges.len(),
        );

        Ok(executor)
    }

    fn is_disabled(&self, node_id: &NodeId) -> bool {
        self.consecutive_failures.get(node_id).copied().unwrap_or(0) >= MAX_CONSECUTIVE_FAILURES
    }

    fn detect_wildcard_subscribers(&mut self) {
        self.wildcard_subscribers = self
            .nodes
            .iter()
            .filter(|(_, node)| {
                node.capability()
                    .subscriptions
                    .iter()
                    .any(|spec| spec.is_wildcard())
            })
            .map(|(id, _)| id.clone())
            .collect();
    }

    /// Register a lifecycle hook. Hooks persist across super-steps.
    pub fn add_hook(&mut self, hook: Arc<dyn LifecycleHook>) {
        self.hooks.push(hook);
    }

    /// Set the packet interceptor (e.g., signal registry).
    ///
    /// When set, outbound packets with non-empty `signals` are intercepted
    /// after drain_outbox and before being pushed to channels.
    pub fn set_interceptor(&mut self, interceptor: Arc<dyn PacketInterceptor>) {
        self.interceptor = Some(interceptor);
    }

    /// Enable or disable replay mode for effect-aware scheduling.
    ///
    /// In replay mode:
    /// - Packets with external effects use audit data instead of real dispatch
    /// - Packets with `replay_unsafe` effect and no audit record are rejected
    pub fn set_replay_mode(&mut self, mode: bool) {
        self.replay_mode = mode;
    }

    /// Execute one super-step: process all pending channel data
    pub async fn step(&mut self) -> Result<StepResult, GraphError> {
        self.step_count += 1;
        let step_num = self.step_count;
        let mut fired = Vec::new();
        let hooks = self.hooks.clone();

        let pending: Vec<(String, Vec<NodeId>)> = self
            .channels
            .pending_channels()
            .into_iter()
            .filter_map(|ch_name| {
                let ch = self.channels.get(ch_name)?;
                Some((ch_name.to_string(), ch.subscribers.clone()))
            })
            .collect();

        log::debug!(
            "step {step_num}: {} channels pending, {} source nodes",
            pending.len(),
            self.source_nodes.len(),
        );

        for (ch_name, subscribers) in &pending {
            log::trace!(
                "step {step_num}: channel '{ch_name}' → {} subscribers",
                subscribers.len()
            );
        }

        for (ch_name, subscribers) in pending {
            let packets = self
                .channels
                .get_mut(&ch_name)
                .map(|ch| ch.drain())
                .unwrap_or_default();

            if packets.is_empty() {
                continue;
            }

            for pkt in &packets {
                log::trace!(
                    "step {step_num}: routing from '{}' via '{}' → {:?} [{}]",
                    pkt.origin,
                    ch_name,
                    subscribers,
                    payload_summary(&pkt.payload),
                );
            }

            let mut sorted_subscribers = subscribers.clone();
            sorted_subscribers.sort_by(|a, b| {
                let pa = self.node_priorities.get(a).copied().unwrap_or(0);
                let pb = self.node_priorities.get(b).copied().unwrap_or(0);
                pb.cmp(&pa)
            });

            for node_id in &sorted_subscribers {
                if self.is_disabled(node_id) {
                    continue;
                }

                if let Some(node) = self.nodes.get_mut(node_id) {
                    let ctx = match self.contexts.get_mut(node_id) {
                        Some(c) => c,
                        None => continue,
                    };

                    match node.process(&ch_name, &packets, ctx).await {
                        Ok(()) => {
                            self.consecutive_failures.shift_remove(node_id);
                            if !fired.contains(node_id) {
                                fired.push(node_id.clone());
                            }
                            for hook in &hooks {
                                for pkt in &packets {
                                    hook.on_packet_in(node_id.as_str(), &ch_name, pkt, ctx);
                                }
                            }
                        }
                        Err(e) => {
                            log::error!("node {} error on channel {}: {e}", node_id, ch_name);
                            ctx.drain_outbox();
                            let count = self
                                .consecutive_failures
                                .entry(node_id.clone())
                                .and_modify(|f| *f += 1)
                                .or_insert(1);
                            if *count >= MAX_CONSECUTIVE_FAILURES {
                                log::warn!(
                                    "node '{}' disabled after {} consecutive failures",
                                    node_id,
                                    MAX_CONSECUTIVE_FAILURES,
                                );
                            }
                        }
                    }
                }
            }

            // Deliver packets to wildcard subscribers (blanket channel fan-out)
            let wildcard_ids = self.wildcard_subscribers.clone();
            for node_id in &wildcard_ids {
                if self.is_disabled(node_id) || sorted_subscribers.contains(node_id) {
                    continue;
                }

                if let Some(node) = self.nodes.get_mut(node_id) {
                    let ctx = match self.contexts.get_mut(node_id) {
                        Some(c) => c,
                        None => continue,
                    };

                    match node.process(&ch_name, &packets, ctx).await {
                        Ok(()) => {
                            self.consecutive_failures.shift_remove(node_id);
                            if !fired.contains(node_id) {
                                fired.push(node_id.clone());
                            }
                            for hook in &hooks {
                                for pkt in &packets {
                                    hook.on_packet_in(node_id.as_str(), &ch_name, pkt, ctx);
                                }
                            }
                        }
                        Err(e) => {
                            log::error!(
                                "wildcard node {} error on channel {}: {e}",
                                node_id,
                                ch_name
                            );
                            ctx.drain_outbox();
                            let count = self
                                .consecutive_failures
                                .entry(node_id.clone())
                                .and_modify(|f| *f += 1)
                                .or_insert(1);
                            if *count >= MAX_CONSECUTIVE_FAILURES {
                                log::warn!(
                                    "wildcard node '{}' disabled after {} consecutive failures",
                                    node_id,
                                    MAX_CONSECUTIVE_FAILURES,
                                );
                            }
                        }
                    }
                }
            }
        }

        let node_ids: Vec<NodeId> = self.contexts.keys().cloned().collect();
        let interceptor = self.interceptor.clone();
        for node_id in &node_ids {
            let outbound = self
                .contexts
                .get_mut(node_id)
                .map(|ctx| ctx.drain_outbox())
                .unwrap_or_default();

            for (channel_name, mut packet) in outbound {
                let intercepted = interceptor.is_some() && !packet.signals.is_empty();

                if intercepted {
                    let verdict = interceptor
                        .as_ref()
                        .expect("interceptor present")
                        .intercept(&packet, node_id, step_num, &channel_name);

                    match verdict {
                        InterceptorVerdict::Accept { registry_ids } => {
                            packet.meta.insert(
                                "registry_ids".to_string(),
                                mcl_spec::value::MclValue::List(
                                    registry_ids
                                        .into_iter()
                                        .map(mcl_spec::value::MclValue::String)
                                        .collect(),
                                ),
                            );
                        }
                        InterceptorVerdict::Reject { reason } => {
                            log::warn!(
                                "step {step_num}: packet from '{}' on '{}' rejected: {reason}",
                                node_id,
                                channel_name,
                            );
                            let mut error_pkt = Packet::new(
                                &channel_name,
                                node_id.clone(),
                                packet.id,
                                mcl_spec::value::MclValue::String(format!(
                                    "signal rejected: {reason}"
                                )),
                            );
                            error_pkt.status = PacketStatus::Failed;
                            error_pkt.origin = node_id.clone();
                            self.channels.push(&channel_name, error_pkt);
                            continue;
                        }
                        InterceptorVerdict::Defer => {
                            packet.meta.insert(
                                "quarantined".to_string(),
                                mcl_spec::value::MclValue::Bool(true),
                            );
                        }
                        InterceptorVerdict::RouteTo {
                            channel: target_channel,
                            policy_id,
                            effect_flags,
                        } => {
                            packet.channel = target_channel.clone();
                            packet.policy_meta = Some(PacketPolicyMeta {
                                policy_id,
                                effect_flags: effect_flags.clone(),
                            });

                            if self.replay_mode {
                                if effect_flags.contains(&"replay_unsafe".to_string()) {
                                    let has_audit = packet.meta.get("audit_record").is_some();
                                    if !has_audit {
                                        log::warn!(
                                            "step {step_num}: replay_unsafe packet from '{}' on '{}' rejected: no audit record",
                                            node_id,
                                            channel_name,
                                        );
                                        continue;
                                    }
                                }

                                if effect_flags.contains(&"external(network)".to_string()) {
                                    log::info!(
                                        "step {step_num}: external(network) packet from '{}' on '{}' — replay mode: using audit data",
                                        node_id,
                                        channel_name,
                                    );
                                    packet.meta.insert(
                                        "replay_injected".to_string(),
                                        mcl_spec::value::MclValue::Bool(true),
                                    );
                                }
                            }

                            self.channels.get_or_create(&target_channel);
                            self.channels.push(&target_channel, packet);
                            continue;
                        }
                    }
                }

                if let Some(ctx) = self.contexts.get_mut(node_id) {
                    for hook in &hooks {
                        hook.on_packet_out(node_id.as_str(), &channel_name, &packet, ctx);
                    }
                }

                if self.replay_mode {
                    let effect_flags = packet
                        .policy_meta
                        .as_ref()
                        .map(|pm| pm.effect_flags.as_slice())
                        .unwrap_or_default();

                    if effect_flags.contains(&"replay_unsafe".to_string()) {
                        let has_audit = packet.meta.get("audit_record").is_some();
                        if !has_audit {
                            log::warn!(
                                "step {step_num}: replay_unsafe packet from '{}' on '{}' rejected: no audit record",
                                node_id,
                                channel_name,
                            );
                            continue;
                        }
                    }

                    if effect_flags.contains(&"external(network)".to_string()) {
                        log::info!(
                            "step {step_num}: external(network) packet from '{}' on '{}' — replay mode: using audit data",
                            node_id,
                            channel_name,
                        );
                        packet.meta.insert(
                            "replay_injected".to_string(),
                            mcl_spec::value::MclValue::Bool(true),
                        );
                    }
                }

                log::trace!(
                    "step {step_num}: node '{}' emitting on '{}': {}",
                    node_id,
                    channel_name,
                    payload_summary(&packet.payload),
                );
                self.channels.push(&channel_name, packet);
            }
        }

        if let Some(interceptor) = &self.interceptor {
            interceptor.on_super_step_end();
        }

        let has_pending = self.channels.has_pending();

        log::debug!(
            "step {step_num}: {} nodes fired, has_pending={has_pending}",
            fired.len(),
        );

        Ok(StepResult { fired, has_pending })
    }

    /// Run super-steps until no channels have pending data
    pub async fn run_until_quiescent(&mut self) -> Result<(), GraphError> {
        let max_steps = 100; // safety limit
        for _ in 0..max_steps {
            let result = self.step().await?;
            if !result.has_pending {
                return Ok(());
            }
        }
        Err(GraphError::Execution(
            "graph did not reach quiescence after 100 steps".to_string(),
        ))
    }

    /// Poll source nodes for new input
    pub async fn poll_sources(&mut self) -> Result<bool, GraphError> {
        let mut got_input = false;

        let source_ids = self.source_nodes.clone();
        for node_id in &source_ids {
            if let Some(node) = self.nodes.get_mut(node_id) {
                let ctx = match self.contexts.get_mut(node_id) {
                    Some(c) => c,
                    None => continue,
                };
                if let Some(packet) = node.poll_source(ctx).await? {
                    log::trace!(
                        "source '{}' produced packet on '{}': {}",
                        node_id,
                        packet.channel,
                        payload_summary(&packet.payload),
                    );
                    let channel = packet.channel.clone();
                    self.channels.push(&channel, packet);
                    got_input = true;
                }
            }
        }

        Ok(got_input)
    }

    /// Main interactive loop: poll sources → run until quiescent → repeat
    pub async fn run_interactive(&mut self) -> Result<(), GraphError> {
        loop {
            // Poll for source input (blocks on stdin for client node)
            let got_input = self.poll_sources().await?;

            if got_input {
                // Process all downstream effects
                self.run_until_quiescent().await?;
            }
        }
    }

    /// Shut down all nodes
    pub async fn shutdown(&mut self) -> Result<(), GraphError> {
        if let Some(interceptor) = &self.interceptor {
            interceptor.on_shutdown();
        }

        let node_ids: Vec<NodeId> = self.nodes.keys().cloned().collect();
        for node_id in node_ids {
            if let Some(node) = self.nodes.get_mut(&node_id) {
                let ctx = match self.contexts.get_mut(&node_id) {
                    Some(c) => c,
                    None => continue,
                };
                node.shutdown(ctx).await?;
            }
        }
        Ok(())
    }

    /// Inject a packet into a channel (for testing or external input)
    pub fn inject(&mut self, channel: &str, packet: Packet) {
        self.channels.push(channel, packet);
    }

    /// Drain all pending packets from a channel (for testing).
    ///
    /// Returns the drained packets. Returns an empty vec if the channel
    /// does not exist.
    pub fn drain_channel(&mut self, name: &str) -> Vec<Packet> {
        self.channels
            .get_mut(name)
            .map(|ch| ch.drain())
            .unwrap_or_default()
    }

    /// Ensure a channel exists (no subscriber added).
    ///
    /// Useful for output-only channels where packets are collected via
    /// `drain_channel` rather than delivered to subscribers.
    pub fn create_channel(&mut self, name: &str) {
        self.channels.get_or_create(name);
    }

    /// Ensure a channel exists and add a subscriber node to it.
    ///
    /// If the channel does not exist, it will be created.
    /// If the subscriber is already registered, this is a no-op.
    pub fn wire_subscriber(&mut self, channel: &str, subscriber: NodeId) {
        let ch = self.channels.get_or_create(channel);
        if !ch.subscribers.contains(&subscriber) {
            ch.subscribers.push(subscriber);
        }
    }

    /// Get the list of registered node IDs
    pub fn node_ids(&self) -> Vec<&NodeId> {
        self.nodes.keys().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph_def::{EdgeDef, NodeDef};
    use crate::node::{ChannelSpec, GovernanceKind, GoverningNode, Node, NodeCapability};
    use mcl_spec::signal::{MclSignal, SignalKind};
    use mcl_spec::span::Span;
    use mcl_spec::value::MclValue;

    /// Simple echo node: receives on "in", emits on "out"
    struct EchoNode;

    #[async_trait::async_trait]
    impl Node for EchoNode {
        fn capability(&self) -> NodeCapability {
            NodeCapability {
                type_name: "echo".into(),
                description: "echoes input to output".into(),
                subscriptions: vec![ChannelSpec::new("in")],
                publications: vec![ChannelSpec::new("out")],
            }
        }

        async fn init(&mut self, _ctx: &mut NodeContext) -> Result<(), GraphError> {
            Ok(())
        }

        async fn process(
            &mut self,
            _channel: &str,
            packets: &[Packet],
            ctx: &mut NodeContext,
        ) -> Result<(), GraphError> {
            for pkt in packets {
                ctx.emit("out", pkt.payload.clone());
            }
            Ok(())
        }
    }

    struct SignalEchoNode;

    #[async_trait::async_trait]
    impl Node for SignalEchoNode {
        fn capability(&self) -> NodeCapability {
            NodeCapability {
                type_name: "signal_echo".into(),
                description: "echoes input with signals preserved".into(),
                subscriptions: vec![ChannelSpec::new("in")],
                publications: vec![ChannelSpec::new("out")],
            }
        }

        async fn init(&mut self, _ctx: &mut NodeContext) -> Result<(), GraphError> {
            Ok(())
        }

        async fn process(
            &mut self,
            _channel: &str,
            packets: &[Packet],
            ctx: &mut NodeContext,
        ) -> Result<(), GraphError> {
            for pkt in packets {
                let mut out = pkt.clone();
                out.channel = "out".to_string();
                ctx.emit_packet("out", out);
            }
            Ok(())
        }
    }

    /// Collector node: stores received packets
    struct CollectorNode {
        collected: Vec<MclValue>,
    }

    #[async_trait::async_trait]
    impl Node for CollectorNode {
        fn capability(&self) -> NodeCapability {
            NodeCapability {
                type_name: "collector".into(),
                description: "collects packets".into(),
                subscriptions: vec![ChannelSpec::new("in")],
                publications: vec![],
            }
        }

        async fn init(&mut self, _ctx: &mut NodeContext) -> Result<(), GraphError> {
            Ok(())
        }

        async fn process(
            &mut self,
            _channel: &str,
            packets: &[Packet],
            _ctx: &mut NodeContext,
        ) -> Result<(), GraphError> {
            for pkt in packets {
                self.collected.push(pkt.payload.clone());
            }
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_basic_routing() {
        let mut factory = NodeFactory::new();
        factory.register_fn("echo", |_| Ok(Box::new(EchoNode)));
        factory.register_fn("collector", |_| {
            Ok(Box::new(CollectorNode { collected: vec![] }))
        });

        let def = GraphDef {
            name: "test".into(),
            config: IndexMap::new(),
            nodes: IndexMap::from([
                (
                    "echo".to_string(),
                    NodeDef {
                        node_type: "echo".into(),
                        config: IndexMap::new(),
                        priority: 0,
                    },
                ),
                (
                    "sink".to_string(),
                    NodeDef {
                        node_type: "collector".into(),
                        config: IndexMap::new(),
                        priority: 0,
                    },
                ),
            ]),
            edges: vec![EdgeDef {
                from: "echo".into(),
                to: "sink".into(),
                channel: "out".into(),
            }],
        };

        let mut exec = GraphExecutor::build(&def, &factory).await.unwrap();

        // Inject a packet into the echo node's input channel
        // First, we need to register "in" channel with echo as subscriber
        {
            let ch = exec.channels.get_or_create("in");
            ch.subscribers.push(NodeId::new("echo"));
        }

        let pkt = Packet::text("in", NodeId::new("test"), 1, "hello");
        exec.inject("in", pkt);

        // Run until quiescent
        exec.run_until_quiescent().await.unwrap();

        // The collector should have received the echoed packet via "out" channel
        // Note: collector subscribes to "in" by default from its capability,
        // but the graph def wires echo.out -> sink via "out" channel
        // We need to check if the routing worked
    }

    /// Node that fails during init
    struct FailOnInitNode;

    #[async_trait::async_trait]
    impl Node for FailOnInitNode {
        fn capability(&self) -> NodeCapability {
            NodeCapability {
                type_name: "fail_init".into(),
                description: "fails during init".into(),
                subscriptions: vec![],
                publications: vec![],
            }
        }

        async fn init(&mut self, _ctx: &mut NodeContext) -> Result<(), GraphError> {
            Err(GraphError::NodeError("init failure".to_string()))
        }

        async fn process(
            &mut self,
            _channel: &str,
            _packets: &[Packet],
            _ctx: &mut NodeContext,
        ) -> Result<(), GraphError> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_build_propagates_init_error() {
        let mut factory = NodeFactory::new();
        factory.register_fn("fail_init", |_| Ok(Box::new(FailOnInitNode)));

        let def = GraphDef {
            name: "test".into(),
            config: IndexMap::new(),
            nodes: IndexMap::from([(
                "bad".to_string(),
                NodeDef {
                    node_type: "fail_init".into(),
                    config: IndexMap::new(),
                    priority: 0,
                },
            )]),
            edges: vec![],
        };

        let result = GraphExecutor::build(&def, &factory).await;
        assert!(
            result.is_err(),
            "build should return Err when a node fails init"
        );
    }

    #[tokio::test]
    async fn test_build_propagates_factory_error() {
        let mut factory = NodeFactory::new();
        factory.register_fn("unknown", |_| {
            Err(GraphError::UnknownNodeType("unknown".to_string()))
        });

        let def = GraphDef {
            name: "test".into(),
            config: IndexMap::new(),
            nodes: IndexMap::from([(
                "bad".to_string(),
                NodeDef {
                    node_type: "unknown".into(),
                    config: IndexMap::new(),
                    priority: 0,
                },
            )]),
            edges: vec![],
        };

        let result = GraphExecutor::build(&def, &factory).await;
        assert!(
            result.is_err(),
            "build should return Err when factory fails to create node"
        );
    }

    struct FailProcessNode;

    #[async_trait::async_trait]
    impl Node for FailProcessNode {
        fn capability(&self) -> NodeCapability {
            NodeCapability {
                type_name: "fail_process".into(),
                description: "always fails during process".into(),
                subscriptions: vec![ChannelSpec::new("in")],
                publications: vec![ChannelSpec::new("out")],
            }
        }

        async fn init(&mut self, _ctx: &mut NodeContext) -> Result<(), GraphError> {
            Ok(())
        }

        async fn process(
            &mut self,
            _channel: &str,
            _packets: &[Packet],
            _ctx: &mut NodeContext,
        ) -> Result<(), GraphError> {
            Err(GraphError::NodeError("process failure".to_string()))
        }
    }

    #[tokio::test]
    async fn graph_continues_after_node_failure() {
        let mut factory = NodeFactory::new();
        factory.register_fn("echo", |_| Ok(Box::new(EchoNode)));
        factory.register_fn("fail_process", |_| Ok(Box::new(FailProcessNode)));
        factory.register_fn("collector", |_| {
            Ok(Box::new(CollectorNode { collected: vec![] }))
        });

        let def = GraphDef {
            name: "test".into(),
            config: IndexMap::new(),
            nodes: IndexMap::from([
                (
                    "echo".to_string(),
                    NodeDef {
                        node_type: "echo".into(),
                        config: IndexMap::new(),
                        priority: 0,
                    },
                ),
                (
                    "bad".to_string(),
                    NodeDef {
                        node_type: "fail_process".into(),
                        config: IndexMap::new(),
                        priority: 0,
                    },
                ),
                (
                    "sink".to_string(),
                    NodeDef {
                        node_type: "collector".into(),
                        config: IndexMap::new(),
                        priority: 0,
                    },
                ),
            ]),
            edges: vec![
                EdgeDef {
                    from: "echo".into(),
                    to: "sink".into(),
                    channel: "out".into(),
                },
                EdgeDef {
                    from: "bad".into(),
                    to: "sink".into(),
                    channel: "bad_out".into(),
                },
            ],
        };

        let mut exec = GraphExecutor::build(&def, &factory).await.unwrap();

        {
            let ch = exec.channels.get_or_create("in");
            ch.subscribers.push(NodeId::new("echo"));
        }
        {
            let ch = exec.channels.get_or_create("bad_in");
            ch.subscribers.push(NodeId::new("bad"));
        }

        exec.inject("in", Packet::text("in", NodeId::new("test"), 1, "hello"));
        exec.inject(
            "bad_in",
            Packet::text("bad_in", NodeId::new("test"), 1, "fail"),
        );

        let result = exec.step().await;
        assert!(result.is_ok(), "step should succeed even when a node fails");

        let step_result = result.unwrap();
        assert!(
            step_result.fired.iter().any(|id| id.as_str() == "echo"),
            "echo node should have fired"
        );
        assert!(
            !step_result.fired.iter().any(|id| id.as_str() == "bad"),
            "failed node should not appear in fired"
        );
    }

    #[tokio::test]
    async fn node_disabled_after_max_consecutive_failures() {
        let mut factory = NodeFactory::new();
        factory.register_fn("fail_process", |_| Ok(Box::new(FailProcessNode)));

        let def = GraphDef {
            name: "test".into(),
            config: IndexMap::new(),
            nodes: IndexMap::from([(
                "bad".to_string(),
                NodeDef {
                    node_type: "fail_process".into(),
                    config: IndexMap::new(),
                    priority: 0,
                },
            )]),
            edges: vec![],
        };

        let mut exec = GraphExecutor::build(&def, &factory).await.unwrap();

        {
            let ch = exec.channels.get_or_create("in");
            ch.subscribers.push(NodeId::new("bad"));
        }

        for _ in 0..(MAX_CONSECUTIVE_FAILURES - 1) {
            exec.inject("in", Packet::text("in", NodeId::new("test"), 1, "fail"));
            let result = exec.step().await.unwrap();
            assert!(!result.fired.iter().any(|id| id.as_str() == "bad"));
        }

        exec.inject("in", Packet::text("in", NodeId::new("test"), 1, "fail"));
        let result = exec.step().await.unwrap();
        assert!(
            !result.fired.iter().any(|id| id.as_str() == "bad"),
            "disabled node should not appear in fired after reaching max consecutive failures"
        );

        exec.inject("in", Packet::text("in", NodeId::new("test"), 1, "fail"));
        let result = exec.step().await.unwrap();
        assert!(
            !result.fired.iter().any(|id| id.as_str() == "bad"),
            "disabled node should remain disabled on subsequent steps"
        );
    }

    #[tokio::test]
    async fn test_shutdown_with_empty_graph() {
        let mut factory = NodeFactory::new();
        factory.register_fn("echo", |_| Ok(Box::new(EchoNode)));

        let def = GraphDef {
            name: "empty".into(),
            config: IndexMap::new(),
            nodes: IndexMap::new(),
            edges: vec![],
        };

        let mut exec = GraphExecutor::build(&def, &factory).await.unwrap();
        assert!(exec.shutdown().await.is_ok());
    }

    struct OrderRecorderNode;

    #[async_trait::async_trait]
    impl Node for OrderRecorderNode {
        fn capability(&self) -> NodeCapability {
            NodeCapability {
                type_name: "order_recorder".into(),
                description: "records execution order".into(),
                subscriptions: vec![ChannelSpec::new("in")],
                publications: vec![],
            }
        }

        async fn init(&mut self, _ctx: &mut NodeContext) -> Result<(), GraphError> {
            Ok(())
        }

        async fn process(
            &mut self,
            _channel: &str,
            _packets: &[Packet],
            _ctx: &mut NodeContext,
        ) -> Result<(), GraphError> {
            Ok(())
        }
    }

    async fn build_priority_graph(nodes: Vec<(&str, u32)>) -> (GraphExecutor, NodeFactory) {
        let mut factory = NodeFactory::new();
        factory.register_fn("order_recorder", |_| Ok(Box::new(OrderRecorderNode)));

        let node_defs: IndexMap<String, NodeDef> = nodes
            .into_iter()
            .map(|(name, priority)| {
                (
                    name.to_string(),
                    NodeDef {
                        node_type: "order_recorder".into(),
                        config: IndexMap::new(),
                        priority,
                    },
                )
            })
            .collect();

        let def = GraphDef {
            name: "priority_test".into(),
            config: IndexMap::new(),
            nodes: node_defs,
            edges: vec![],
        };

        let exec = GraphExecutor::build(&def, &factory).await.unwrap();
        (exec, factory)
    }

    #[tokio::test]
    async fn turn_taking_higher_priority_executes_first() {
        let (mut exec, _) = build_priority_graph(vec![("narrator", 1), ("advisor", 2)]).await;

        {
            let ch = exec.channels.get_or_create("in");
            ch.subscribers.push(NodeId::new("narrator"));
            ch.subscribers.push(NodeId::new("advisor"));
        }

        exec.inject("in", Packet::text("in", NodeId::new("test"), 1, "go"));
        let result = exec.step().await.unwrap();
        assert_eq!(result.fired.len(), 2);
        assert_eq!(
            result.fired[0].as_str(),
            "advisor",
            "higher priority node should fire first"
        );
        assert_eq!(result.fired[1].as_str(), "narrator");
    }

    #[tokio::test]
    async fn turn_taking_default_priority_maintains_current_behavior() {
        let (mut exec, _) = build_priority_graph(vec![("node_a", 0), ("node_b", 0)]).await;

        {
            let ch = exec.channels.get_or_create("in");
            ch.subscribers.push(NodeId::new("node_a"));
            ch.subscribers.push(NodeId::new("node_b"));
        }

        exec.inject("in", Packet::text("in", NodeId::new("test"), 1, "go"));
        let result = exec.step().await.unwrap();
        assert_eq!(result.fired.len(), 2);
        assert!(result.fired.iter().any(|id| id.as_str() == "node_a"));
        assert!(result.fired.iter().any(|id| id.as_str() == "node_b"));
    }

    #[tokio::test]
    async fn turn_taking_same_priority_process_simultaneously() {
        let (mut exec, _) = build_priority_graph(vec![("writer", 3), ("editor", 3)]).await;

        {
            let ch = exec.channels.get_or_create("in");
            ch.subscribers.push(NodeId::new("writer"));
            ch.subscribers.push(NodeId::new("editor"));
        }

        exec.inject("in", Packet::text("in", NodeId::new("test"), 1, "go"));
        let result = exec.step().await.unwrap();
        assert_eq!(result.fired.len(), 2);
        assert!(result.fired.iter().any(|id| id.as_str() == "writer"));
        assert!(result.fired.iter().any(|id| id.as_str() == "editor"));
    }

    #[tokio::test]
    async fn turn_taking_mixed_priorities_order_correct() {
        let (mut exec, _) =
            build_priority_graph(vec![("low", 0), ("medium", 1), ("high", 2)]).await;

        {
            let ch = exec.channels.get_or_create("in");
            ch.subscribers.push(NodeId::new("low"));
            ch.subscribers.push(NodeId::new("medium"));
            ch.subscribers.push(NodeId::new("high"));
        }

        exec.inject("in", Packet::text("in", NodeId::new("test"), 1, "go"));
        let result = exec.step().await.unwrap();
        assert_eq!(result.fired.len(), 3);
        assert_eq!(result.fired[0].as_str(), "high");
        assert_eq!(result.fired[1].as_str(), "medium");
        assert_eq!(result.fired[2].as_str(), "low");
    }

    #[tokio::test]
    async fn memory_population_hook_auto_stores_packets() {
        use crate::lifecycle::MemoryPopulationHook;

        let mut factory = NodeFactory::new();
        factory.register_fn("echo", |_| Ok(Box::new(EchoNode)));

        let def = GraphDef {
            name: "memory_hook_test".into(),
            config: IndexMap::new(),
            nodes: IndexMap::from([(
                "echo".to_string(),
                NodeDef {
                    node_type: "echo".into(),
                    config: IndexMap::new(),
                    priority: 0,
                },
            )]),
            edges: vec![],
        };

        let mut exec = GraphExecutor::build(&def, &factory).await.unwrap();
        exec.add_hook(std::sync::Arc::new(MemoryPopulationHook::new()));

        let embedder = mcl_memory::create_mock_embedder(384);
        let memory = mcl_memory::NodeMemory::new(100, embedder, 384).unwrap();
        exec.contexts
            .get_mut(&NodeId::new("echo"))
            .unwrap()
            .set_memory(Some(memory));

        {
            let ch = exec.channels.get_or_create("in");
            ch.subscribers.push(NodeId::new("echo"));
        }

        exec.inject(
            "in",
            Packet::text("in", NodeId::new("src"), 1, "hello world"),
        );

        let result = exec.step().await.unwrap();
        assert!(result.fired.iter().any(|id| id.as_str() == "echo"));

        {
            let ctx = exec.contexts.get(&NodeId::new("echo")).unwrap();
            let mem = ctx.memory().unwrap();
            assert_eq!(mem.incoming.len(), 1);
            assert_eq!(mem.incoming.recent(1)[0].text, "hello world");
        }

        exec.run_until_quiescent().await.unwrap();

        let ctx = exec.contexts.get(&NodeId::new("echo")).unwrap();
        let mem = ctx.memory().unwrap();
        assert_eq!(mem.outgoing.len(), 1);
        assert_eq!(mem.outgoing.recent(1)[0].text, "hello world");
    }

    #[tokio::test]
    async fn executor_logs_step_info() {
        let mut factory = NodeFactory::new();
        factory.register_fn("echo", |_| Ok(Box::new(EchoNode)));
        factory.register_fn("collector", |_| {
            Ok(Box::new(CollectorNode { collected: vec![] }))
        });

        let def = GraphDef {
            name: "log_test".into(),
            config: IndexMap::new(),
            nodes: IndexMap::from([
                (
                    "echo".to_string(),
                    NodeDef {
                        node_type: "echo".into(),
                        config: IndexMap::new(),
                        priority: 0,
                    },
                ),
                (
                    "sink".to_string(),
                    NodeDef {
                        node_type: "collector".into(),
                        config: IndexMap::new(),
                        priority: 0,
                    },
                ),
            ]),
            edges: vec![EdgeDef {
                from: "echo".into(),
                to: "sink".into(),
                channel: "out".into(),
            }],
        };

        let mut exec = GraphExecutor::build(&def, &factory).await.unwrap();

        {
            let ch = exec.channels.get_or_create("in");
            ch.subscribers.push(NodeId::new("echo"));
        }

        exec.inject(
            "in",
            Packet::text("in", NodeId::new("test"), 1, "hello world"),
        );

        let result = exec.step().await.unwrap();
        assert!(!result.fired.is_empty(), "echo node should have fired");
        assert_eq!(exec.step_count, 1);
    }

    struct MockGoverningNode {
        received: std::sync::Arc<std::sync::Mutex<Vec<(String, MclValue)>>>,
    }

    impl MockGoverningNode {
        fn new(received: std::sync::Arc<std::sync::Mutex<Vec<(String, MclValue)>>>) -> Self {
            Self { received }
        }
    }

    #[async_trait::async_trait]
    impl Node for MockGoverningNode {
        fn capability(&self) -> NodeCapability {
            NodeCapability {
                type_name: "mock_governor".into(),
                description: "governor with wildcard sub".into(),
                subscriptions: vec![ChannelSpec::wildcard()],
                publications: vec![],
            }
        }

        async fn init(&mut self, _ctx: &mut NodeContext) -> Result<(), GraphError> {
            Ok(())
        }

        async fn process(
            &mut self,
            channel: &str,
            packets: &[Packet],
            _ctx: &mut NodeContext,
        ) -> Result<(), GraphError> {
            let mut received = self.received.lock().unwrap();
            for pkt in packets {
                received.push((channel.to_string(), pkt.payload.clone()));
            }
            Ok(())
        }
    }

    impl GoverningNode for MockGoverningNode {
        fn governance_kind(&self) -> GovernanceKind {
            GovernanceKind::Summarizer
        }
    }

    #[tokio::test]
    async fn wildcard_subscriber_receives_packets_from_all_channels() {
        let received: std::sync::Arc<std::sync::Mutex<Vec<(String, MclValue)>>> =
            std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let received_clone = received.clone();

        let mut factory = NodeFactory::new();
        factory.register_fn("echo", |_| Ok(Box::new(EchoNode)));
        factory.register_fn("collector", |_| {
            Ok(Box::new(CollectorNode { collected: vec![] }))
        });

        let def = GraphDef {
            name: "wildcard_test".into(),
            config: IndexMap::new(),
            nodes: IndexMap::from([
                (
                    "echo".to_string(),
                    NodeDef {
                        node_type: "echo".into(),
                        config: IndexMap::new(),
                        priority: 0,
                    },
                ),
                (
                    "governor".to_string(),
                    NodeDef {
                        node_type: "governor".into(),
                        config: IndexMap::new(),
                        priority: 0,
                    },
                ),
            ]),
            edges: vec![],
        };

        let mut exec = GraphExecutor::build_with_override(
            &def,
            &factory,
            "governor",
            Box::new(MockGoverningNode::new(received_clone)),
        )
        .await
        .unwrap();

        // Wire echo as subscriber to "in" channel
        {
            let ch = exec.channels.get_or_create("in");
            ch.subscribers.push(NodeId::new("echo"));
        }

        // Ensure channels exist so inject() doesn't silently drop packets
        exec.channels.get_or_create("alpha");
        exec.channels.get_or_create("beta");

        // Inject packets into multiple distinct channels
        exec.inject(
            "alpha",
            Packet::text("alpha", NodeId::new("src"), 1, "packet_a"),
        );
        exec.inject(
            "beta",
            Packet::text("beta", NodeId::new("src"), 2, "packet_b"),
        );
        exec.inject("in", Packet::text("in", NodeId::new("src"), 3, "packet_c"));

        let result = exec.step().await.unwrap();
        assert!(
            result.fired.iter().any(|id| id.as_str() == "governor"),
            "governor should have fired"
        );

        let received = received.lock().unwrap();
        let channels: std::collections::HashSet<&str> =
            received.iter().map(|(ch, _)| ch.as_str()).collect();
        assert!(
            channels.contains("alpha"),
            "governor should receive from alpha"
        );
        assert!(
            channels.contains("beta"),
            "governor should receive from beta"
        );
        assert!(channels.contains("in"), "governor should receive from in");
        assert_eq!(received.len(), 3, "governor should receive all 3 packets");
    }

    #[tokio::test]
    async fn interceptor_none_does_not_break_normal_flow() {
        // Verify that without an interceptor, signal-bearing packets pass through normally
        let mut factory = NodeFactory::new();
        factory.register_fn("echo", |_| Ok(Box::new(EchoNode)));

        let def = GraphDef {
            name: "no_interceptor_test".into(),
            config: IndexMap::new(),
            nodes: IndexMap::from([(
                "echo".to_string(),
                NodeDef {
                    node_type: "echo".into(),
                    config: IndexMap::new(),
                    priority: 0,
                },
            )]),
            edges: vec![],
        };

        let mut exec = GraphExecutor::build(&def, &factory).await.unwrap();

        {
            let ch = exec.channels.get_or_create("in");
            ch.subscribers.push(NodeId::new("echo"));
        }

        exec.inject("in", Packet::text("in", NodeId::new("test"), 1, "hello"));

        let result = exec.step().await.unwrap();
        assert!(result.fired.iter().any(|id| id.as_str() == "echo"));
    }

    struct RouteToInterceptor;

    impl PacketInterceptor for RouteToInterceptor {
        fn intercept(
            &self,
            _packet: &Packet,
            _origin: &NodeId,
            _turn: u64,
            _channel: &str,
        ) -> InterceptorVerdict {
            InterceptorVerdict::RouteTo {
                channel: "rerouted".to_string(),
                policy_id: "policy-42".to_string(),
                effect_flags: vec![],
            }
        }
    }

    #[tokio::test]
    async fn route_to_changes_signal_destination() {
        let mut factory = NodeFactory::new();
        factory.register_fn("signal_echo", |_| Ok(Box::new(SignalEchoNode)));

        let def = GraphDef {
            name: "route_to_test".into(),
            config: IndexMap::new(),
            nodes: IndexMap::from([(
                "echo".to_string(),
                NodeDef {
                    node_type: "signal_echo".into(),
                    config: IndexMap::new(),
                    priority: 0,
                },
            )]),
            edges: vec![],
        };

        let mut exec = GraphExecutor::build(&def, &factory).await.unwrap();
        exec.set_interceptor(std::sync::Arc::new(RouteToInterceptor));
        exec.channels.get_or_create("rerouted");

        {
            let ch = exec.channels.get_or_create("in");
            ch.subscribers.push(NodeId::new("echo"));
        }

        let mut pkt = Packet::text("in", NodeId::new("test"), 1, "hello");
        pkt.signals.push(MclSignal {
            kind: SignalKind::Action {
                name: "test_signal".to_string(),
                args: indexmap::IndexMap::new(),
            },
            span: Span::new(0, 1, 1, 1),
            labels: mcl_spec::label::LabelSet::default(),
        });
        exec.inject("in", pkt);

        exec.step().await.unwrap();

        let rerouted = exec.drain_channel("rerouted");
        assert_eq!(rerouted.len(), 1, "packet should be on rerouted channel");
        assert_eq!(rerouted[0].channel, "rerouted");
        assert_eq!(
            rerouted[0].policy_meta.as_ref().unwrap().policy_id,
            "policy-42"
        );

        let original = exec.drain_channel("out");
        assert!(
            original.is_empty(),
            "original channel should have no packets"
        );
    }

    struct ReplayUnsafeInterceptor;

    impl PacketInterceptor for ReplayUnsafeInterceptor {
        fn intercept(
            &self,
            _packet: &Packet,
            _origin: &NodeId,
            _turn: u64,
            channel: &str,
        ) -> InterceptorVerdict {
            InterceptorVerdict::RouteTo {
                channel: channel.to_string(),
                policy_id: "unsafe-policy".to_string(),
                effect_flags: vec!["replay_unsafe".to_string()],
            }
        }
    }

    #[tokio::test]
    async fn replay_unsafe_blocked_without_audit_record() {
        let mut factory = NodeFactory::new();
        factory.register_fn("signal_echo", |_| Ok(Box::new(SignalEchoNode)));

        let def = GraphDef {
            name: "replay_unsafe_test".into(),
            config: IndexMap::new(),
            nodes: IndexMap::from([(
                "echo".to_string(),
                NodeDef {
                    node_type: "signal_echo".into(),
                    config: IndexMap::new(),
                    priority: 0,
                },
            )]),
            edges: vec![],
        };

        let mut exec = GraphExecutor::build(&def, &factory).await.unwrap();
        exec.set_interceptor(std::sync::Arc::new(ReplayUnsafeInterceptor));
        exec.set_replay_mode(true);

        {
            let ch = exec.channels.get_or_create("in");
            ch.subscribers.push(NodeId::new("echo"));
        }

        let mut pkt = Packet::text("in", NodeId::new("test"), 1, "unsafe_action");
        pkt.signals.push(MclSignal {
            kind: SignalKind::Action {
                name: "unsafe_signal".to_string(),
                args: indexmap::IndexMap::new(),
            },
            span: Span::new(0, 1, 1, 1),
            labels: mcl_spec::label::LabelSet::default(),
        });
        exec.inject("in", pkt);

        exec.step().await.unwrap();

        let out = exec.drain_channel("out");
        assert!(
            out.is_empty(),
            "replay_unsafe packet without audit record should be rejected"
        );
    }

    struct ExternalNetworkInterceptor;

    impl PacketInterceptor for ExternalNetworkInterceptor {
        fn intercept(
            &self,
            _packet: &Packet,
            _origin: &NodeId,
            _turn: u64,
            channel: &str,
        ) -> InterceptorVerdict {
            InterceptorVerdict::RouteTo {
                channel: channel.to_string(),
                policy_id: "ext-policy".to_string(),
                effect_flags: vec!["external(network)".to_string()],
            }
        }
    }

    #[tokio::test]
    async fn effect_external_replay_uses_audit() {
        let mut factory = NodeFactory::new();
        factory.register_fn("signal_echo", |_| Ok(Box::new(SignalEchoNode)));

        let def = GraphDef {
            name: "external_replay_test".into(),
            config: IndexMap::new(),
            nodes: IndexMap::from([(
                "echo".to_string(),
                NodeDef {
                    node_type: "signal_echo".into(),
                    config: IndexMap::new(),
                    priority: 0,
                },
            )]),
            edges: vec![],
        };

        let mut exec = GraphExecutor::build(&def, &factory).await.unwrap();
        exec.set_interceptor(std::sync::Arc::new(ExternalNetworkInterceptor));
        exec.set_replay_mode(true);
        exec.channels.get_or_create("out");

        {
            let ch = exec.channels.get_or_create("in");
            ch.subscribers.push(NodeId::new("echo"));
        }

        let mut pkt = Packet::text("in", NodeId::new("test"), 1, "network_call");
        pkt.signals.push(MclSignal {
            kind: SignalKind::Action {
                name: "external_signal".to_string(),
                args: indexmap::IndexMap::new(),
            },
            span: Span::new(0, 1, 1, 1),
            labels: mcl_spec::label::LabelSet::default(),
        });
        exec.inject("in", pkt);

        exec.step().await.unwrap();

        let out = exec.drain_channel("out");
        assert_eq!(out.len(), 1, "packet should still be forwarded");
        assert_eq!(
            out[0].meta.get("replay_injected"),
            Some(&mcl_spec::value::MclValue::Bool(true)),
            "external(network) packet should have replay_injected flag in replay mode"
        );
    }
}
