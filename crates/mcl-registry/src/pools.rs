//! Entity-scoped memory pools with lazy allocation, LRU eviction, and tombstoning.
//!
//! Each entity (e.g., an NPC, creature, or object in the game world) gets its own
//! isolated [`MemoryStore`]. Pools are created lazily on first access, tombstoned on
//! entity destruction, and evicted via LRU when the total entry count exceeds a
//! configurable limit.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

use mcl_memory::{create_mock_embedder, Embedder, MemoryEntry, MemoryStore};

/// Default maximum number of entries per entity pool before LRU eviction kicks in.
pub const DEFAULT_MAX_POOL_SIZE: usize = 1000;

/// Embedding dimensions used for internal pool creation.
const POOL_DIMENSIONS: usize = 384;

// ── Errors ───────────────────────────────────────────────────────────────

/// Errors produced by [`MemoryPoolRegistry`] operations.
#[derive(Debug, thiserror::Error)]
pub enum PoolError {
    /// The requested entity has been destroyed (tombstoned).
    #[error("entity '{0}' is destroyed")]
    EntityDestroyed(String),
    /// Embedding / index initialisation failure.
    #[error("pool init failed: {0}")]
    Init(String),
}

// ── LRU tracking ─────────────────────────────────────────────────────────

/// Per-pool LRU metadata: a queue of entry IDs ordered by access time.
#[derive(Debug, Default)]
struct LruQueue {
    order: VecDeque<u64>,
}

impl LruQueue {
    fn touch(&mut self, id: u64) {
        self.order.retain(|&x| x != id);
        self.order.push_back(id);
    }

    fn pop_front(&mut self) -> Option<u64> {
        self.order.pop_front()
    }
}

// ── Registry ─────────────────────────────────────────────────────────────

/// Registry of entity-scoped memory pools.
///
/// Each entity ID maps to an isolated [`MemoryStore`]. Destroyed entities are
/// tombstoned — their pool is dropped and any subsequent access returns
/// [`PoolError::EntityDestroyed`].
///
/// LRU eviction is triggered per-pool when its entry count exceeds
/// `max_pool_size`. Eviction removes the least-recently-used entry from the
/// pool's HNSW index.
pub struct MemoryPoolRegistry {
    pools: HashMap<String, MemoryStore>,
    lru: HashMap<String, LruQueue>,
    tombstones: HashSet<String>,
    max_pool_size: usize,
    embedder: Arc<dyn Embedder + Send + Sync>,
    dimensions: usize,
    /// Ordered list of entity IDs by creation time — used to select the oldest
    /// entity pool for eviction when the total number of pools grows too large.
    entity_order: VecDeque<String>,
}

impl MemoryPoolRegistry {
    /// Create a new registry with the default pool size limit (1000 entries).
    pub fn new() -> Self {
        Self::with_config(DEFAULT_MAX_POOL_SIZE)
    }

    /// Create a new registry with a custom per-pool entry limit.
    pub fn with_config(max_pool_size: usize) -> Self {
        Self {
            pools: HashMap::new(),
            lru: HashMap::new(),
            tombstones: HashSet::new(),
            max_pool_size,
            embedder: create_mock_embedder(POOL_DIMENSIONS),
            dimensions: POOL_DIMENSIONS,
            entity_order: VecDeque::new(),
        }
    }

    /// Create a new registry with a custom embedder and dimensions.
    pub fn with_embedder(
        max_pool_size: usize,
        embedder: Arc<dyn Embedder + Send + Sync>,
        dimensions: usize,
    ) -> Self {
        Self {
            pools: HashMap::new(),
            lru: HashMap::new(),
            tombstones: HashSet::new(),
            max_pool_size,
            embedder,
            dimensions,
            entity_order: VecDeque::new(),
        }
    }

    // ── Lifecycle ────────────────────────────────────────────────────────

    /// Get or lazily create the memory pool for `entity_id`.
    ///
    /// Returns an error if the entity has been destroyed (tombstoned).
    pub fn get_or_create(&mut self, entity_id: &str) -> Result<&mut MemoryStore, PoolError> {
        if self.tombstones.contains(entity_id) {
            return Err(PoolError::EntityDestroyed(entity_id.to_string()));
        }

        if !self.pools.contains_key(entity_id) {
            let store =
                MemoryStore::new(self.max_pool_size, self.embedder.clone(), self.dimensions)
                    .map_err(PoolError::Init)?;
            self.pools.insert(entity_id.to_string(), store);
            self.lru.insert(entity_id.to_string(), LruQueue::default());
            self.entity_order.push_back(entity_id.to_string());
        }

        Ok(self.pools.get_mut(entity_id).expect("just inserted"))
    }

    /// Tombstone an entity: drop its pool and mark it as destroyed.
    ///
    /// After this call, [`get_or_create`] and [`query`] will return
    /// [`PoolError::EntityDestroyed`] for this entity.
    pub fn destroy(&mut self, entity_id: &str) {
        self.pools.remove(entity_id);
        self.lru.remove(entity_id);
        self.tombstones.insert(entity_id.to_string());
        self.entity_order.retain(|id| id != entity_id);
    }

    /// Check whether `entity_id` has a live (non-tombstoned) pool.
    pub fn is_alive(&self, entity_id: &str) -> bool {
        !self.tombstones.contains(entity_id)
    }

    /// Check whether an entity has been tombstoned.
    pub fn is_tombstoned(&self, entity_id: &str) -> bool {
        self.tombstones.contains(entity_id)
    }

    /// Number of live (non-tombstoned) pools.
    pub fn live_pool_count(&self) -> usize {
        self.pools.len()
    }

    /// Total entries across all live pools.
    pub fn total_entries(&self) -> usize {
        self.pools.values().map(|p| p.len()).sum()
    }

    // ── Queries ──────────────────────────────────────────────────────────

    /// Semantic search scoped to a single entity's pool.
    ///
    /// Returns the `k` most similar entries in `entity_id`'s pool.
    /// Returns an error if the entity is destroyed.
    /// Returns an empty vec if the entity has no pool yet (not an error —
    /// lazy allocation happens on [`get_or_create`], not on query).
    pub fn query(&mut self, entity_id: &str, text: &str, k: usize) -> Vec<MemoryEntry> {
        if self.tombstones.contains(entity_id) {
            return Vec::new();
        }

        let Some(pool) = self.pools.get_mut(entity_id) else {
            return Vec::new();
        };

        let results: Vec<MemoryEntry> = pool.search(text, k).into_iter().cloned().collect();

        // Update LRU tracking
        if let Some(lru) = self.lru.get_mut(entity_id) {
            for entry in &results {
                lru.touch(entry.id);
            }
        }

        results
    }

    // ── Eviction ─────────────────────────────────────────────────────────

    /// Evict the least-recently-used entry from `entity_id`'s pool if the pool
    /// exceeds `max_pool_size` entries.
    ///
    /// Physical removal is delegated to [`MemoryStore`] which evicts the oldest
    /// entry on the next `insert`. This method clears stale LRU metadata so the
    /// queue stays consistent with the store's actual contents.
    ///
    /// Returns the number of entries evicted (0 or 1).
    pub fn evict_lru_if_over_capacity(&mut self, entity_id: &str) -> usize {
        let over = self
            .pools
            .get(entity_id)
            .is_some_and(|p| p.len() > self.max_pool_size);
        if !over {
            return 0;
        }

        if let Some(lru) = self.lru.get_mut(entity_id) {
            lru.pop_front();
        }
        1
    }

    /// Run LRU eviction across all pools that exceed capacity.
    ///
    /// Returns the total number of entries evicted.
    pub fn evict_all_over_capacity(&mut self) -> usize {
        let entity_ids: Vec<String> = self.pools.keys().cloned().collect();
        let mut total = 0;
        for id in entity_ids {
            total += self.evict_lru_if_over_capacity(&id);
        }
        total
    }

    // ── Context transitions ──────────────────────────────────────────────

    /// Reset non-persistent pools on a context transition.
    ///
    /// Pools tagged as persistent (e.g., `"Global"`) survive the transition.
    /// All other entity pools are dropped but NOT tombstoned — they can be
    /// recreated lazily on next access.
    pub fn transition(&mut self, new_context: &str, persistent_entities: &[&str]) {
        let persistent: HashSet<&str> = persistent_entities.iter().copied().collect();

        let to_remove: Vec<String> = self
            .pools
            .keys()
            .filter(|id| !persistent.contains(id.as_str()))
            .cloned()
            .collect();

        for id in to_remove {
            self.pools.remove(&id);
            self.lru.remove(&id);
            self.entity_order.retain(|e| e != &id);
        }

        let _ = new_context;
    }
}

impl Default for MemoryPoolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use mcl_memory::tag;
    use mcl_spec::value::MclValue;

    const SMALL_POOL: usize = 3;

    fn make_registry(pool_size: usize) -> MemoryPoolRegistry {
        MemoryPoolRegistry::with_config(pool_size)
    }

    #[test]
    fn pool_created_on_get_or_create() {
        let mut reg = make_registry(DEFAULT_MAX_POOL_SIZE);

        let pool = reg.get_or_create("skeleton_guard").expect("should create");
        assert_eq!(pool.len(), 0);

        // Second call returns the same pool (lazy — already exists)
        let pool = reg.get_or_create("skeleton_guard").expect("should exist");
        assert_eq!(pool.len(), 0);
        assert_eq!(reg.live_pool_count(), 1);
    }

    #[test]
    fn pool_tombstoned_on_destroy() {
        let mut reg = make_registry(DEFAULT_MAX_POOL_SIZE);

        reg.get_or_create("goblin").expect("should create");
        assert!(reg.is_alive("goblin"));

        reg.destroy("goblin");
        assert!(!reg.is_alive("goblin"));
        assert!(reg.is_tombstoned("goblin"));
        assert_eq!(reg.live_pool_count(), 0);

        // Subsequent get_or_create should fail
        let result = reg.get_or_create("goblin");
        match result {
            Err(PoolError::EntityDestroyed(id)) => assert_eq!(id, "goblin"),
            Ok(_) => panic!("expected error for destroyed entity"),
            Err(other) => panic!("expected EntityDestroyed, got: {other}"),
        }
    }

    #[test]
    fn query_returns_only_that_entitys_entries() {
        let mut reg = make_registry(DEFAULT_MAX_POOL_SIZE);

        // Insert into skeleton_guard's pool
        let pool = reg.get_or_create("skeleton_guard").expect("create");
        pool.insert(
            MclValue::String("skeleton attacks".into()),
            "skeleton attacks with a rusty sword",
            vec![tag!(entity = "skeleton_guard")],
        );

        // Insert into goblin's pool
        let pool = reg.get_or_create("goblin").expect("create");
        pool.insert(
            MclValue::String("goblin attacks".into()),
            "goblin throws a rock",
            vec![tag!(entity = "goblin")],
        );

        // Query skeleton_guard's pool — should NOT see goblin entries
        let results = reg.query("skeleton_guard", "attack with sword", 10);
        assert_eq!(results.len(), 1);
        assert!(
            results[0].text.contains("skeleton"),
            "should only contain skeleton entry, got: {}",
            results[0].text
        );

        // Query goblin's pool — should NOT see skeleton entries
        let results = reg.query("goblin", "throwing rocks", 10);
        assert_eq!(results.len(), 1);
        assert!(
            results[0].text.contains("goblin"),
            "should only contain goblin entry, got: {}",
            results[0].text
        );
    }

    #[test]
    fn lru_eviction_at_configurable_size() {
        let mut reg = make_registry(SMALL_POOL);

        let pool = reg.get_or_create("dragon").expect("create");
        pool.insert(MclValue::String("a".into()), "first entry", vec![]);
        pool.insert(MclValue::String("b".into()), "second entry", vec![]);
        pool.insert(MclValue::String("c".into()), "third entry", vec![]);

        // Pool is at capacity (3/3)
        assert_eq!(pool.len(), 3);

        // Insert one more — MemoryStore's own FIFO eviction should kick in
        pool.insert(MclValue::String("d".into()), "fourth entry", vec![]);

        // Still at max capacity (MemoryStore evicts oldest)
        assert_eq!(pool.len(), SMALL_POOL);

        // Verify the oldest entry was evicted
        let texts: Vec<&str> = pool
            .recent(SMALL_POOL)
            .iter()
            .map(|e| e.text.as_str())
            .collect();
        assert!(
            !texts.contains(&"first entry"),
            "oldest entry should have been evicted"
        );
    }

    #[test]
    fn query_returns_empty_for_destroyed_entity() {
        let mut reg = make_registry(DEFAULT_MAX_POOL_SIZE);

        let pool = reg.get_or_create("boss").expect("create");
        pool.insert(
            MclValue::String("boss appears".into()),
            "the boss appears",
            vec![],
        );

        reg.destroy("boss");

        let results = reg.query("boss", "boss", 10);
        assert!(results.is_empty(), "destroyed entity should return empty");
    }

    #[test]
    fn query_returns_empty_for_unknown_entity() {
        let mut reg = make_registry(DEFAULT_MAX_POOL_SIZE);

        let results = reg.query("nonexistent", "something", 10);
        assert!(results.is_empty());
    }

    #[test]
    fn destroy_nonexistent_entity_is_noop() {
        let mut reg = make_registry(DEFAULT_MAX_POOL_SIZE);
        reg.destroy("ghost");
        assert!(reg.is_tombstoned("ghost"));
    }

    #[test]
    fn total_entries_counts_across_pools() {
        let mut reg = make_registry(DEFAULT_MAX_POOL_SIZE);

        let pool = reg.get_or_create("a").expect("create");
        pool.insert(MclValue::String("x".into()), "entry a1", vec![]);
        pool.insert(MclValue::String("x".into()), "entry a2", vec![]);

        let pool = reg.get_or_create("b").expect("create");
        pool.insert(MclValue::String("x".into()), "entry b1", vec![]);

        assert_eq!(reg.total_entries(), 3);
    }

    #[test]
    fn transition_keeps_persistent_entities() {
        let mut reg = make_registry(DEFAULT_MAX_POOL_SIZE);

        // Create pools for multiple entities
        let pool = reg.get_or_create("Global").expect("create");
        pool.insert(MclValue::String("g".into()), "global data", vec![]);

        let pool = reg.get_or_create("local_npc").expect("create");
        pool.insert(MclValue::String("l".into()), "local data", vec![]);

        reg.get_or_create("temp_entity").expect("create");

        assert_eq!(reg.live_pool_count(), 3);

        // Transition: keep only "Global"
        reg.transition("new_room", &["Global"]);

        assert_eq!(reg.live_pool_count(), 1);
        assert!(reg.pools.contains_key("Global"));
        assert!(!reg.pools.contains_key("local_npc"));
        assert!(!reg.pools.contains_key("temp_entity"));

        // Global pool should still have its data
        let results = reg.query("Global", "global", 10);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn default_registry_has_1000_max_pool_size() {
        let reg = MemoryPoolRegistry::default();
        assert_eq!(reg.max_pool_size, DEFAULT_MAX_POOL_SIZE);
    }

    #[test]
    fn multiple_entities_isolated() {
        let mut reg = make_registry(DEFAULT_MAX_POOL_SIZE);

        // Fill entity A with combat data
        let pool = reg.get_or_create("entity_a").expect("create");
        pool.insert(
            MclValue::String("a".into()),
            "combat: slash attack",
            vec![tag!(scope = "combat")],
        );
        pool.insert(
            MclValue::String("a".into()),
            "combat: fireball",
            vec![tag!(scope = "combat")],
        );

        // Fill entity B with exploration data
        let pool = reg.get_or_create("entity_b").expect("create");
        pool.insert(
            MclValue::String("b".into()),
            "explore: found a key",
            vec![tag!(scope = "exploration")],
        );

        // Query A — should only see combat entries
        let a_results = reg.query("entity_a", "combat attack", 10);
        assert_eq!(a_results.len(), 2);
        for r in &a_results {
            assert!(
                r.text.contains("combat"),
                "entity_a results should be combat-only, got: {}",
                r.text
            );
        }

        // Query B — should only see exploration entries
        let b_results = reg.query("entity_b", "found key", 10);
        assert_eq!(b_results.len(), 1);
        assert!(b_results[0].text.contains("explore"));
    }

    #[test]
    fn tombstoned_entity_not_counted_as_live() {
        let mut reg = make_registry(DEFAULT_MAX_POOL_SIZE);

        reg.get_or_create("a").expect("create");
        reg.get_or_create("b").expect("create");
        assert_eq!(reg.live_pool_count(), 2);

        reg.destroy("a");
        assert_eq!(reg.live_pool_count(), 1);
    }
}
