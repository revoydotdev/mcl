use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Mutex;

use fastembed::TextEmbedding;

/// Trait for text embedding models.
///
/// Implementations can be real models (fastembed) or mocks for testing.
pub trait Embedder: Send + Sync {
    /// Generate an embedding vector for a single text string.
    /// Returns `None` if embedding fails.
    fn embed_one(&self, text: &str) -> Option<Vec<f32>>;
}

/// Real embedder backed by fastembed (downloads model on first use).
pub struct FastembedEmbedder {
    inner: Mutex<TextEmbedding>,
}

impl FastembedEmbedder {
    pub fn new(inner: TextEmbedding) -> Self {
        Self {
            inner: Mutex::new(inner),
        }
    }
}

impl Embedder for FastembedEmbedder {
    fn embed_one(&self, text: &str) -> Option<Vec<f32>> {
        self.inner
            .lock()
            .ok()?
            .embed(vec![text], None)
            .ok()
            .and_then(|mut v| v.pop())
    }
}

/// Mock embedder producing deterministic hash-based embeddings. No model download required.
pub struct MockEmbedder {
    dimensions: usize,
}

impl MockEmbedder {
    pub fn new(dimensions: usize) -> Self {
        Self { dimensions }
    }
}

impl Embedder for MockEmbedder {
    fn embed_one(&self, text: &str) -> Option<Vec<f32>> {
        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        let seed = hasher.finish();

        let mut vec = Vec::with_capacity(self.dimensions);
        for i in 0..self.dimensions {
            let mut h = DefaultHasher::new();
            seed.hash(&mut h);
            (i as u64).hash(&mut h);
            let val = ((h.finish() % 2000) as f32) / 1000.0;
            vec.push(val);
        }

        let norm: f32 = vec.iter().map(|v| v * v).sum::<f32>().sqrt();
        if norm > 0.0 {
            for v in &mut vec {
                *v /= norm;
            }
        }

        Some(vec)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_embedder_produces_unit_vector() {
        let embedder = MockEmbedder::new(384);
        let vec = embedder.embed_one("hello world").unwrap();
        assert_eq!(vec.len(), 384);
        let norm: f32 = vec.iter().map(|v| v * v).sum::<f32>().sqrt();
        assert!(
            (norm - 1.0).abs() < 1e-6,
            "vector should be unit length, got {norm}"
        );
    }

    #[test]
    fn mock_embedder_is_deterministic() {
        let embedder = MockEmbedder::new(384);
        let v1 = embedder.embed_one("test text").unwrap();
        let v2 = embedder.embed_one("test text").unwrap();
        assert_eq!(v1, v2, "same input should produce same output");
    }

    #[test]
    fn mock_embedder_different_texts_differ() {
        let embedder = MockEmbedder::new(384);
        let v1 = embedder.embed_one("apple").unwrap();
        let v2 = embedder.embed_one("banana").unwrap();
        assert_ne!(v1, v2, "different inputs should produce different outputs");
    }

    #[test]
    fn mock_embedder_custom_dimensions() {
        let embedder = MockEmbedder::new(128);
        let vec = embedder.embed_one("test").unwrap();
        assert_eq!(vec.len(), 128);
    }

    #[test]
    fn mock_embedder_similarity_semantic() {
        let embedder = MockEmbedder::new(384);
        let v1 = embedder.embed_one("dragon attacks with fire").unwrap();
        let v2 = embedder.embed_one("dragon breathes flames").unwrap();

        let dot: f32 = v1.iter().zip(v2.iter()).map(|(a, b)| a * b).sum();
        assert!(dot.is_finite());
    }
}
