use std::io::{self, BufRead, Write};

use async_trait::async_trait;
use mcl_graph::node::{ChannelSpec, NodeCapability};
use mcl_graph::{GraphError, Node, NodeContext, Packet, PacketStatus};
use mcl_spec::value::MclValue;

/// Client node: bridges the graph to a human user via stdio.
/// This is a "source" node — it produces packets from stdin
/// without being triggered by upstream channels.
pub struct ClientNode;

#[async_trait]
impl Node for ClientNode {
    fn capability(&self) -> NodeCapability {
        NodeCapability {
            type_name: "client".into(),
            description: "stdio client for interactive sessions".into(),
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
        packets: &[Packet],
        _ctx: &mut NodeContext,
    ) -> Result<(), GraphError> {
        // Display incoming packets to the user
        for pkt in packets {
            if let Some(text) = pkt.text_content() {
                println!("{text}");
            }

            // Show resolution hints if present
            if let Some(map) = pkt.payload.as_map() {
                if let Some(MclValue::String(hint)) = map.get("hint") {
                    println!("\n[{hint}]");
                }
            }

            if pkt.status == PacketStatus::Failed {
                if let Some(text) = pkt.text_content() {
                    eprintln!("[error: {text}]");
                }
            }
        }

        println!();
        Ok(())
    }

    fn is_source(&self) -> bool {
        true
    }

    async fn poll_source(&mut self, ctx: &mut NodeContext) -> Result<Option<Packet>, GraphError> {
        let stdin = io::stdin();
        let mut stdout = io::stdout();

        print!("> ");
        stdout.flush().ok();

        let mut input = String::new();
        match stdin.lock().read_line(&mut input) {
            Ok(0) => return Err(GraphError::Execution("stdin closed".into())),
            Ok(_) => {}
            Err(e) => return Err(GraphError::Io(e.to_string())),
        }

        let input = input.trim();
        if input.is_empty() {
            return Ok(None);
        }

        // Handle client-side commands
        match input {
            "/quit" | "/q" | "/exit" => {
                return Err(GraphError::Execution("user quit".into()));
            }
            "/help" => {
                println!("\nCommands:");
                println!("  /quit  — exit the session");
                println!("  /help  — this message");
                println!("\nAnything else is sent as player input.\n");
                return Ok(None);
            }
            _ => {}
        }

        ctx.emit_text("user_input", input);
        // Return the packet from the outbox via drain
        let mut outbound = ctx.drain_outbox();
        Ok(outbound.pop().map(|(_, pkt)| pkt))
    }
}
