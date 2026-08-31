//! Vector memory: fastembed + usearch with 3-store model.
//!
//! Provides semantic search and retrieval for conversation history.

pub mod chunker;
pub mod config;
pub mod embedder;
pub mod node_memory;
pub mod store;
pub mod tags;

pub use chunker::{ChunkResult, Chunker, ParagraphChunker, SignalChunker, WholePacketChunker};
pub use config::EmbeddingConfig;
pub use embedder::{Embedder, FastembedEmbedder, MockEmbedder};
pub use node_memory::NodeMemory;
pub use store::{MemoryEntry, MemoryError, MemoryStore};
pub use tags::MemoryTag;

use fastembed::{InitOptions, TextEmbedding};
use std::sync::Arc;

const MCL_MOCK_EMBEDDING: &str = "MCL_MOCK_EMBEDDING";

pub fn create_embedder(
    config: &EmbeddingConfig,
) -> Result<Arc<dyn Embedder + Send + Sync>, String> {
    if std::env::var(MCL_MOCK_EMBEDDING).is_ok_and(|v| v == "1" || v.eq_ignore_ascii_case("true")) {
        return Ok(create_mock_embedder(config.output_dims()));
    }

    let options = InitOptions::new(config.model.clone())
        .with_show_download_progress(config.show_download_progress);

    let model = TextEmbedding::try_new(options)
        .map_err(|e| format!("failed to initialize embedding model: {e}"))?;

    Ok(Arc::new(FastembedEmbedder::new(model)))
}

pub fn create_mock_embedder(dimensions: usize) -> Arc<dyn Embedder + Send + Sync> {
    Arc::new(MockEmbedder::new(dimensions))
}
