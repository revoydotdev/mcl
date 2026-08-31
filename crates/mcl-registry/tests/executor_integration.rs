use std::sync::Arc;

use indexmap::IndexMap;

use mcl_graph::executor::GraphExecutor;
use mcl_graph::factory::NodeFactory;
use mcl_graph::graph_def::{GraphDef, NodeDef};
use mcl_graph::id::NodeId;
use mcl_graph::interceptor::{InterceptorVerdict, PacketInterceptor};
use mcl_graph::message::{Packet, PacketStatus};
use mcl_graph::node::{ChannelSpec, Node, NodeCapability};

use mcl_registry::ArbiterChain;
use mcl_registry::FsyncPolicy;
use mcl_registry::RegistryHandle;
use mcl_spec::label::LabelSet;
use mcl_spec::signal::{MclSignal, SignalKind};
use mcl_spec::span::Span;

fn test_span() -> Span {
    Span::new(0, 1, 1, 1)
}

struct RegistryInterceptor {
    arbiter_chain: ArbiterChain,
    registry: RegistryHandle,
}

impl RegistryInterceptor {
    fn new(arbiter_chain: ArbiterChain, registry: RegistryHandle) -> Self {
        Self {
            arbiter_chain,
            registry,
        }
    }
}

impl PacketInterceptor for RegistryInterceptor {
    fn intercept(
        &self,
        packet: &Packet,
        origin: &NodeId,
        turn: u64,
        channel: &str,
    ) -> InterceptorVerdict {
        let mut registry_ids = Vec::new();

        for signal in &packet.signals {
            let ctx = mcl_registry::IngestContext {
                origin_node: origin.clone(),
                turn,
                channel: channel.to_string(),
                source: mcl_spec::source::SourceId::default(),
            };

            let verdict = self.arbiter_chain.review(signal, &ctx);

            let _ = self.registry.append(
                signal.clone(),
                origin.clone(),
                turn,
                channel.to_string(),
                verdict.clone(),
            );

            match &verdict {
                mcl_registry::Verdict::Accept => {
                    let id = mcl_registry::SignalId::from_signal(origin, turn, channel, signal);
                    registry_ids.push(id.hex());
                }
                mcl_registry::Verdict::Reject { reason } => {
                    return InterceptorVerdict::Reject {
                        reason: reason.clone(),
                    };
                }
                mcl_registry::Verdict::Defer => {
                    return InterceptorVerdict::Defer;
                }
                mcl_registry::Verdict::RouteTo {
                    channel: rt_channel,
                    policy_id,
                } => {
                    return InterceptorVerdict::Reject {
                        reason: format!("routed to '{}' by policy '{}'", rt_channel, policy_id),
                    };
                }
            }
        }

        InterceptorVerdict::Accept { registry_ids }
    }
}

struct SignalEmitterNode {
    signals: Vec<MclSignal>,
}

#[async_trait::async_trait]
impl Node for SignalEmitterNode {
    fn capability(&self) -> NodeCapability {
        NodeCapability {
            type_name: "signal_emitter".into(),
            description: "emits packets with signals".into(),
            subscriptions: vec![ChannelSpec::new("in")],
            publications: vec![ChannelSpec::new("out")],
        }
    }

    async fn init(
        &mut self,
        _ctx: &mut mcl_graph::context::NodeContext,
    ) -> Result<(), mcl_graph::error::GraphError> {
        Ok(())
    }

    async fn process(
        &mut self,
        _channel: &str,
        packets: &[Packet],
        ctx: &mut mcl_graph::context::NodeContext,
    ) -> Result<(), mcl_graph::error::GraphError> {
        for pkt in packets {
            let text = pkt.text_content().unwrap_or("no_text").to_string();
            let mut out_pkt = Packet::text("out", pkt.origin.clone(), pkt.id, text);
            out_pkt.signals = self.signals.clone();
            ctx.emit_packet("out", out_pkt);
        }
        Ok(())
    }
}

#[tokio::test]
async fn interceptor_accepts_valid_signal_and_attaches_registry_id() {
    let tmp = tempfile::NamedTempFile::new().expect("temp file");
    let registry = RegistryHandle::new(tmp.path(), FsyncPolicy::OnShutdown).expect("registry");

    let mut chain = ArbiterChain::new();
    chain.add(Box::new(mcl_registry::SchemaArbiter::new()));

    let interceptor = Arc::new(RegistryInterceptor::new(chain, registry));

    let mut factory = NodeFactory::new();
    factory.register_fn("signal_emitter", |_| {
        Ok(Box::new(SignalEmitterNode {
            signals: vec![MclSignal {
                kind: SignalKind::Action {
                    name: "attack".to_string(),
                    args: Default::default(),
                },
                span: test_span(),
                labels: LabelSet::default(),
            }],
        }))
    });

    let def = GraphDef {
        name: "interceptor_test".into(),
        config: IndexMap::new(),
        nodes: IndexMap::from([(
            "emitter".to_string(),
            NodeDef {
                node_type: "signal_emitter".into(),
                config: IndexMap::new(),
                priority: 0,
            },
        )]),
        edges: vec![],
    };

    let mut exec = GraphExecutor::build(&def, &factory).await.unwrap();
    exec.set_interceptor(interceptor);
    exec.wire_subscriber("in", NodeId::new("emitter"));
    exec.create_channel("out");

    exec.inject("in", Packet::text("in", NodeId::new("test"), 1, "go"));
    exec.step().await.unwrap();

    let pending = exec.drain_channel("out");
    assert_eq!(pending.len(), 1, "accepted packet should be on out channel");
    assert!(
        pending[0].meta.contains_key("registry_ids"),
        "accepted packet should have registry_ids in meta"
    );
}

#[tokio::test]
async fn interceptor_passthrough_for_signalless_packets() {
    let tmp = tempfile::NamedTempFile::new().expect("temp file");
    let registry = RegistryHandle::new(tmp.path(), FsyncPolicy::OnShutdown).expect("registry");

    let mut chain = ArbiterChain::new();
    chain.add(Box::new(mcl_registry::SchemaArbiter::new()));

    let interceptor = Arc::new(RegistryInterceptor::new(chain, registry));

    let mut factory = NodeFactory::new();
    factory.register_fn("echo", |_| Ok(Box::new(EchoNode)));

    let def = GraphDef {
        name: "passthrough_test".into(),
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
    exec.set_interceptor(interceptor);
    exec.wire_subscriber("in", NodeId::new("echo"));
    exec.create_channel("out");

    exec.inject("in", Packet::text("in", NodeId::new("test"), 1, "go"));
    exec.step().await.unwrap();

    let pending = exec.drain_channel("out");
    assert_eq!(pending.len(), 1, "signalless packet should pass through");
    assert!(
        !pending[0].meta.contains_key("registry_ids"),
        "signalless packet should not have registry_ids"
    );
    assert!(
        pending[0].signals.is_empty(),
        "echo node should not add signals"
    );
}

#[tokio::test]
async fn interceptor_mixed_valid_and_invalid_in_same_step() {
    let tmp = tempfile::NamedTempFile::new().expect("temp file");
    let registry = RegistryHandle::new(tmp.path(), FsyncPolicy::OnShutdown).expect("registry");

    let mut chain = ArbiterChain::new();
    chain.add(Box::new(mcl_registry::SchemaArbiter::new()));

    let interceptor = Arc::new(RegistryInterceptor::new(chain, registry));

    let mut factory = NodeFactory::new();

    factory.register_fn("valid_emitter", |_| {
        Ok(Box::new(SignalEmitterNode {
            signals: vec![MclSignal {
                kind: SignalKind::Action {
                    name: "attack".to_string(),
                    args: Default::default(),
                },
                span: test_span(),
                labels: LabelSet::default(),
            }],
        }))
    });

    factory.register_fn("invalid_emitter", |_| {
        Ok(Box::new(SignalEmitterNode {
            signals: vec![MclSignal {
                kind: SignalKind::Action {
                    name: String::new(),
                    args: Default::default(),
                },
                span: test_span(),
                labels: LabelSet::default(),
            }],
        }))
    });

    factory.register_fn("passthrough", |_| Ok(Box::new(EchoNode)));

    let def = GraphDef {
        name: "mixed_test".into(),
        config: IndexMap::new(),
        nodes: IndexMap::from([
            (
                "valid".to_string(),
                NodeDef {
                    node_type: "valid_emitter".into(),
                    config: IndexMap::new(),
                    priority: 0,
                },
            ),
            (
                "invalid".to_string(),
                NodeDef {
                    node_type: "invalid_emitter".into(),
                    config: IndexMap::new(),
                    priority: 0,
                },
            ),
            (
                "plain".to_string(),
                NodeDef {
                    node_type: "passthrough".into(),
                    config: IndexMap::new(),
                    priority: 0,
                },
            ),
        ]),
        edges: vec![],
    };

    let mut exec = GraphExecutor::build(&def, &factory).await.unwrap();
    exec.set_interceptor(interceptor);
    exec.wire_subscriber("in", NodeId::new("valid"));
    exec.wire_subscriber("in", NodeId::new("invalid"));
    exec.wire_subscriber("in", NodeId::new("plain"));
    exec.create_channel("out");

    exec.inject("in", Packet::text("in", NodeId::new("test"), 1, "go"));

    let result = exec.step().await.unwrap();
    assert!(result.fired.iter().any(|id| id.as_str() == "valid"));
    assert!(result.fired.iter().any(|id| id.as_str() == "invalid"));
    assert!(result.fired.iter().any(|id| id.as_str() == "plain"));

    let pending = exec.drain_channel("out");

    let accepted = pending
        .iter()
        .filter(|p| p.meta.contains_key("registry_ids"))
        .count();
    let rejected = pending
        .iter()
        .filter(|p| p.status == PacketStatus::Failed)
        .count();
    let passthrough = pending
        .iter()
        .filter(|p| p.signals.is_empty() && p.status != PacketStatus::Failed)
        .count();

    assert_eq!(
        accepted, 1,
        "one packet should be accepted with registry_ids"
    );
    assert_eq!(
        rejected, 1,
        "one packet should be rejected with Failed status"
    );
    assert_eq!(passthrough, 1, "one signalless packet should pass through");
    assert_eq!(
        pending.len(),
        3,
        "total of 3 packets should be on out channel"
    );
}

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

    async fn init(
        &mut self,
        _ctx: &mut mcl_graph::context::NodeContext,
    ) -> Result<(), mcl_graph::error::GraphError> {
        Ok(())
    }

    async fn process(
        &mut self,
        _channel: &str,
        packets: &[Packet],
        ctx: &mut mcl_graph::context::NodeContext,
    ) -> Result<(), mcl_graph::error::GraphError> {
        for pkt in packets {
            let text = pkt.text_content().unwrap_or("").to_string();
            ctx.emit_text("out", text);
        }
        Ok(())
    }
}
