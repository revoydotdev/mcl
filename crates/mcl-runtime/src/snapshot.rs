use mcl_llm::backend::ChatMessage;
use mcl_spec::MclError;
use serde::{Deserialize, Serialize};

use crate::state::WorldState;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSnapshot {
    pub world_state: WorldState,
    pub active_context: Option<String>,
    pub conversation_history: Vec<ChatMessage>,
    pub backend_config: BackendConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendConfig {
    pub backend_name: String,
    pub model: String,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
}

impl SessionSnapshot {
    pub fn save_to_file(&self, path: &std::path::Path) -> Result<(), MclError> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| MclError::State(format!("failed to serialize snapshot: {e}")))?;

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        std::fs::write(path, json)?;
        Ok(())
    }

    pub fn load_from_file(path: &std::path::Path) -> Result<Self, MclError> {
        let json = std::fs::read_to_string(path).map_err(|e| {
            MclError::Io(format!(
                "failed to read snapshot file '{}': {e}",
                path.display()
            ))
        })?;

        serde_json::from_str(&json).map_err(|e| {
            MclError::State(format!(
                "failed to parse snapshot file '{}': {e}",
                path.display()
            ))
        })
    }

    pub fn summary(&self) -> String {
        let context = self.active_context.as_deref().unwrap_or("(none)");
        let history_len = self.conversation_history.len();
        let state_keys = self.world_state.local.len();
        let player_keys = self.world_state.player.len();
        let entity_count = self.world_state.entities.len();

        format!(
            "context={context}, history={history_len} msgs, local_keys={state_keys}, player_keys={player_keys}, entities={entity_count}, backend={}",
            self.backend_config.backend_name
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::IndexMap;
    use mcl_llm::backend::ChatRole;
    use mcl_spec::value::MclValue;
    use std::path::PathBuf;
    use tempfile::NamedTempFile;

    fn make_snapshot() -> SessionSnapshot {
        let mut state = WorldState::new();
        state.set("hp", MclValue::Int(50)).unwrap();
        state
            .set("player.name", MclValue::String("Hero".into()))
            .unwrap();

        let mut entity_props = IndexMap::new();
        entity_props.insert("hp".to_string(), MclValue::Int(10));
        state.entities.insert("goblin".to_string(), entity_props);

        SessionSnapshot {
            world_state: state,
            active_context: Some("crypt_entrance".to_string()),
            conversation_history: vec![
                ChatMessage {
                    role: ChatRole::User,
                    content: "look around".to_string(),
                },
                ChatMessage {
                    role: ChatRole::Assistant,
                    content: "You see a dark crypt.".to_string(),
                },
            ],
            backend_config: BackendConfig {
                backend_name: "ollama".to_string(),
                model: "llama3".to_string(),
                api_key: None,
                base_url: None,
            },
        }
    }

    #[test]
    fn save_creates_valid_json_file() {
        let snapshot = make_snapshot();
        let tmp = NamedTempFile::new().expect("create temp file");
        let path = tmp.path().to_path_buf();

        snapshot.save_to_file(&path).expect("save should succeed");

        let contents = std::fs::read_to_string(&path).expect("read file");
        let parsed: serde_json::Value = serde_json::from_str(&contents).expect("valid json");

        assert_eq!(parsed["active_context"], "crypt_entrance");
        assert_eq!(parsed["conversation_history"].as_array().unwrap().len(), 2);
        assert_eq!(parsed["backend_config"]["backend_name"], "ollama");
    }

    #[test]
    fn load_restores_state_and_history() {
        let snapshot = make_snapshot();
        let tmp = NamedTempFile::new().expect("create temp file");
        let path = tmp.path().to_path_buf();

        snapshot.save_to_file(&path).expect("save");

        let loaded = SessionSnapshot::load_from_file(&path).expect("load should succeed");

        assert_eq!(loaded.active_context, Some("crypt_entrance".to_string()));
        assert_eq!(loaded.conversation_history.len(), 2);
        assert_eq!(loaded.conversation_history[0].content, "look around");
        assert_eq!(
            loaded.conversation_history[1].content,
            "You see a dark crypt."
        );
        assert_eq!(loaded.world_state.local.get("hp"), Some(&MclValue::Int(50)));
        assert_eq!(
            loaded.world_state.player.get("name"),
            Some(&MclValue::String("Hero".into()))
        );
        assert_eq!(loaded.world_state.entities.len(), 1);
        assert_eq!(loaded.backend_config.backend_name, "ollama");
        assert_eq!(loaded.backend_config.model, "llama3");
    }

    #[test]
    fn load_missing_file_returns_error() {
        let result = SessionSnapshot::load_from_file(&PathBuf::from("/nonexistent/path/snap.json"));
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("failed to read snapshot file"));
    }

    #[test]
    fn load_invalid_json_returns_error() {
        let tmp = NamedTempFile::new().expect("create temp file");
        let path = tmp.path().to_path_buf();

        std::fs::write(&path, "not valid json {{{").expect("write garbage");

        let result = SessionSnapshot::load_from_file(&path);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("failed to parse snapshot file"));
    }

    #[test]
    fn round_trip_preserves_changelog() {
        let snapshot = make_snapshot();
        let tmp = NamedTempFile::new().expect("create temp file");
        let path = tmp.path().to_path_buf();

        assert_eq!(snapshot.world_state.changelog.len(), 2);

        snapshot.save_to_file(&path).expect("save");
        let loaded = SessionSnapshot::load_from_file(&path).expect("load");

        assert_eq!(loaded.world_state.changelog.len(), 2);
        assert_eq!(loaded.world_state.changelog[0].path, "hp");
    }

    #[test]
    fn summary_shows_key_info() {
        let snapshot = make_snapshot();
        let summary = snapshot.summary();
        assert!(summary.contains("context=crypt_entrance"));
        assert!(summary.contains("history=2 msgs"));
        assert!(summary.contains("backend=ollama"));
    }
}
