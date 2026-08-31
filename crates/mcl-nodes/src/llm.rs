use async_trait::async_trait;
use indexmap::IndexMap;
use mcl_graph::node::{ChannelSpec, NodeCapability};
use mcl_graph::{GraphError, Node, NodeContext, Packet};
use mcl_llm::backend::{ChatMessage, ChatRole, LlmBackend};
use mcl_memory::{EmbeddingConfig, NodeMemory};
use mcl_spec::value::MclValue;

const DEFAULT_MEMORY_CAPACITY: usize = 1000;
const MEMORY_SEARCH_K: usize = 5;

pub struct LlmNode {
    backend: Box<dyn LlmBackend>,
    backend_name: String,
    model_name: String,
    history: Vec<ChatMessage>,
    system_prompt: String,
    memory_enabled: bool,
    memory_capacity: usize,
}

impl LlmNode {
    pub fn new(backend: Box<dyn LlmBackend>, system_prompt: String) -> Self {
        let name = backend.name().to_string();
        Self {
            backend,
            backend_name: name,
            model_name: String::new(),
            history: Vec::new(),
            system_prompt,
            memory_enabled: false,
            memory_capacity: DEFAULT_MEMORY_CAPACITY,
        }
    }

    pub fn from_config(ctx: &NodeContext) -> Result<Self, GraphError> {
        let backend_name = ctx.config_str("backend").unwrap_or("ollama");
        let model = ctx.config_str("model").unwrap_or("llama3").to_string();

        let (backend, resolved_backend_name): (Box<dyn LlmBackend>, String) = match backend_name {
            "ollama" => {
                let mut b = mcl_llm::ollama::OllamaBackend::new(&model);
                if let Some(url) = ctx.config_str("base_url") {
                    b = b.with_url(url);
                }
                (Box::new(b), "ollama".to_string())
            }
            "openai" => {
                let key = ctx
                    .config_str("api_key")
                    .or_else(|| std::env::var("OPENAI_API_KEY").ok().as_deref().map(|_| ""))
                    .ok_or_else(|| GraphError::NodeError("openai requires api_key".into()))?;
                let api_key = if key.is_empty() {
                    std::env::var("OPENAI_API_KEY")
                        .map_err(|_| GraphError::NodeError("OPENAI_API_KEY not set".into()))?
                } else {
                    key.to_string()
                };
                let mut b = mcl_llm::openai::OpenAiBackend::new(&model, &api_key);
                if let Some(url) = ctx.config_str("base_url") {
                    b = b.with_url(url);
                }
                (Box::new(b), "openai".to_string())
            }
            "anthropic" => {
                let key = ctx
                    .config_str("api_key")
                    .map(|s| s.to_string())
                    .or_else(|| std::env::var("ANTHROPIC_API_KEY").ok())
                    .ok_or_else(|| GraphError::NodeError("anthropic requires api_key".into()))?;
                (
                    Box::new(mcl_llm::anthropic::AnthropicBackend::new(&model, &key)),
                    "anthropic".to_string(),
                )
            }
            "mock" => {
                let responses = ctx
                    .config_str("mock_responses")
                    .map(|s| s.split(',').map(String::from).collect())
                    .unwrap_or_default();
                (
                    Box::new(mcl_llm::MockLlmBackend::new(responses)),
                    "mock".to_string(),
                )
            }
            other => return Err(GraphError::NodeError(format!("unknown backend: {other}"))),
        };

        let memory_enabled = ctx.config_bool("memory_enabled").unwrap_or(false);
        let memory_capacity = ctx
            .config
            .get("memory_capacity")
            .and_then(|v| v.as_int())
            .map(|n| n as usize)
            .unwrap_or(DEFAULT_MEMORY_CAPACITY);

        Ok(Self {
            backend,
            backend_name: resolved_backend_name,
            model_name: model,
            history: Vec::new(),
            system_prompt: String::new(),
            memory_enabled,
            memory_capacity,
        })
    }

    fn create_memory(&self) -> Result<Option<NodeMemory>, GraphError> {
        if !self.memory_enabled {
            return Ok(None);
        }
        let emb_config = EmbeddingConfig::default();
        let embedder = mcl_memory::create_embedder(&emb_config)
            .map_err(|e| GraphError::NodeError(format!("embedder init: {e}")))?;
        let memory = NodeMemory::new(self.memory_capacity, embedder, emb_config.output_dims())
            .map_err(|e| GraphError::NodeError(format!("memory init: {e}")))?;
        Ok(Some(memory))
    }

    pub fn set_system_prompt(&mut self, prompt: String) {
        self.system_prompt = prompt;
    }

    pub fn has_memory(&self) -> bool {
        self.memory_enabled
    }

    /// Returns the backend name (e.g. "ollama", "openai", "anthropic", "mock")
    pub fn backend_name(&self) -> &str {
        &self.backend_name
    }

    /// Returns the model name configured for this node
    pub fn model_name(&self) -> &str {
        &self.model_name
    }

    pub fn memory_capacity(&self) -> Option<usize> {
        if self.memory_enabled {
            Some(self.memory_capacity)
        } else {
            None
        }
    }

    fn build_memory_context(ctx: &mut NodeContext, query: &str) -> String {
        let memory = match ctx.memory_mut() {
            Some(m) => m,
            None => return String::new(),
        };

        let mut entries = Vec::new();
        let outgoing = memory.outgoing.search(query, MEMORY_SEARCH_K);
        for e in outgoing {
            entries.push(e);
        }
        let local = memory.local.search(query, MEMORY_SEARCH_K);
        for e in local {
            entries.push(e);
        }

        if entries.is_empty() {
            return String::new();
        }

        let mut section = String::from("\n## Relevant Memory\n");
        for entry in &entries {
            let elapsed = entry
                .timestamp
                .elapsed()
                .map(|d| format!("{:.0}s ago", d.as_secs_f64()))
                .unwrap_or_else(|_| "unknown".into());
            section.push_str(&format!("- [{elapsed}] {}\n", entry.text));
        }
        section
    }
}

#[async_trait]
impl Node for LlmNode {
    fn capability(&self) -> NodeCapability {
        NodeCapability {
            type_name: "llm".into(),
            description: "LLM narrative/decision engine".into(),
            subscriptions: vec![
                ChannelSpec::new("user_input"),
                ChannelSpec::new("feedback"),
                ChannelSpec::new("system_prompt"),
            ],
            publications: vec![ChannelSpec::new("narrative")],
        }
    }

    async fn init(&mut self, ctx: &mut NodeContext) -> Result<(), GraphError> {
        if self.system_prompt.is_empty() {
            if let Some(prompt) = ctx.config_str("system_prompt") {
                self.system_prompt = prompt.to_string();
            }
        }
        if let Some(memory) = self.create_memory()? {
            ctx.set_memory(Some(memory));
        }
        Ok(())
    }

    async fn process(
        &mut self,
        channel: &str,
        packets: &[Packet],
        ctx: &mut NodeContext,
    ) -> Result<(), GraphError> {
        match channel {
            "user_input" => {
                for pkt in packets {
                    let user_text = pkt.text_content().unwrap_or("");
                    if user_text.is_empty() {
                        continue;
                    }

                    let memory_context = Self::build_memory_context(ctx, user_text);
                    let augmented_prompt = if memory_context.is_empty() {
                        self.system_prompt.clone()
                    } else {
                        format!("{}{memory_context}", self.system_prompt)
                    };

                    let response = self
                        .backend
                        .complete(&augmented_prompt, &self.history, user_text)
                        .await
                        .map_err(|e| GraphError::NodeError(format!("LLM error: {e}")))?;

                    self.history.push(ChatMessage {
                        role: ChatRole::User,
                        content: user_text.to_string(),
                    });
                    self.history.push(ChatMessage {
                        role: ChatRole::Assistant,
                        content: response.clone(),
                    });

                    let parsed = mcl_signal::extract_signals(&response);

                    let mut payload = IndexMap::new();
                    payload.insert("text".into(), MclValue::String(response.clone()));
                    payload.insert(
                        "narration".into(),
                        MclValue::List(
                            parsed.narration.into_iter().map(MclValue::String).collect(),
                        ),
                    );

                    let mut pkt = mcl_graph::Packet::new(
                        "narrative",
                        ctx.node_id.clone(),
                        0,
                        MclValue::Map(payload),
                    );
                    pkt.signals = parsed.signals;

                    ctx.emit_packet("narrative", pkt);
                }
            }

            "feedback" => {
                for pkt in packets {
                    let text = pkt.text_content().unwrap_or("");
                    if !text.is_empty() {
                        self.history.push(ChatMessage {
                            role: ChatRole::User,
                            content: format!("[System: {text}]"),
                        });
                    }
                }
            }

            "system_prompt" => {
                if let Some(pkt) = packets.last() {
                    if let Some(text) = pkt.text_content() {
                        self.system_prompt = text.to_string();
                    }
                }
            }

            _ => {}
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::IndexMap;
    use mcl_graph::NodeId;
    use mcl_spec::value::MclValue;

    fn mock_ctx(config: IndexMap<String, MclValue>) -> NodeContext {
        NodeContext::new(NodeId::new("test"), config)
    }

    fn config_map(pairs: Vec<(&str, &str)>) -> IndexMap<String, MclValue> {
        pairs
            .into_iter()
            .map(|(k, v)| (k.to_string(), MclValue::String(v.to_string())))
            .collect()
    }

    #[test]
    fn per_node_model_defaults_to_ollama_llama3() {
        let node = LlmNode::from_config(&mock_ctx(IndexMap::new())).unwrap();
        assert_eq!(node.backend_name(), "ollama");
        assert_eq!(node.model_name(), "llama3");
    }

    #[test]
    fn per_node_model_reads_custom_backend_and_model() {
        let config = config_map(vec![("backend", "mock"), ("model", "gpt-4o")]);
        let node = LlmNode::from_config(&mock_ctx(config)).unwrap();
        assert_eq!(node.backend_name(), "mock");
        assert_eq!(node.model_name(), "gpt-4o");
    }

    #[test]
    fn per_node_model_openai_with_custom_model() {
        std::env::set_var("OPENAI_API_KEY", "sk-test-key");
        let config = config_map(vec![("backend", "openai"), ("model", "gpt-4o-mini")]);
        let node = LlmNode::from_config(&mock_ctx(config)).unwrap();
        assert_eq!(node.backend_name(), "openai");
        assert_eq!(node.model_name(), "gpt-4o-mini");
    }

    #[test]
    fn per_node_model_anthropic_with_custom_model() {
        std::env::set_var("ANTHROPIC_API_KEY", "sk-ant-test");
        let config = config_map(vec![
            ("backend", "anthropic"),
            ("model", "claude-3-haiku-20240307"),
        ]);
        let node = LlmNode::from_config(&mock_ctx(config)).unwrap();
        assert_eq!(node.backend_name(), "anthropic");
        assert_eq!(node.model_name(), "claude-3-haiku-20240307");
    }

    #[test]
    fn per_node_model_unknown_backend_errors() {
        let config = config_map(vec![("backend", "nonexistent"), ("model", "anything")]);
        let result = LlmNode::from_config(&mock_ctx(config));
        assert!(result.is_err());
        let err = format!("{:?}", result.err());
        assert!(err.contains("unknown backend"));
    }

    #[test]
    fn per_node_model_mock_backend_with_responses() {
        let config = config_map(vec![
            ("backend", "mock"),
            ("model", "test-model"),
            ("mock_responses", "resp1,resp2,resp3"),
        ]);
        let node = LlmNode::from_config(&mock_ctx(config)).unwrap();
        assert_eq!(node.backend_name(), "mock");
        assert_eq!(node.model_name(), "test-model");
    }

    #[test]
    fn per_node_model_factory_creates_nodes_with_different_configs() {
        let narrator_ctx = mock_ctx(config_map(vec![("backend", "mock"), ("model", "gpt-4o")]));
        let narrator = LlmNode::from_config(&narrator_ctx).unwrap();
        assert_eq!(narrator.backend_name(), "mock");
        assert_eq!(narrator.model_name(), "gpt-4o");

        let picker_ctx = mock_ctx(config_map(vec![
            ("backend", "mock"),
            ("model", "claude-3-haiku"),
        ]));
        let picker = LlmNode::from_config(&picker_ctx).unwrap();
        assert_eq!(picker.backend_name(), "mock");
        assert_eq!(picker.model_name(), "claude-3-haiku");

        assert_ne!(
            narrator.model_name(),
            picker.model_name(),
            "two llm nodes should have different models"
        );
    }

    #[test]
    fn per_node_model_toml_graph_def_carries_per_node_config() {
        use mcl_graph::graph_def::GraphDefParser;
        use mcl_graph::graph_def::TomlGraphDefParser;

        let toml = r#"
[graph]
name = "dual_model_graph"

[nodes.narrator]
type = "llm"
config.backend = "mock"
config.model = "gpt-4o"

[nodes.picker]
type = "llm"
config.backend = "mock"
config.model = "claude-3-haiku"

[[edges]]
from = "narrator"
to = "picker"
channel = "narrative"
"#;
        let parser = TomlGraphDefParser;
        let def = parser.parse(toml).unwrap();

        let narrator_config = &def.nodes["narrator"].config;
        assert_eq!(
            narrator_config.get("backend"),
            Some(&MclValue::String("mock".into()))
        );
        assert_eq!(
            narrator_config.get("model"),
            Some(&MclValue::String("gpt-4o".into()))
        );

        let picker_config = &def.nodes["picker"].config;
        assert_eq!(
            picker_config.get("backend"),
            Some(&MclValue::String("mock".into()))
        );
        assert_eq!(
            picker_config.get("model"),
            Some(&MclValue::String("claude-3-haiku".into()))
        );

        assert_ne!(
            narrator_config.get("model"),
            picker_config.get("model"),
            "two nodes should have different model configs"
        );
    }
}
