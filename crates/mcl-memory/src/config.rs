use fastembed::EmbeddingModel;

/// Configuration for the embedding model used by memory stores
#[derive(Debug, Clone)]
pub struct EmbeddingConfig {
    /// Which fastembed model to use
    pub model: EmbeddingModel,
    /// Optional Matryoshka dimension truncation (e.g., 384 from 768)
    pub dimensions: Option<usize>,
    /// Batch size for embedding generation
    pub batch_size: usize,
    /// Show model download progress
    pub show_download_progress: bool,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            model: EmbeddingModel::AllMiniLML6V2, // 384d, fast, good quality
            dimensions: None,
            batch_size: 32,
            show_download_progress: true,
        }
    }
}

impl EmbeddingConfig {
    /// Use nomic-embed-text-v1.5 (768d, 8K context, better quality)
    pub fn nomic() -> Self {
        Self {
            model: EmbeddingModel::NomicEmbedTextV15,
            ..Default::default()
        }
    }

    /// Get the output dimensionality
    pub fn output_dims(&self) -> usize {
        self.dimensions.unwrap_or(match self.model {
            EmbeddingModel::AllMiniLML6V2 => 384,
            EmbeddingModel::NomicEmbedTextV15 => 768,
            _ => 384, // safe default
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_uses_allminilm() {
        let config = EmbeddingConfig::default();
        assert!(matches!(config.model, EmbeddingModel::AllMiniLML6V2));
    }

    #[test]
    fn nomic_config_uses_correct_dimensions() {
        let config = EmbeddingConfig::nomic();
        assert_eq!(config.output_dims(), 768);
    }

    #[test]
    fn config_output_dims_defaults_to_model_dims() {
        let config = EmbeddingConfig::default();
        assert!(config.dimensions.is_none());
        assert_eq!(config.output_dims(), 384);
    }
}
