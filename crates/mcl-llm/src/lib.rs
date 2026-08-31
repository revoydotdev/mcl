//! LLM backends: Ollama, OpenAI, Anthropic, and Mock.
//!
//! Unified interface for communicating with various LLM providers.

pub mod anthropic;
pub mod backend;
pub mod mock;
pub mod models;
pub mod normalize;
pub mod ollama;
pub mod openai;
pub mod retry;

pub use backend::{ChatMessage, ChatRole, LlmBackend};
pub use mock::MockLlmBackend;
pub use normalize::normalize_response;
