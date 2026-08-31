use async_trait::async_trait;
use mcl_graph::node::{ChannelSpec, NodeCapability};
use mcl_graph::{GraphError, Node, NodeContext, Packet};
use mcl_llm::backend::ChatMessage;
use mcl_llm::LlmBackend;

/// LLM-driven autonomous player node. Uses an LLM to decide player actions
/// based on the narrator's output.
pub struct PlayerLlmNode {
    backend: Box<dyn LlmBackend>,
    system_prompt: String,
    history: Vec<ChatMessage>,
    turn_count: usize,
    max_turns: usize,
    pending_display: Option<String>,
    started: bool,
    pub transcript: Vec<PlayerTranscriptEntry>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PlayerTranscriptEntry {
    pub turn: usize,
    pub input: String,
    pub display_text: String,
}

const PLAYER_SYSTEM_PROMPT: &str = "\
You are a player in a text adventure game. Read the narrator's description and decide what to do next.

Rules:
- Respond with ONLY a short action command (2-8 words)
- Try to: explore rooms, fight enemies, collect items, unlock doors, progress through the dungeon
- Be curious and thorough — examine things, pick up items, talk to NPCs
- When you've explored everything in a room, try to move to the next area
- Do NOT narrate or describe — just state your action plainly

Examples of good responses:
- look around
- attack the skeleton guard
- pick up the flint
- examine the rusty key
- light the torch
- unlock the northern door
- go north
- talk to the ghost";

impl PlayerLlmNode {
    pub fn new(backend: Box<dyn LlmBackend>, max_turns: usize) -> Self {
        Self {
            backend,
            system_prompt: PLAYER_SYSTEM_PROMPT.to_string(),
            history: Vec::new(),
            turn_count: 0,
            max_turns,
            pending_display: None,
            started: false,
            transcript: Vec::new(),
        }
    }

    pub fn from_config(ctx: &NodeContext) -> Result<Self, GraphError> {
        let backend_name = ctx.config_str("backend").unwrap_or("openai");
        let model = ctx
            .config_str("model")
            .unwrap_or("meta-llama/llama-4-scout");
        let api_key = ctx.config_str("api_key").unwrap_or("");
        let base_url = ctx
            .config_str("base_url")
            .unwrap_or("https://openrouter.ai/api/v1");
        let max_turns = ctx
            .config
            .get("max_turns")
            .and_then(|v| v.as_int())
            .unwrap_or(30) as usize;

        let backend: Box<dyn LlmBackend> = match backend_name {
            "ollama" => {
                let mut b = mcl_llm::ollama::OllamaBackend::new(model);
                if base_url != "https://openrouter.ai/api/v1" {
                    b = b.with_url(base_url);
                }
                Box::new(b)
            }
            _ => Box::new(mcl_llm::openai::OpenAiBackend::new(model, api_key).with_url(base_url)),
        };

        Ok(Self::new(backend, max_turns))
    }

    /// Create with an explicit backend (for programmatic construction)
    pub fn with_backend(backend: Box<dyn LlmBackend>, max_turns: usize) -> Self {
        Self::new(backend, max_turns)
    }
}

#[async_trait]
impl Node for PlayerLlmNode {
    fn capability(&self) -> NodeCapability {
        NodeCapability {
            type_name: "player_llm".into(),
            description: "LLM-driven autonomous player".into(),
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
        // Store display text for the LLM to read
        let mut combined = String::new();
        for pkt in packets {
            if let Some(text) = pkt.text_content() {
                if !combined.is_empty() {
                    combined.push('\n');
                }
                combined.push_str(text);
            }
        }
        if !combined.is_empty() {
            self.pending_display = Some(combined);
        }
        Ok(())
    }

    fn is_source(&self) -> bool {
        true
    }

    async fn poll_source(&mut self, ctx: &mut NodeContext) -> Result<Option<Packet>, GraphError> {
        if self.turn_count >= self.max_turns {
            return Err(GraphError::Execution("max turns reached".into()));
        }

        // On first call, emit an initial action without waiting for display
        if !self.started {
            self.started = true;
            self.turn_count += 1;
            let initial = "look around".to_string();
            self.transcript.push(PlayerTranscriptEntry {
                turn: self.turn_count,
                input: initial.clone(),
                display_text: String::new(),
            });
            ctx.emit_text("user_input", &initial);
            let mut outbound = ctx.drain_outbox();
            return Ok(outbound.pop().map(|(_, pkt)| pkt));
        }

        // Wait for display text from narrator
        let display_text = match self.pending_display.take() {
            Some(text) => text,
            None => return Ok(None), // No display yet, wait
        };

        // Add narrator output to history
        self.history.push(ChatMessage {
            role: mcl_llm::ChatRole::User,
            content: display_text.clone(),
        });

        // Call LLM to decide action
        let action = self
            .backend
            .complete(&self.system_prompt, &self.history, &display_text)
            .await
            .map_err(|e| GraphError::Execution(format!("player LLM error: {e}")))?;

        // Clean up the response — take just the first line, trim
        let action = action
            .lines()
            .next()
            .unwrap_or("look around")
            .trim()
            .to_string();

        // Add our action to history
        self.history.push(ChatMessage {
            role: mcl_llm::ChatRole::Assistant,
            content: action.clone(),
        });

        self.turn_count += 1;
        self.transcript.push(PlayerTranscriptEntry {
            turn: self.turn_count,
            input: action.clone(),
            display_text,
        });

        ctx.emit_text("user_input", &action);
        let mut outbound = ctx.drain_outbox();
        Ok(outbound.pop().map(|(_, pkt)| pkt))
    }
}
