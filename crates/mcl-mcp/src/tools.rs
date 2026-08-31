use std::path::Path;

use indexmap::IndexMap;
use mcl_runtime::knowledge_filter::filter_state_for_knowledge;
use mcl_runtime::{ContextManager, ResolverRegistry, WorldState};
use mcl_spec::document::MclDocument;
use mcl_spec::label::LabelSet;
use mcl_spec::span::Span;
use mcl_spec::value::MclValue;
use rand::SeedableRng;

pub struct McpToolHandler {
    contexts: ContextManager,
    state: WorldState,
    resolvers: ResolverRegistry,
    pub(crate) active_context: Option<String>,
}

impl McpToolHandler {
    pub fn new(contexts_dir: &Path, start_context: &str) -> Result<Self, String> {
        let mut contexts = ContextManager::new();

        contexts
            .load_directory(contexts_dir)
            .map_err(|e| format!("load directory: {e}"))?;

        let mut state = WorldState::new();
        contexts
            .activate(start_context, &mut state)
            .map_err(|e| format!("activate: {e}"))?;

        let mut resolvers = ResolverRegistry::new();
        mcl_domains::rpg::register_rpg_resolvers(&mut resolvers);

        Ok(Self {
            contexts,
            state,
            resolvers,
            active_context: Some(start_context.to_string()),
        })
    }

    pub fn list_tools(&self) -> Vec<serde_json::Value> {
        vec![
            serde_json::json!({
                "name": "mcl.get_state",
                "description": "Get current world state (filtered by knowledge boundaries)",
                "inputSchema": { "type": "object", "properties": {} }
            }),
            serde_json::json!({
                "name": "mcl.list_actions",
                "description": "List available actions in the current context",
                "inputSchema": { "type": "object", "properties": {} }
            }),
            serde_json::json!({
                "name": "mcl.resolve_action",
                "description": "Resolve a player action through MCL's deterministic game engine",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "action": { "type": "string", "description": "Action name" },
                        "target": { "type": "string", "description": "Target entity or item ID" }
                    },
                    "required": ["action"]
                }
            }),
            serde_json::json!({
                "name": "mcl.transition",
                "description": "Move to another context/room",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "target": { "type": "string", "description": "Context ID to transition to" }
                    },
                    "required": ["target"]
                }
            }),
            serde_json::json!({
                "name": "mcl.get_prompt",
                "description": "Get the system prompt for the current context",
                "inputSchema": { "type": "object", "properties": {} }
            }),
        ]
    }

    pub fn call_tool(
        &mut self,
        name: &str,
        args: &serde_json::Value,
    ) -> Result<serde_json::Value, (i32, String)> {
        match name {
            "mcl.get_state" => self.tool_get_state(),
            "mcl.list_actions" => self.tool_list_actions(),
            "mcl.resolve_action" => self.tool_resolve_action(args),
            "mcl.transition" => self.tool_transition(args),
            "mcl.get_prompt" => self.tool_get_prompt(),
            _ => Err((-32601, format!("Unknown tool: {name}"))),
        }
    }

    pub fn list_resources(&self) -> Vec<serde_json::Value> {
        vec![
            serde_json::json!({
                "uri": "mcl://contexts",
                "name": "Loaded MCL contexts",
                "mimeType": "application/json"
            }),
            serde_json::json!({
                "uri": "mcl://state",
                "name": "Current world state",
                "mimeType": "application/json"
            }),
        ]
    }

    pub fn read_resource(&self, uri: &str) -> Result<serde_json::Value, (i32, String)> {
        match uri {
            "mcl://contexts" => {
                let names: Vec<&str> = self.contexts.context_ids();
                let text = serde_json::to_string(&names).unwrap_or_else(|_| "[]".into());
                Ok(serde_json::json!({
                    "contents": [{
                        "uri": "mcl://contexts",
                        "mimeType": "application/json",
                        "text": text
                    }]
                }))
            }
            "mcl://state" => {
                let state_json = serde_json::to_string_pretty(&self.state)
                    .map_err(|e| (-32603, format!("state serialize: {e}")))?;
                Ok(serde_json::json!({
                    "contents": [{
                        "uri": "mcl://state",
                        "mimeType": "application/json",
                        "text": state_json
                    }]
                }))
            }
            _ => Err((-32602, format!("Unknown resource: {uri}"))),
        }
    }

    fn get_active_doc(&self) -> Result<&MclDocument, (i32, String)> {
        let ctx_name = self
            .active_context
            .as_deref()
            .ok_or((-32603, "No active context".to_string()))?;
        self.contexts
            .get(ctx_name)
            .ok_or((-32603, format!("Context '{ctx_name}' not found")))
    }

    fn tool_get_state(&self) -> Result<serde_json::Value, (i32, String)> {
        let doc = self.get_active_doc()?;
        let filtered = filter_state_for_knowledge(&self.state, &doc.knowledge);
        let json =
            serde_json::to_value(&filtered).map_err(|e| (-32603, format!("serialize: {e}")))?;
        let text = serde_json::to_string_pretty(&json)
            .map_err(|e| (-32603, format!("pretty print: {e}")))?;
        Ok(serde_json::json!({
            "content": [{ "type": "text", "text": text }]
        }))
    }

    fn tool_list_actions(&self) -> Result<serde_json::Value, (i32, String)> {
        let doc = self.get_active_doc()?;
        let actions: Vec<serde_json::Value> = doc
            .actions
            .iter()
            .map(|(name, action)| {
                serde_json::json!({
                    "name": name,
                    "description": action.description,
                    "has_target": action.target.is_some(),
                    "trigger_phrases": action.trigger_phrases,
                })
            })
            .collect();
        let text = serde_json::to_string_pretty(&actions)
            .map_err(|e| (-32603, format!("pretty print: {e}")))?;
        Ok(serde_json::json!({
            "content": [{ "type": "text", "text": text }]
        }))
    }

    fn tool_resolve_action(
        &mut self,
        args: &serde_json::Value,
    ) -> Result<serde_json::Value, (i32, String)> {
        let action_name = args["action"]
            .as_str()
            .ok_or((-32602, "Missing 'action' parameter".to_string()))?;
        let target = args["target"].as_str();

        let doc = self.get_active_doc()?.clone();
        let action_def = doc
            .actions
            .get(action_name)
            .ok_or((-32602, format!("Unknown action: {action_name}")))?;

        // Check if action has a resolver
        if let Some(ref resolution) = action_def.resolution {
            if let Some(resolver) = self.resolvers.get(resolution) {
                let mut signal_args = IndexMap::new();
                if let Some(t) = target {
                    signal_args.insert("target".to_string(), MclValue::String(t.to_string()));
                }
                let signal = mcl_spec::signal::MclSignal {
                    kind: mcl_spec::signal::SignalKind::Action {
                        name: action_name.to_string(),
                        args: signal_args,
                    },
                    span: Span::empty(),
                    labels: LabelSet::default(),
                };

                let mut rng = rand::rngs::StdRng::seed_from_u64(42);
                match resolver.resolve(action_def, &signal, &self.state, &mut rng) {
                    Ok(resolution) => {
                        // Apply state changes from resolution details
                        for (key, val) in &resolution.details {
                            if let Some(path) = key.strip_prefix("set:") {
                                let _ = self.state.set(path, val.clone());
                            }
                        }
                        let json = serde_json::to_value(&resolution)
                            .map_err(|e| (-32603, format!("serialize: {e}")))?;
                        let text = serde_json::to_string_pretty(&json)
                            .map_err(|e| (-32603, format!("pretty print: {e}")))?;
                        return Ok(serde_json::json!({
                            "content": [{ "type": "text", "text": text }]
                        }));
                    }
                    Err(e) => return Err((-32603, format!("Resolver error: {e}"))),
                }
            }
        }

        // Direct mutate actions (no resolver)
        for (key, val) in &action_def.mutates {
            let _ = self.state.set(key, val.clone());
        }

        Ok(serde_json::json!({
            "content": [{ "type": "text", "text": serde_json::to_string_pretty(&serde_json::json!({
                "action": action_name,
                "outcome": "success",
                "mutates_applied": action_def.mutates.keys().collect::<Vec<_>>(),
            })).unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}")) }]
        }))
    }

    fn tool_transition(
        &mut self,
        args: &serde_json::Value,
    ) -> Result<serde_json::Value, (i32, String)> {
        let target = args["target"]
            .as_str()
            .ok_or((-32602, "Missing 'target' parameter".to_string()))?;

        self.contexts
            .transition(target, &mut self.state)
            .map_err(|e| (-32603, format!("Transition error: {e}")))?;
        self.active_context = Some(target.to_string());

        Ok(serde_json::json!({
            "content": [{ "type": "text", "text": format!("Transitioned to '{target}'") }]
        }))
    }

    fn tool_get_prompt(&self) -> Result<serde_json::Value, (i32, String)> {
        let doc = self.get_active_doc()?;
        let prompt = mcl_runtime::prompt::build_system_prompt(doc, &self.state);
        Ok(serde_json::json!({
            "content": [{ "type": "text", "text": prompt }]
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handler_initializes_with_crypt() {
        let handler =
            McpToolHandler::new(Path::new("../../examples/crypt"), "crypt_antechamber").unwrap();
        assert!(handler.active_context.is_some());
    }

    #[test]
    fn list_tools_returns_five() {
        let handler =
            McpToolHandler::new(Path::new("../../examples/crypt"), "crypt_antechamber").unwrap();
        assert_eq!(handler.list_tools().len(), 5);
    }

    #[test]
    fn get_state_respects_knowledge_boundaries() {
        let mut handler =
            McpToolHandler::new(Path::new("../../examples/crypt"), "crypt_antechamber").unwrap();
        handler
            .state
            .set("secret_passage", MclValue::Bool(true))
            .unwrap();
        let result = handler.tool_get_state().unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(!text.contains("secret_passage"));
    }

    #[test]
    fn list_actions_returns_actions() {
        let handler =
            McpToolHandler::new(Path::new("../../examples/crypt"), "crypt_antechamber").unwrap();
        let result = handler.tool_list_actions().unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("attack"));
        assert!(text.contains("look"));
    }
}
