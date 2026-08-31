//! Query interface for the signal registry.
//!
//! [`Registry`] composes [`SignalLog`], [`RegistryIndex`], and [`CausalityGraph`]
//! into a unified query interface. [`RegistryQuery`] defines six query variants,
//! and [`Registry::query`] dispatches to the appropriate subsystem.

#![warn(clippy::unwrap_used)]

use std::collections::HashSet;
use std::path::Path;

use mcl_graph::id::NodeId;

use crate::causality::CausalityGraph;
use crate::index::RegistryIndex;
use crate::log::{SignalLog, SignalLogError};
use crate::types::{FsyncPolicy, SignalId, SignalLogEntry};

/// Query variant for searching the signal registry.
pub enum RegistryQuery {
    /// Filter entries by channel path.
    ByPath { path: String },
    /// Filter entries by turn range [start, end].
    ByTurnRange { start: u64, end: u64 },
    /// Filter entries by origin node.
    ByOrigin { node_id: NodeId },
    /// Vector similarity search over ingested entries.
    BySemantic { text: String, k: usize },
    /// Find all transitive ancestors (causes) of a signal.
    ByAncestors { signal_id: SignalId },
    /// Find all transitive descendants (effects) of a signal.
    ByDescendants { signal_id: SignalId },
}

/// Result of a registry query.
pub struct QueryResult {
    /// Matching log entries.
    pub entries: Vec<SignalLogEntry>,
    /// Total number of matching entries.
    pub total: usize,
}

/// Unified registry composing signal log, semantic index, and causality graph.
pub struct Registry {
    log: SignalLog,
    index: RegistryIndex,
    causality: CausalityGraph,
}

/// Errors that can occur during registry operations.
#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("signal log error: {0}")]
    Log(#[from] SignalLogError),
    #[error("index error: {0}")]
    Index(String),
}

impl Registry {
    /// Create a new registry backed by a JSONL file at `path`.
    pub fn new(path: &Path, policy: FsyncPolicy) -> Result<Self, RegistryError> {
        let log = SignalLog::open(path, policy)?;
        let index = RegistryIndex::new().map_err(RegistryError::Index)?;
        let causality = CausalityGraph::new();
        Ok(Self {
            log,
            index,
            causality,
        })
    }

    /// Execute a query against the registry.
    pub fn query(&mut self, q: RegistryQuery) -> Result<QueryResult, RegistryError> {
        let entries = self.log.read()?;

        let matching: Vec<SignalLogEntry> = match q {
            RegistryQuery::ByPath { path } => {
                entries.into_iter().filter(|e| e.channel == path).collect()
            }

            RegistryQuery::ByTurnRange { start, end } => entries
                .into_iter()
                .filter(|e| e.turn >= start && e.turn <= end)
                .collect(),

            RegistryQuery::ByOrigin { node_id } => entries
                .into_iter()
                .filter(|e| e.origin == node_id)
                .collect(),

            RegistryQuery::BySemantic { ref text, k } => {
                let results = self.index.query_semantic(text, k);
                let id_set: HashSet<&SignalId> = results.iter().map(|(id, _)| id).collect();
                entries
                    .into_iter()
                    .filter(|e| id_set.contains(&e.id))
                    .collect()
            }

            RegistryQuery::ByAncestors { ref signal_id } => {
                let ancestor_ids = self.causality.ancestors(signal_id);
                let id_set: HashSet<SignalId> = ancestor_ids.into_iter().collect();
                entries
                    .into_iter()
                    .filter(|e| id_set.contains(&e.id))
                    .collect()
            }

            RegistryQuery::ByDescendants { ref signal_id } => {
                let descendant_ids = self.causality.descendants(signal_id);
                let id_set: HashSet<SignalId> = descendant_ids.into_iter().collect();
                entries
                    .into_iter()
                    .filter(|e| id_set.contains(&e.id))
                    .collect()
            }
        };

        let total = matching.len();
        Ok(QueryResult {
            entries: matching,
            total,
        })
    }

    /// Access the underlying signal log (immutable).
    pub fn log(&self) -> &SignalLog {
        &self.log
    }

    /// Access the underlying signal log (mutable).
    pub fn log_mut(&mut self) -> &mut SignalLog {
        &mut self.log
    }

    /// Access the underlying semantic index (immutable).
    pub fn index(&self) -> &RegistryIndex {
        &self.index
    }

    /// Access the underlying semantic index (mutable).
    pub fn index_mut(&mut self) -> &mut RegistryIndex {
        &mut self.index
    }

    /// Access the underlying causality graph (immutable).
    pub fn causality(&self) -> &CausalityGraph {
        &self.causality
    }

    /// Access the underlying causality graph (mutable).
    pub fn causality_mut(&mut self) -> &mut CausalityGraph {
        &mut self.causality
    }

    /// Split-borrow accessor for log + index (needed for rebuild).
    pub fn log_and_index_mut(&mut self) -> (&SignalLog, &mut RegistryIndex) {
        (&self.log, &mut self.index)
    }

    /// Number of entries in the signal log.
    pub fn len(&self) -> u64 {
        self.log.len()
    }

    /// Whether the signal log is empty.
    pub fn is_empty(&self) -> bool {
        self.log.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mcl_spec::label::LabelSet;
    use mcl_spec::signal::SignalKind;
    use mcl_spec::span::Span;
    use tempfile::NamedTempFile;

    fn test_signal(name: &str) -> mcl_spec::signal::MclSignal {
        mcl_spec::signal::MclSignal {
            kind: SignalKind::Action {
                name: name.to_string(),
                args: Default::default(),
            },
            span: Span::new(0, 1, 1, 1),
            labels: LabelSet::default(),
        }
    }

    /// Helper: create a Registry with a temp file and seed it with entries.
    fn setup_registry() -> (Registry, NamedTempFile) {
        let tmp = NamedTempFile::new().expect("temp file");
        let path = tmp.path().to_path_buf();

        let mut registry = Registry::new(&path, FsyncPolicy::OnShutdown).expect("new");

        registry
            .log_mut()
            .append(
                test_signal("attack"),
                NodeId::new("player"),
                1,
                "actions".to_string(),
                crate::types::Verdict::Accept,
            )
            .expect("append 1");
        registry
            .log_mut()
            .append(
                test_signal("defend"),
                NodeId::new("player"),
                2,
                "actions".to_string(),
                crate::types::Verdict::Accept,
            )
            .expect("append 2");
        registry
            .log_mut()
            .append(
                test_signal("heal"),
                NodeId::new("player"),
                3,
                "inventory".to_string(),
                crate::types::Verdict::Accept,
            )
            .expect("append 3");
        registry
            .log_mut()
            .append(
                test_signal("move"),
                NodeId::new("guard"),
                2,
                "actions".to_string(),
                crate::types::Verdict::Accept,
            )
            .expect("append 4");
        registry
            .log_mut()
            .append(
                test_signal("alert"),
                NodeId::new("guard"),
                5,
                "internal".to_string(),
                crate::types::Verdict::Accept,
            )
            .expect("append 5");

        (registry, tmp)
    }

    #[test]
    fn query_by_path_filters_by_channel() {
        let (mut registry, _tmp) = setup_registry();

        let result = registry
            .query(RegistryQuery::ByPath {
                path: "actions".to_string(),
            })
            .expect("query");

        assert_eq!(result.total, 3, "3 entries on 'actions' channel");
        assert!(result.entries.iter().all(|e| e.channel == "actions"));
    }

    #[test]
    fn query_by_turn_range_filters_by_turn() {
        let (mut registry, _tmp) = setup_registry();

        let result = registry
            .query(RegistryQuery::ByTurnRange { start: 2, end: 3 })
            .expect("query");

        assert_eq!(result.total, 3, "3 entries with turn 2 or 3");
        assert!(result.entries.iter().all(|e| e.turn >= 2 && e.turn <= 3));
    }

    #[test]
    fn query_by_origin_filters_by_node() {
        let (mut registry, _tmp) = setup_registry();

        let result = registry
            .query(RegistryQuery::ByOrigin {
                node_id: NodeId::new("guard"),
            })
            .expect("query");

        assert_eq!(result.total, 2, "2 entries from guard");
        assert!(result
            .entries
            .iter()
            .all(|e| e.origin == NodeId::new("guard")));
    }

    #[test]
    fn query_by_semantic_uses_vector_index() {
        let (mut registry, _tmp) = setup_registry();

        let entries = registry.log().read().expect("read");
        for entry in &entries {
            registry.index_mut().ingest(entry);
        }

        let result = registry
            .query(RegistryQuery::BySemantic {
                text: "attack".to_string(),
                k: 2,
            })
            .expect("query");

        assert!(
            result.total <= 2,
            "semantic query should return at most k=2 results, got {}",
            result.total
        );
        let entry_ids: HashSet<&SignalId> = entries.iter().map(|e| &e.id).collect();
        for e in &result.entries {
            assert!(entry_ids.contains(&e.id), "returned entry must be from log");
        }
    }

    #[test]
    fn query_by_ancestors_uses_causality_graph() {
        let (mut registry, _tmp) = setup_registry();

        let entries = registry.log().read().expect("read");

        registry
            .causality_mut()
            .add_edge(&entries[1].id, &[entries[0].id.clone()]);
        registry
            .causality_mut()
            .add_edge(&entries[2].id, &[entries[1].id.clone()]);

        let result = registry
            .query(RegistryQuery::ByAncestors {
                signal_id: entries[2].id.clone(),
            })
            .expect("query");

        assert_eq!(result.total, 2, "entry[2] should have 2 ancestors");
        let result_ids: HashSet<&SignalId> = result.entries.iter().map(|e| &e.id).collect();
        assert!(
            result_ids.contains(&entries[0].id),
            "entry[0] should be an ancestor"
        );
        assert!(
            result_ids.contains(&entries[1].id),
            "entry[1] should be an ancestor"
        );
    }

    #[test]
    fn query_by_descendants_uses_causality_graph() {
        let (mut registry, _tmp) = setup_registry();

        let entries = registry.log().read().expect("read");

        registry
            .causality_mut()
            .add_edge(&entries[1].id, &[entries[0].id.clone()]);
        registry
            .causality_mut()
            .add_edge(&entries[2].id, &[entries[1].id.clone()]);

        let result = registry
            .query(RegistryQuery::ByDescendants {
                signal_id: entries[0].id.clone(),
            })
            .expect("query");

        assert_eq!(result.total, 2, "entry[0] should have 2 descendants");
        let result_ids: HashSet<&SignalId> = result.entries.iter().map(|e| &e.id).collect();
        assert!(
            result_ids.contains(&entries[1].id),
            "entry[1] should be a descendant"
        );
        assert!(
            result_ids.contains(&entries[2].id),
            "entry[2] should be a descendant"
        );
    }

    #[test]
    fn query_empty_registry_returns_empty() {
        let tmp = NamedTempFile::new().expect("temp file");
        let path = tmp.path().to_path_buf();

        let mut registry = Registry::new(&path, FsyncPolicy::OnShutdown).expect("new");
        assert!(registry.is_empty());

        let result = registry
            .query(RegistryQuery::ByPath {
                path: "anything".to_string(),
            })
            .expect("query");

        assert_eq!(result.total, 0);
        assert!(result.entries.is_empty());
    }

    #[test]
    fn query_by_path_no_matches() {
        let (mut registry, _tmp) = setup_registry();

        let result = registry
            .query(RegistryQuery::ByPath {
                path: "nonexistent".to_string(),
            })
            .expect("query");

        assert_eq!(result.total, 0);
        assert!(result.entries.is_empty());
    }

    #[test]
    fn query_by_turn_range_exclusive_bounds() {
        let (mut registry, _tmp) = setup_registry();

        let result = registry
            .query(RegistryQuery::ByTurnRange { start: 5, end: 5 })
            .expect("query");

        assert_eq!(result.total, 1);
        assert_eq!(result.entries[0].turn, 5);
        assert_eq!(result.entries[0].origin, NodeId::new("guard"));
    }
}
