use std::path::Path;

use async_trait::async_trait;
use indexmap::IndexMap;
use mcl_graph::node::{ChannelSpec, NodeCapability};
use mcl_graph::{GraphError, Node, NodeContext, Packet};
use mcl_memory::{EmbeddingConfig, NodeMemory};
use mcl_runtime::context::ContextManager;
use mcl_runtime::prompt::build_system_prompt;
use mcl_runtime::state::WorldState;
use mcl_spec::signal::SignalKind;
use mcl_spec::value::MclValue;

const DEFAULT_MEMORY_CAPACITY: usize = 1000;

pub struct GameEngineNode {
    contexts: ContextManager,
    state: WorldState,
    memory_enabled: bool,
    memory_capacity: usize,
}

impl GameEngineNode {
    pub fn new() -> Self {
        Self {
            contexts: ContextManager::new(),
            state: WorldState::new(),
            memory_enabled: false,
            memory_capacity: DEFAULT_MEMORY_CAPACITY,
        }
    }

    pub fn from_config(ctx: &NodeContext) -> Self {
        let memory_enabled = ctx.config_bool("memory_enabled").unwrap_or(false);
        let memory_capacity = ctx
            .config
            .get("memory_capacity")
            .and_then(|v| v.as_int())
            .map(|n| n as usize)
            .unwrap_or(DEFAULT_MEMORY_CAPACITY);

        Self {
            contexts: ContextManager::new(),
            state: WorldState::new(),
            memory_enabled,
            memory_capacity,
        }
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

    pub fn load(&mut self, contexts_dir: &Path, start_context: &str) -> Result<(), GraphError> {
        self.contexts
            .load_directory(contexts_dir)
            .map_err(|e| GraphError::NodeError(format!("load contexts: {e}")))?;

        self.contexts
            .activate(start_context, &mut self.state)
            .map_err(|e| GraphError::NodeError(format!("activate context: {e}")))?;

        Ok(())
    }

    fn current_system_prompt(&self) -> Option<String> {
        self.contexts
            .active_doc()
            .map(|doc| build_system_prompt(doc, &self.state))
    }

    pub fn has_memory(&self) -> bool {
        self.memory_enabled
    }

    pub fn memory_capacity(&self) -> Option<usize> {
        if self.memory_enabled {
            Some(self.memory_capacity)
        } else {
            None
        }
    }
}

impl Default for GameEngineNode {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Node for GameEngineNode {
    fn capability(&self) -> NodeCapability {
        NodeCapability {
            type_name: "game_engine".into(),
            description: "game state manager, signal router, context transitions".into(),
            subscriptions: vec![
                ChannelSpec::new("narrative"),
                ChannelSpec::new("resolution"),
            ],
            publications: vec![
                ChannelSpec::new("display"),
                ChannelSpec::new("combat_action"),
                ChannelSpec::new("inventory_action"),
                ChannelSpec::new("feedback"),
                ChannelSpec::new("system_prompt"),
            ],
        }
    }

    async fn init(&mut self, ctx: &mut NodeContext) -> Result<(), GraphError> {
        if let Some(memory) = self.create_memory()? {
            ctx.set_memory(Some(memory));
        }

        if let Some(dir) = ctx.config_str("contexts_dir") {
            let dir = dir.to_string();
            let start = ctx.config_str("start_context").unwrap_or("").to_string();

            self.contexts
                .load_directory(Path::new(&dir))
                .map_err(|e| GraphError::NodeError(format!("load contexts: {e}")))?;

            let start_id = if start.is_empty() {
                self.contexts
                    .context_ids()
                    .first()
                    .map(|s| s.to_string())
                    .ok_or_else(|| GraphError::NodeError("no contexts loaded".into()))?
            } else {
                start
            };

            self.contexts
                .activate(&start_id, &mut self.state)
                .map_err(|e| GraphError::NodeError(format!("activate: {e}")))?;

            if let Some(prompt) = self.current_system_prompt() {
                ctx.emit_text("system_prompt", &prompt);
            }

            if let Some(doc) = self.contexts.active_doc() {
                let mut intro = format!("=== {} ===\n", doc.frontmatter.context);
                if let Some(ref scene) = doc.narrator.context {
                    intro.push_str(scene);
                }
                ctx.emit_text("display", &intro);
            }
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
            "narrative" => {
                self.state.clear_changelog();

                for pkt in packets {
                    let mut display_text = String::new();
                    if let Some(map) = pkt.payload.as_map() {
                        if let Some(MclValue::List(lines)) = map.get("narration") {
                            for line in lines {
                                if let Some(s) = line.as_str() {
                                    if !display_text.is_empty() {
                                        display_text.push('\n');
                                    }
                                    display_text.push_str(s);
                                }
                            }
                        }
                    }

                    for signal in &pkt.signals {
                        match &signal.kind {
                            SignalKind::Action { name, .. } => {
                                let doc = self.contexts.active_doc();
                                if let Some(doc) = doc {
                                    if let Some(action) = doc.actions.get(name).cloned() {
                                        match action.resolution.as_deref() {
                                            Some("combat_system") => {
                                                let mut action_pkt = IndexMap::new();
                                                action_pkt.insert(
                                                    "action".into(),
                                                    MclValue::String(name.clone()),
                                                );
                                                action_pkt.insert(
                                                    "signal".into(),
                                                    MclValue::String(signal.to_string()),
                                                );
                                                ctx.emit(
                                                    "combat_action",
                                                    MclValue::Map(action_pkt),
                                                );
                                            }
                                            Some("inventory_system") => {
                                                let mut action_pkt = IndexMap::new();
                                                action_pkt.insert(
                                                    "action".into(),
                                                    MclValue::String(name.clone()),
                                                );
                                                action_pkt.insert(
                                                    "signal".into(),
                                                    MclValue::String(signal.to_string()),
                                                );
                                                ctx.emit(
                                                    "inventory_action",
                                                    MclValue::Map(action_pkt),
                                                );
                                            }
                                            None => {
                                                let _ = self.state.apply_mutations(&action.mutates);
                                            }
                                            Some(_other) => {}
                                        }
                                    }
                                }
                            }

                            SignalKind::Transition { context } => {
                                let _ = self.contexts.transition(context, &mut self.state);

                                if let Some(prompt) = self.current_system_prompt() {
                                    ctx.emit_text("system_prompt", &prompt);
                                }

                                display_text.push_str(&format!("\n--- Entering {context} ---"));
                                if let Some(doc) = self.contexts.active_doc() {
                                    if let Some(ref scene) = doc.narrator.context {
                                        display_text.push('\n');
                                        display_text.push_str(scene);
                                    }
                                }
                            }

                            SignalKind::Mutate { path, value } => {
                                let _ = self.state.set(path, value.clone());
                            }

                            SignalKind::Dialogue { entity, text } => {
                                display_text.push_str(&format!("\n{entity}: \"{text}\""));
                            }

                            _ => {}
                        }
                    }

                    if let Some(target) = self.contexts.check_auto_transitions(&self.state) {
                        let _ = self.contexts.transition(&target, &mut self.state);
                        if let Some(prompt) = self.current_system_prompt() {
                            ctx.emit_text("system_prompt", &prompt);
                        }
                    }

                    if !display_text.is_empty() {
                        ctx.emit_text("display", &display_text);
                    }
                }
            }

            "resolution" => {
                for pkt in packets {
                    if let Some(map) = pkt.payload.as_map() {
                        for (key, val) in map {
                            if let Some(path) = key.strip_prefix("set:") {
                                let _ = self.state.set(path, val.clone());
                            }
                        }

                        if let Some(MclValue::String(hint)) = map.get("hint") {
                            let mut display = IndexMap::new();
                            display.insert("text".into(), MclValue::String(hint.clone()));
                            display.insert("hint".into(), MclValue::String(hint.clone()));
                            ctx.emit("display", MclValue::Map(display));
                        }

                        ctx.emit_text("feedback", pkt.payload.to_string());
                    }

                    if let Some(prompt) = self.current_system_prompt() {
                        ctx.emit_text("system_prompt", &prompt);
                    }
                }
            }

            _ => {}
        }

        Ok(())
    }
}
