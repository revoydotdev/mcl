use async_trait::async_trait;
use mcl_graph::node::{ChannelSpec, NodeCapability};
use mcl_graph::{GraphError, Node, NodeContext, Packet};

/// Automated player that executes a scripted sequence of actions.
/// Replaces ClientNode for automated playtesting.
pub struct ScriptedPlayerNode {
    actions: Vec<String>,
    index: usize,
    max_turns: usize,
    turn_count: usize,
    /// Store last display text for transcript capture
    last_display: Option<String>,
    /// Collected transcript entries
    pub transcript: Vec<TranscriptEntry>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct TranscriptEntry {
    pub turn: usize,
    pub input: String,
    pub display_text: String,
}

impl ScriptedPlayerNode {
    pub fn new(actions: Vec<String>, max_turns: usize) -> Self {
        Self {
            actions,
            index: 0,
            max_turns,
            turn_count: 0,
            last_display: None,
            transcript: Vec::new(),
        }
    }

    /// Create from NodeContext config
    pub fn from_config(ctx: &NodeContext) -> Self {
        let actions = ctx
            .config
            .get("actions")
            .and_then(|v| v.as_list())
            .map(|list| {
                list.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_else(default_crypt_actions);
        let max_turns = ctx
            .config
            .get("max_turns")
            .and_then(|v| v.as_int())
            .unwrap_or(30) as usize;
        Self::new(actions, max_turns)
    }
}

pub fn default_crypt_actions() -> Vec<String> {
    vec![
        "look around",
        "examine the skeleton guard",
        "pick up the flint",
        "light the torch",
        "examine the rusty key",
        "pick up the rusty key",
        "attack the skeleton guard",
        "attack the skeleton guard",
        "attack the skeleton guard",
        "unlock the northern door",
        "go north",
        "look around",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}

#[async_trait]
impl Node for ScriptedPlayerNode {
    fn capability(&self) -> NodeCapability {
        NodeCapability {
            type_name: "scripted_player".into(),
            description: "Automated scripted player for playtesting".into(),
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
        // Capture display output
        for pkt in packets {
            if let Some(text) = pkt.text_content() {
                self.last_display = Some(text.to_string());
            }
        }
        Ok(())
    }

    fn is_source(&self) -> bool {
        true
    }

    async fn poll_source(&mut self, ctx: &mut NodeContext) -> Result<Option<Packet>, GraphError> {
        if self.turn_count >= self.max_turns || self.index >= self.actions.len() {
            return Err(GraphError::Execution("playthrough complete".into()));
        }

        let action = self.actions[self.index].clone();
        self.index += 1;
        self.turn_count += 1;

        // Record transcript
        self.transcript.push(TranscriptEntry {
            turn: self.turn_count,
            input: action.clone(),
            display_text: self.last_display.take().unwrap_or_default(),
        });

        ctx.emit_text("user_input", &action);
        let mut outbound = ctx.drain_outbox();
        Ok(outbound.pop().map(|(_, pkt)| pkt))
    }
}
