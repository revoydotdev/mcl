use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::embedder::Embedder;
use crate::store::MemoryStore;

/// The three-store memory model for a node.
///
/// - `incoming`: populated from upstream packets via chunker
/// - `outgoing`: auto-populated when the node emits packets
/// - `local`: node-internal persistent scratchpad
/// - `persist_path`: optional path for automatic persistence
pub struct NodeMemory {
    pub incoming: MemoryStore,
    pub outgoing: MemoryStore,
    pub local: MemoryStore,
    pub persist_path: Option<PathBuf>,
}

impl NodeMemory {
    pub fn new(
        capacity: usize,
        embedder: Arc<dyn Embedder + Send + Sync>,
        dimensions: usize,
    ) -> Result<Self, String> {
        Ok(Self {
            incoming: MemoryStore::new(capacity, embedder.clone(), dimensions)?,
            outgoing: MemoryStore::new(capacity, embedder.clone(), dimensions)?,
            local: MemoryStore::new(capacity, embedder, dimensions)?,
            persist_path: None,
        })
    }

    /// Save all three stores to the given path prefix.
    /// Each store saves to `{path}-{store_name}.meta.json` and `{path}-{store_name}.usearch`.
    pub fn save_to(&self, path: &Path) -> Result<(), String> {
        let incoming_path = format!("{}-incoming", path.display());
        self.incoming
            .save(PathBuf::from(&incoming_path).as_path())
            .map_err(|e| format!("failed to save incoming store: {e}"))?;
        let outgoing_path = format!("{}-outgoing", path.display());
        self.outgoing
            .save(PathBuf::from(&outgoing_path).as_path())
            .map_err(|e| format!("failed to save outgoing store: {e}"))?;
        let local_path = format!("{}-local", path.display());
        self.local
            .save(PathBuf::from(&local_path).as_path())
            .map_err(|e| format!("failed to save local store: {e}"))?;
        Ok(())
    }

    /// Load all three stores from the given path prefix.
    /// Each store loads from `{path}-{store_name}.meta.json` and `{path}-{store_name}.usearch`.
    pub fn load_from(
        &mut self,
        path: &Path,
        embedder: Arc<dyn Embedder + Send + Sync>,
        dimensions: usize,
    ) -> Result<(), String> {
        let incoming_path = format!("{}-incoming", path.display());
        self.incoming = MemoryStore::load(
            PathBuf::from(&incoming_path).as_path(),
            embedder.clone(),
            dimensions,
        )
        .map_err(|e| format!("failed to load incoming store: {e}"))?;
        let outgoing_path = format!("{}-outgoing", path.display());
        self.outgoing = MemoryStore::load(
            PathBuf::from(&outgoing_path).as_path(),
            embedder.clone(),
            dimensions,
        )
        .map_err(|e| format!("failed to load outgoing store: {e}"))?;
        let local_path = format!("{}-local", path.display());
        self.local = MemoryStore::load(PathBuf::from(&local_path).as_path(), embedder, dimensions)
            .map_err(|e| format!("failed to load local store: {e}"))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::create_mock_embedder;
    use crate::tag;
    use mcl_spec::value::MclValue;

    const MOCK_DIMS: usize = 384;

    #[test]
    fn node_memory_create_with_mock_embedder() {
        let embedder = create_mock_embedder(MOCK_DIMS);
        let node_mem = NodeMemory::new(100, embedder, MOCK_DIMS);
        assert!(node_mem.is_ok());
    }

    #[test]
    fn node_memory_three_stores_are_independent() {
        let embedder = create_mock_embedder(MOCK_DIMS);
        let mut node_mem =
            NodeMemory::new(100, embedder, MOCK_DIMS).expect("node memory creation should succeed");

        node_mem.incoming.insert(
            MclValue::String("incoming data".into()),
            "incoming packet",
            vec![tag!(custom = "incoming_tag")],
        );
        node_mem.outgoing.insert(
            MclValue::String("outgoing data".into()),
            "outgoing packet",
            vec![tag!(custom = "outgoing_tag")],
        );
        node_mem.local.insert(
            MclValue::String("local data".into()),
            "local note",
            vec![tag!(custom = "local_tag")],
        );

        assert_eq!(node_mem.incoming.len(), 1);
        assert_eq!(node_mem.outgoing.len(), 1);
        assert_eq!(node_mem.local.len(), 1);

        assert_eq!(node_mem.incoming.recent(1)[0].text, "incoming packet");
        assert_eq!(node_mem.outgoing.recent(1)[0].text, "outgoing packet");
        assert_eq!(node_mem.local.recent(1)[0].text, "local note");
    }

    #[test]
    fn node_memory_save_load_round_trip() {
        let embedder = create_mock_embedder(MOCK_DIMS);
        let mut node_mem = NodeMemory::new(100, embedder.clone(), MOCK_DIMS)
            .expect("node memory creation should succeed");

        node_mem.incoming.insert(
            MclValue::String("incoming data".into()),
            "incoming packet",
            vec![tag!(custom = "incoming_tag")],
        );
        node_mem.outgoing.insert(
            MclValue::String("outgoing data".into()),
            "outgoing packet",
            vec![tag!(custom = "outgoing_tag")],
        );
        node_mem.local.insert(
            MclValue::String("local data".into()),
            "local note",
            vec![tag!(custom = "local_tag")],
        );

        let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
        let save_path = temp_dir.path().join("node_memory");
        node_mem
            .save_to(&save_path)
            .expect("failed to save node memory");

        let mut loaded_mem = NodeMemory::new(100, embedder.clone(), MOCK_DIMS)
            .expect("node memory creation should succeed");
        loaded_mem
            .load_from(&save_path, embedder, MOCK_DIMS)
            .expect("failed to load node memory");

        assert_eq!(loaded_mem.incoming.len(), 1);
        assert_eq!(loaded_mem.outgoing.len(), 1);
        assert_eq!(loaded_mem.local.len(), 1);

        assert_eq!(loaded_mem.incoming.recent(1)[0].text, "incoming packet");
        assert_eq!(loaded_mem.outgoing.recent(1)[0].text, "outgoing packet");
        assert_eq!(loaded_mem.local.recent(1)[0].text, "local note");

        assert_eq!(
            loaded_mem.incoming.recent(1)[0].tags,
            vec![tag!(custom = "incoming_tag")]
        );
        assert_eq!(
            loaded_mem.outgoing.recent(1)[0].tags,
            vec![tag!(custom = "outgoing_tag")]
        );
        assert_eq!(
            loaded_mem.local.recent(1)[0].tags,
            vec![tag!(custom = "local_tag")]
        );
    }
}
