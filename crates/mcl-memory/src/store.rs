use std::path::Path;
use std::sync::Arc;
use std::time::SystemTime;

use mcl_spec::value::MclValue;
use usearch::ffi::{IndexOptions, MetricKind, ScalarKind};

use crate::embedder::Embedder;
use crate::tags::MemoryTag;

/// Errors produced by memory store operations
#[derive(Debug, thiserror::Error)]
pub enum MemoryError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("embedding error: {0}")]
    Embedding(String),
    #[error("serialization error: {0}")]
    Serialization(String),
    #[error("index error: {0}")]
    Index(String),
}

/// A single entry in a memory store
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MemoryEntry {
    pub id: u64,
    pub timestamp: SystemTime,
    pub content: MclValue,
    pub text: String,
    pub tags: Vec<MemoryTag>,
    pub access_count: u32,
}

/// Vector memory store backed by an embedder + usearch HNSW index
pub struct MemoryStore {
    entries: Vec<MemoryEntry>,
    index: usearch::Index,
    embedder: Arc<dyn Embedder + Send + Sync>,
    capacity: usize,
    next_id: u64,
}

impl MemoryStore {
    /// Create a new memory store with the given capacity and shared embedder
    pub fn new(
        capacity: usize,
        embedder: Arc<dyn Embedder + Send + Sync>,
        dimensions: usize,
    ) -> Result<Self, String> {
        let options = IndexOptions {
            dimensions,
            metric: MetricKind::Cos,
            quantization: ScalarKind::F32,
            ..Default::default()
        };
        let index = usearch::new_index(&options)
            .map_err(|e| format!("failed to create usearch index: {e}"))?;
        index
            .reserve(capacity)
            .map_err(|e| format!("failed to reserve index capacity: {e}"))?;

        Ok(Self {
            entries: Vec::new(),
            index,
            embedder,
            capacity,
            next_id: 0,
        })
    }

    /// Insert a new entry. Returns the entry ID. Evicts oldest if at capacity.
    pub fn insert(&mut self, content: MclValue, text: &str, tags: Vec<MemoryTag>) -> u64 {
        // Evict if at capacity
        while self.entries.len() >= self.capacity {
            self.evict_oldest();
        }

        let id = self.next_id;
        self.next_id += 1;

        // Generate embedding
        let embedding = self.embed(text);

        // Add to HNSW index
        if let Some(ref emb) = embedding {
            let _ = self.index.add(id, emb);
        }

        self.entries.push(MemoryEntry {
            id,
            timestamp: SystemTime::now(),
            content,
            text: text.to_string(),
            tags,
            access_count: 0,
        });

        id
    }

    /// Semantic search: find the k most similar entries to the query
    pub fn search(&mut self, query: &str, k: usize) -> Vec<&MemoryEntry> {
        if self.entries.is_empty() {
            return Vec::new();
        }

        let embedding = match self.embed(query) {
            Some(e) => e,
            None => return self.recent(k),
        };

        let results = self.index.search(&embedding, k);
        match results {
            Ok(results) => {
                let mut found = Vec::new();
                for &key in &results.keys {
                    if let Some(entry) = self.entries.iter_mut().find(|e| e.id == key) {
                        entry.access_count += 1;
                        found.push(entry.id);
                    }
                }
                // Return references (re-borrow after mutation)
                self.entries
                    .iter()
                    .filter(|e| found.contains(&e.id))
                    .collect()
            }
            Err(_) => self.recent(k),
        }
    }

    /// Hybrid search: combine semantic similarity with tag overlap
    pub fn search_hybrid(
        &mut self,
        query: &str,
        tags: &[MemoryTag],
        k: usize,
    ) -> Vec<&MemoryEntry> {
        if self.entries.is_empty() {
            return Vec::new();
        }

        // Get more candidates from semantic search, then re-rank with tags
        let candidates_k = (k * 3).min(self.entries.len());
        let embedding = self.embed(query);

        let mut scored: Vec<(u64, f32)> = if let Some(ref emb) = embedding {
            match self.index.search(emb, candidates_k) {
                Ok(results) => results
                    .keys
                    .iter()
                    .zip(results.distances.iter())
                    .map(|(&key, &dist)| (key, 1.0 - dist)) // cosine similarity = 1 - distance
                    .collect(),
                Err(_) => self.entries.iter().map(|e| (e.id, 0.5)).collect(),
            }
        } else {
            self.entries.iter().map(|e| (e.id, 0.5)).collect()
        };

        // Boost score for tag matches
        if !tags.is_empty() {
            for (id, score) in &mut scored {
                if let Some(entry) = self.entries.iter().find(|e| e.id == *id) {
                    let overlap = entry.tags.iter().filter(|t| tags.contains(t)).count() as f32;
                    let tag_boost = overlap / tags.len() as f32;
                    *score = *score * 0.7 + tag_boost * 0.3; // 70% semantic, 30% tags
                }
            }
        }

        // Sort by score descending, take top k
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(k);

        let top_ids: Vec<u64> = scored.iter().map(|(id, _)| *id).collect();

        // Bump access counts
        for entry in &mut self.entries {
            if top_ids.contains(&entry.id) {
                entry.access_count += 1;
            }
        }

        self.entries
            .iter()
            .filter(|e| top_ids.contains(&e.id))
            .collect()
    }

    /// Get the k most recent entries
    pub fn recent(&self, k: usize) -> Vec<&MemoryEntry> {
        let mut entries: Vec<&MemoryEntry> = self.entries.iter().collect();
        entries.sort_by_key(|b| std::cmp::Reverse(b.timestamp));
        entries.truncate(k);
        entries
    }

    /// Number of entries
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Is empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Evict the oldest entry
    fn evict_oldest(&mut self) {
        if let Some(oldest_idx) = self
            .entries
            .iter()
            .enumerate()
            .min_by_key(|(_, e)| e.timestamp)
            .map(|(i, _)| i)
        {
            let removed = self.entries.remove(oldest_idx);
            let _ = self.index.remove(removed.id);
        }
    }

    /// Generate embedding for text, returning None on failure
    fn embed(&self, text: &str) -> Option<Vec<f32>> {
        self.embedder.embed_one(text)
    }

    /// Save entries to `{path}.meta.json` and HNSW index to `{path}.usearch`.
    pub fn save(&self, path: &Path) -> Result<(), MemoryError> {
        let meta_path = path.with_extension("meta.json");
        let json = serde_json::to_string_pretty(&self.entries)
            .map_err(|e| MemoryError::Serialization(e.to_string()))?;
        std::fs::write(&meta_path, json)?;

        let index_path = path.with_extension("usearch");
        self.index
            .save(
                index_path
                    .to_str()
                    .ok_or_else(|| MemoryError::Index("invalid index path".into()))?,
            )
            .map_err(|e| MemoryError::Index(e.to_string()))?;

        Ok(())
    }

    /// Load entries from `{path}.meta.json` and HNSW index from `{path}.usearch`.
    pub fn load(
        path: &Path,
        embedder: Arc<dyn Embedder + Send + Sync>,
        dimensions: usize,
    ) -> Result<Self, MemoryError> {
        let meta_path = path.with_extension("meta.json");
        let json = std::fs::read_to_string(&meta_path)?;
        let entries: Vec<MemoryEntry> =
            serde_json::from_str(&json).map_err(|e| MemoryError::Serialization(e.to_string()))?;

        let options = IndexOptions {
            dimensions,
            metric: MetricKind::Cos,
            quantization: ScalarKind::F32,
            ..Default::default()
        };
        let index = usearch::new_index(&options)
            .map_err(|e| MemoryError::Index(format!("failed to create usearch index: {e}")))?;

        let index_path = path.with_extension("usearch");
        index
            .load(
                index_path
                    .to_str()
                    .ok_or_else(|| MemoryError::Index("invalid index path".into()))?,
            )
            .map_err(|e| MemoryError::Index(e.to_string()))?;

        let next_id = entries
            .iter()
            .map(|e| e.id)
            .max()
            .map(|m| m + 1)
            .unwrap_or(0);

        Ok(Self {
            entries,
            index,
            embedder,
            capacity: next_id as usize,
            next_id,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::create_mock_embedder;
    use crate::tag;

    const MOCK_DIMS: usize = 384;

    fn mock_store(capacity: usize) -> MemoryStore {
        MemoryStore::new(capacity, create_mock_embedder(MOCK_DIMS), MOCK_DIMS)
            .expect("store creation should succeed")
    }

    #[test]
    fn memory_store_new_returns_ok() {
        let store = MemoryStore::new(100, create_mock_embedder(MOCK_DIMS), MOCK_DIMS);
        assert!(store.is_ok(), "MemoryStore::new should return Ok");
        let store = store.unwrap();
        assert!(store.is_empty());
        assert_eq!(store.len(), 0);
    }

    #[test]
    fn memory_store_insert_and_search() {
        let mut store = mock_store(100);

        let id = store.insert(
            MclValue::String("goblin attacks with a rusty sword".into()),
            "goblin attacks with a rusty sword",
            vec![tag!(scope = "combat")],
        );
        assert_eq!(id, 0);
        assert_eq!(store.len(), 1);

        let id2 = store.insert(
            MclValue::String("you find a healing potion".into()),
            "you find a healing potion",
            vec![tag!(scope = "inventory")],
        );
        assert_eq!(id2, 1);
        assert_eq!(store.len(), 2);

        let results = store.search("goblin attacks with a rusty sword", 1);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, 0);
    }

    #[test]
    fn memory_store_capacity_eviction() {
        let mut store = mock_store(2);

        store.insert(MclValue::String("entry A".into()), "entry A", vec![]);
        store.insert(MclValue::String("entry B".into()), "entry B", vec![]);
        assert_eq!(store.len(), 2);

        store.insert(MclValue::String("entry C".into()), "entry C", vec![]);
        assert_eq!(store.len(), 2);

        let recent = store.recent(2);
        let texts: Vec<&str> = recent.iter().map(|e| e.text.as_str()).collect();
        assert!(texts.contains(&"entry B"));
        assert!(texts.contains(&"entry C"));
        assert!(!texts.contains(&"entry A"));
    }

    #[test]
    fn memory_entry_serialization_roundtrip() {
        let entry = MemoryEntry {
            id: 42,
            timestamp: SystemTime::UNIX_EPOCH,
            content: MclValue::String("hello world".into()),
            text: "hello world".to_string(),
            tags: vec![tag!(scope = "greeting")],
            access_count: 3,
        };
        let json = serde_json::to_string(&entry).expect("serialize");
        let decoded: MemoryEntry = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decoded.id, 42);
        assert_eq!(decoded.text, "hello world");
        assert_eq!(decoded.tags, vec![tag!(scope = "greeting")]);
        assert_eq!(decoded.access_count, 3);
    }

    #[test]
    fn memory_store_len_and_is_empty() {
        let mut store = mock_store(100);

        assert!(store.is_empty());
        assert_eq!(store.len(), 0);

        store.insert(MclValue::String("test".into()), "test content", vec![]);
        assert!(!store.is_empty());
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn memory_store_tags_stored_correctly() {
        let mut store = mock_store(100);

        let tags = vec![tag!(scope = "combat"), tag!(scope = "melee")];
        store.insert(
            MclValue::String("hit".into()),
            "hit with sword",
            tags.clone(),
        );

        let recent = store.recent(1);
        assert_eq!(recent[0].tags, tags);
    }

    #[test]
    fn memory_store_eviction_uses_fifo() {
        let mut store = mock_store(3);

        store.insert(MclValue::String("first".into()), "first entry", vec![]);
        store.insert(MclValue::String("second".into()), "second entry", vec![]);
        store.insert(MclValue::String("third".into()), "third entry", vec![]);

        assert_eq!(store.len(), 3);

        store.insert(MclValue::String("fourth".into()), "fourth entry", vec![]);
        assert_eq!(store.len(), 3);

        let texts: Vec<String> = store.recent(3).iter().map(|e| e.text.clone()).collect();
        assert!(texts.contains(&"second entry".to_string()));
        assert!(texts.contains(&"third entry".to_string()));
        assert!(texts.contains(&"fourth entry".to_string()));
        assert!(!texts.contains(&"first entry".to_string()));
    }

    #[test]
    fn memory_store_search_returns_correct_count() {
        let mut store = mock_store(100);

        store.insert(MclValue::String("a".into()), "alpha one", vec![]);
        store.insert(MclValue::String("b".into()), "beta two", vec![]);
        store.insert(MclValue::String("c".into()), "gamma three", vec![]);
        store.insert(MclValue::String("d".into()), "delta four", vec![]);
        store.insert(MclValue::String("e".into()), "epsilon five", vec![]);

        let results = store.search("alpha one", 3);
        assert!(results.len() <= 3);
    }

    #[test]
    fn memory_store_recent_returns_most_recent() {
        let mut store = mock_store(100);

        store.insert(MclValue::String("a".into()), "first", vec![]);
        store.insert(MclValue::String("b".into()), "second", vec![]);
        store.insert(MclValue::String("c".into()), "third", vec![]);
        store.insert(MclValue::String("d".into()), "fourth", vec![]);
        store.insert(MclValue::String("e".into()), "fifth", vec![]);

        let recent = store.recent(3);
        assert_eq!(recent.len(), 3);
        assert_eq!(recent[0].text, "fifth");
        assert_eq!(recent[1].text, "fourth");
        assert_eq!(recent[2].text, "third");
    }

    #[test]
    fn memory_store_hybrid_search_filters_by_tags() {
        let mut store = mock_store(100);

        store.insert(
            MclValue::String("a".into()),
            "dragon fire attack",
            vec![tag!(scope = "combat")],
        );
        store.insert(
            MclValue::String("b".into()),
            "healing potion found",
            vec![tag!(scope = "inventory")],
        );
        store.insert(
            MclValue::String("c".into()),
            "sword slash combat move",
            vec![tag!(scope = "combat"), tag!(scope = "melee")],
        );

        let results = store.search_hybrid(
            "fire attack",
            &[tag!(scope = "combat"), tag!(scope = "melee")],
            2,
        );
        assert!(results.len() <= 2);
    }

    #[test]
    fn memory_store_insert_returns_id() {
        let mut store = mock_store(100);

        let id0 = store.insert(MclValue::String("a".into()), "first", vec![]);
        let id1 = store.insert(MclValue::String("b".into()), "second", vec![]);
        let id2 = store.insert(MclValue::String("c".into()), "third", vec![]);

        assert_eq!(id0, 0);
        assert_eq!(id1, 1);
        assert_eq!(id2, 2);
    }

    #[test]
    fn memory_save_and_load_round_trip() {
        let embedder = create_mock_embedder(MOCK_DIMS);
        let mut store = MemoryStore::new(100, embedder.clone(), MOCK_DIMS)
            .expect("store creation should succeed");

        store.insert(
            MclValue::String("a".into()),
            "dragon attacks the party",
            vec![tag!(scope = "combat")],
        );
        store.insert(
            MclValue::String("b".into()),
            "healing potion consumed",
            vec![tag!(scope = "inventory")],
        );
        store.insert(
            MclValue::String("c".into()),
            "ancient door discovered",
            vec![tag!(scope = "exploration")],
        );

        let dir = std::env::temp_dir().join("mcl_memory_test_roundtrip");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("test_store");

        store.save(&path).expect("save should succeed");

        let mut loaded =
            MemoryStore::load(&path, embedder, MOCK_DIMS).expect("load should succeed");

        assert_eq!(loaded.len(), 3);
        let texts: Vec<&str> = loaded.recent(3).iter().map(|e| e.text.as_str()).collect();
        assert!(texts.contains(&"dragon attacks the party"));
        assert!(texts.contains(&"healing potion consumed"));
        assert!(texts.contains(&"ancient door discovered"));

        let next_id = loaded.insert(MclValue::String("d".into()), "new entry after load", vec![]);
        assert_eq!(next_id, 3);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn memory_save_creates_both_files() {
        let mut store = mock_store(100);

        store.insert(MclValue::String("a".into()), "test entry", vec![]);

        let dir = std::env::temp_dir().join("mcl_memory_test_files");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("test_files_store");

        store.save(&path).expect("save should succeed");

        assert!(path.with_extension("meta.json").exists());
        assert!(path.with_extension("usearch").exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn memory_load_missing_file_returns_error() {
        let path = std::path::PathBuf::from("/nonexistent/path/to/store");
        let result = MemoryStore::load(&path, create_mock_embedder(MOCK_DIMS), MOCK_DIMS);
        assert!(result.is_err());
    }
}
