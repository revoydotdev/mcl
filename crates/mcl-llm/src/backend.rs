use std::pin::Pin;
use std::sync::OnceLock;

use async_trait::async_trait;
use futures::stream::{self, Stream};
use mcl_spec::MclError;
use serde::{Deserialize, Serialize};

pub type LlmStream = Pin<Box<dyn Stream<Item = Result<String, MclError>> + Send>>;

/// Warn once per process if the model is unknown. Call from backend
/// `complete()` implementations or from the call site.
pub fn warn_if_unknown_model(provider: &str, model: &str) {
    if crate::models::is_unknown(provider, model) {
        UNKNOWN_MODEL_WARNED.get_or_init(|| {
            log::warn!(
                "unknown LLM model: provider={}, model={}. \
                 See MODELS.md at repo root for the compatibility matrix. \
                 Unknown models are not blocked but may produce unexpected results.",
                provider,
                model
            );
        });
    }
}

static UNKNOWN_MODEL_WARNED: OnceLock<()> = OnceLock::new();

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChatRole {
    System,
    User,
    Assistant,
}

#[async_trait]
pub trait LlmBackend: Send + Sync {
    async fn complete(
        &self,
        system: &str,
        history: &[ChatMessage],
        user_message: &str,
    ) -> Result<String, MclError>;

    fn name(&self) -> &str;

    fn model(&self) -> &str {
        "unknown"
    }

    /// Stream a response token-by-token. Default implementation falls back to
    /// non-streaming `complete()`. Backends can override for real streaming.
    fn stream(&self, system: &str, history: &[ChatMessage], user_message: &str) -> LlmStream {
        let _ = (system, history, user_message);
        Box::pin(stream::once(async {
            Err(MclError::Llm(
                "streaming not supported by this backend".to_string(),
            ))
        }))
    }
}
