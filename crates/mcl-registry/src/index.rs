//! Semantic vector index for signal registry queries.

#![warn(clippy::unwrap_used)]

use std::collections::HashMap;
use std::sync::Arc;

use mcl_memory::{create_mock_embedder, Embedder, MemoryStore, MemoryTag};
use mcl_spec::signal::SignalKind;
use mcl_spec::value::MclValue;

use crate::types::{SignalId, SignalLogEntry};

const DEFAULT_DIMS: usize = 384;
const DEFAULT_CAPACITY: usize = 10_000;

pub struct RegistryIndex {
    store: MemoryStore,
    entry_id_to_signal: HashMap<u64, SignalId>,
    ingested: HashMap<SignalId, u64>,
}

impl RegistryIndex {
    pub fn new() -> Result<Self, String> {
        Self::with_embedder(create_mock_embedder(DEFAULT_DIMS), DEFAULT_CAPACITY)
    }

    pub fn with_embedder(
        embedder: Arc<dyn Embedder + Send + Sync>,
        capacity: usize,
    ) -> Result<Self, String> {
        let store = MemoryStore::new(capacity, embedder, DEFAULT_DIMS)?;
        Ok(Self {
            store,
            entry_id_to_signal: HashMap::new(),
            ingested: HashMap::new(),
        })
    }

    pub fn ingest(&mut self, entry: &SignalLogEntry) {
        if self.ingested.contains_key(&entry.id) {
            return;
        }

        let text = signal_to_text(entry);
        let tags = vec![MemoryTag::SignalId(entry.id.hex())];
        let content = MclValue::String(text.clone());

        let entry_id = self.store.insert(content, &text, tags);
        self.entry_id_to_signal.insert(entry_id, entry.id.clone());
        self.ingested.insert(entry.id.clone(), entry_id);
    }

    pub fn query_semantic(&mut self, text: &str, k: usize) -> Vec<(SignalId, f32)> {
        let results = self.store.search(text, k);
        if results.is_empty() {
            return Vec::new();
        }

        results
            .into_iter()
            .filter_map(|entry| {
                let signal_id = self.entry_id_to_signal.get(&entry.id)?.clone();
                let score = cosine_similarity_text(&entry.text, text);
                Some((signal_id, score))
            })
            .collect()
    }

    pub fn len(&self) -> usize {
        self.store.len()
    }

    pub fn is_empty(&self) -> bool {
        self.store.is_empty()
    }
}

impl Default for RegistryIndex {
    fn default() -> Self {
        Self::new().expect("RegistryIndex::new should not fail with MockEmbedder")
    }
}

fn signal_to_text(entry: &SignalLogEntry) -> String {
    let kind_name = match &entry.signal.kind {
        SignalKind::Action { .. } => "Action",
        SignalKind::Mutate { .. } => "Mutate",
        SignalKind::Transition { .. } => "Transition",
        SignalKind::Dialogue { .. } => "Dialogue",
        SignalKind::Narrate { .. } => "Narrate",
        SignalKind::Query { .. } => "Query",
        SignalKind::Error { .. } => "Error",
        SignalKind::Noop => "Noop",
    };
    format!("{kind_name}:{}", entry.signal)
}

fn cosine_similarity_text(a: &str, b: &str) -> f32 {
    if a == b {
        return 1.0;
    }
    let shared = a
        .chars()
        .zip(b.chars())
        .take_while(|(ca, cb)| ca == cb)
        .count();
    let max_len = a.len().max(b.len());
    if max_len == 0 {
        return 1.0;
    }
    let prefix_sim = shared as f32 / max_len as f32;
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    a.hash(&mut hasher);
    b.hash(&mut hasher);
    let hash_component = (hasher.finish() % 100) as f32 / 1000.0;
    (prefix_sim + hash_component).min(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mcl_graph::id::NodeId;
    use mcl_spec::label::LabelSet;
    use mcl_spec::span::Span;

    fn make_entry(origin: &str, turn: u64, channel: &str, name: &str) -> SignalLogEntry {
        let signal = mcl_spec::signal::MclSignal {
            kind: SignalKind::Action {
                name: name.to_string(),
                args: Default::default(),
            },
            span: Span::new(0, 1, 1, 1),
            labels: LabelSet::default(),
        };
        SignalLogEntry {
            id: SignalId::from_signal(&NodeId::new(origin), turn, channel, &signal),
            signal,
            origin: NodeId::new(origin),
            turn,
            channel: channel.to_string(),
            timestamp: 1714000000000 + turn,
            verdict: crate::types::Verdict::Accept,
            policy_bindings: None,
        }
    }

    #[test]
    fn ingest_and_query_returns_results() {
        let mut index = RegistryIndex::new().expect("new");

        let entry1 = make_entry("player", 1, "actions", "attack");
        let entry2 = make_entry("player", 2, "actions", "defend");
        let entry3 = make_entry("player", 3, "actions", "heal");

        index.ingest(&entry1);
        index.ingest(&entry2);
        index.ingest(&entry3);

        assert_eq!(index.len(), 3);

        let results = index.query_semantic("attack", 2);
        assert!(!results.is_empty(), "query should return results");
        assert!(results.len() <= 2, "should return at most k results");

        let all_ids = [entry1.id.clone(), entry2.id.clone(), entry3.id.clone()];
        for (id, _score) in &results {
            assert!(all_ids.contains(id));
        }
    }

    #[test]
    fn mock_embedder_is_deterministic() {
        let mut index1 = RegistryIndex::new().expect("new");
        let mut index2 = RegistryIndex::new().expect("new");

        let entry = make_entry("player", 1, "actions", "attack");
        index1.ingest(&entry);
        index2.ingest(&entry);

        let results1 = index1.query_semantic("attack", 1);
        let results2 = index2.query_semantic("attack", 1);

        assert_eq!(results1.len(), results2.len());
        if let (Some((id1, s1)), Some((id2, s2))) = (results1.first(), results2.first()) {
            assert_eq!(id1, id2);
            assert!(
                (s1 - s2).abs() < f32::EPSILON,
                "scores should be identical for deterministic embedder"
            );
        }
    }

    #[test]
    fn empty_index_returns_empty_results() {
        let mut index = RegistryIndex::new().expect("new");
        assert!(index.is_empty());

        let results = index.query_semantic("anything", 5);
        assert!(results.is_empty());
    }

    #[test]
    fn duplicate_ingest_is_ignored() {
        let mut index = RegistryIndex::new().expect("new");

        let entry = make_entry("player", 1, "actions", "attack");
        index.ingest(&entry);
        index.ingest(&entry);

        assert_eq!(index.len(), 1, "duplicate signal should not be re-ingested");
    }

    #[test]
    fn query_k_limits_results() {
        let mut index = RegistryIndex::new().expect("new");

        for i in 0..5 {
            index.ingest(&make_entry("player", i, "actions", &format!("action_{i}")));
        }

        let results = index.query_semantic("action", 2);
        assert!(results.len() <= 2);
    }

    #[test]
    fn signal_to_text_includes_kind_and_display() {
        let entry = make_entry("player", 1, "actions", "attack");
        let text = signal_to_text(&entry);
        assert!(
            text.starts_with("Action:"),
            "text should start with kind name"
        );
        assert!(text.contains("attack"), "text should contain action name");
    }
}
