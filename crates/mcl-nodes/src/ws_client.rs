use async_trait::async_trait;
use mcl_graph::node::{ChannelSpec, NodeCapability};
use mcl_graph::{GraphError, Node, NodeContext, PacketStatus};
use mcl_spec::value::MclValue;
use tokio::sync::mpsc;

pub struct WsBridgeNode {
    inbound_rx: mpsc::UnboundedReceiver<String>,
    outbound_tx: mpsc::UnboundedSender<String>,
}

impl WsBridgeNode {
    pub fn new(
        inbound_rx: mpsc::UnboundedReceiver<String>,
        outbound_tx: mpsc::UnboundedSender<String>,
    ) -> Self {
        Self {
            inbound_rx,
            outbound_tx,
        }
    }
}

#[async_trait]
impl Node for WsBridgeNode {
    fn capability(&self) -> NodeCapability {
        NodeCapability {
            type_name: "ws_client".into(),
            description: "WebSocket bridge for server-side sessions".into(),
            subscriptions: vec![ChannelSpec::new("display")],
            publications: vec![ChannelSpec::new("user_input")],
        }
    }

    async fn init(&mut self, _ctx: &mut NodeContext) -> Result<(), GraphError> {
        Ok(())
    }

    async fn process(
        &mut self,
        _channel: &str,
        packets: &[mcl_graph::Packet],
        _ctx: &mut NodeContext,
    ) -> Result<(), GraphError> {
        for pkt in packets {
            if let Some(text) = pkt.text_content() {
                if self.outbound_tx.send(text.to_string()).is_err() {
                    return Err(GraphError::Execution(
                        "WebSocket client disconnected".into(),
                    ));
                }
            }

            if let Some(map) = pkt.payload.as_map() {
                if let Some(MclValue::String(hint)) = map.get("hint") {
                    let _ = self.outbound_tx.send(format!("\n[{hint}]"));
                }
            }

            if pkt.status == PacketStatus::Failed {
                if let Some(text) = pkt.text_content() {
                    let _ = self.outbound_tx.send(format!("[error: {text}]"));
                }
            }
        }
        Ok(())
    }

    fn is_source(&self) -> bool {
        true
    }

    async fn poll_source(
        &mut self,
        ctx: &mut NodeContext,
    ) -> Result<Option<mcl_graph::Packet>, GraphError> {
        match self.inbound_rx.try_recv() {
            Ok(text) => {
                let text = text.trim().to_string();
                if text.is_empty() {
                    return Ok(None);
                }

                match text.as_str() {
                    "/quit" | "/q" | "/exit" => {
                        return Err(GraphError::Execution("user quit".into()));
                    }
                    _ => {}
                }

                ctx.emit_text("user_input", &text);
                let mut outbound = ctx.drain_outbox();
                Ok(outbound.pop().map(|(_, pkt)| pkt))
            }
            Err(mpsc::error::TryRecvError::Empty) => Ok(None),
            Err(mpsc::error::TryRecvError::Disconnected) => Err(GraphError::Execution(
                "WebSocket client disconnected".into(),
            )),
        }
    }
}
