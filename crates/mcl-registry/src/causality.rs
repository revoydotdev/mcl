//! Causality graph for tracking cause-effect relationships between signals.
//!
//! [`CausalityGraph`] maintains a directed acyclic graph (DAG) where edges
//! represent "child was caused by parent" relationships. It supports:
//! - Transitive ancestor/descendant queries via BFS
//! - Auto-inheritance: signals without explicit causes inherit from recent node history
//! - Persistence to JSONL (append-only edge log)

use std::collections::{HashMap, HashSet, VecDeque};
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::types::SignalId;

/// Serialized edge record for JSONL persistence.
#[derive(Debug, Serialize, Deserialize)]
struct EdgeRecord {
    child: String,
    parents: Vec<String>,
}

/// Directed acyclic graph capturing causality between signals.
///
/// Edges point from parent (cause) to child (effect). The graph maintains
/// both directions for efficient traversal.
pub struct CausalityGraph {
    /// child → direct parents (causes of this signal)
    parents: HashMap<SignalId, Vec<SignalId>>,
    /// parent → direct children (signals caused by this one)
    children: HashMap<SignalId, Vec<SignalId>>,
    /// How many recent signals to auto-inherit when no explicit causes given
    auto_inherit_window: usize,
    /// Per-node ring buffer of recently received signals for auto-inherit
    node_history: HashMap<String, VecDeque<SignalId>>,
}

impl CausalityGraph {
    /// Create a new graph with the default auto-inherit window of 10.
    pub fn new() -> Self {
        Self::with_window(10)
    }

    /// Create a new graph with a custom auto-inherit window.
    pub fn with_window(auto_inherit_window: usize) -> Self {
        Self {
            parents: HashMap::new(),
            children: HashMap::new(),
            auto_inherit_window,
            node_history: HashMap::new(),
        }
    }

    /// Record a causality edge: `child` was caused by each of `parents`.
    ///
    /// Updates both the parent→child and child→parent adjacency maps.
    /// Duplicate edges are silently ignored.
    pub fn add_edge(&mut self, child: &SignalId, parents: &[SignalId]) {
        let existing = self.parents.entry(child.clone()).or_default();
        for p in parents {
            if !existing.contains(p) {
                existing.push(p.clone());
            }
        }
        for p in parents {
            let list = self.children.entry(p.clone()).or_default();
            if !list.contains(child) {
                list.push(child.clone());
            }
        }
    }

    /// Return all transitive ancestors (causes) of `signal_id` via BFS.
    ///
    /// Order is breadth-first: direct parents first, then their parents, etc.
    /// The signal itself is NOT included in the result.
    pub fn ancestors(&self, signal_id: &SignalId) -> Vec<SignalId> {
        let mut result = Vec::new();
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();

        // Seed with direct parents
        if let Some(direct) = self.parents.get(signal_id) {
            for p in direct {
                if visited.insert(p.clone()) {
                    queue.push_back(p.clone());
                }
            }
        }

        while let Some(current) = queue.pop_front() {
            result.push(current.clone());
            if let Some(grandparents) = self.parents.get(&current) {
                for gp in grandparents {
                    if visited.insert(gp.clone()) {
                        queue.push_back(gp.clone());
                    }
                }
            }
        }

        result
    }

    /// Return all transitive descendants (effects) of `signal_id` via BFS.
    ///
    /// Order is breadth-first: direct children first, then their children, etc.
    /// The signal itself is NOT included in the result.
    pub fn descendants(&self, signal_id: &SignalId) -> Vec<SignalId> {
        let mut result = Vec::new();
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();

        // Seed with direct children
        if let Some(direct) = self.children.get(signal_id) {
            for c in direct {
                if visited.insert(c.clone()) {
                    queue.push_back(c.clone());
                }
            }
        }

        while let Some(current) = queue.pop_front() {
            result.push(current.clone());
            if let Some(grandchildren) = self.children.get(&current) {
                for gc in grandchildren {
                    if visited.insert(gc.clone()) {
                        queue.push_back(gc.clone());
                    }
                }
            }
        }

        result
    }

    /// Record that a node received a signal, for auto-inherit tracking.
    ///
    /// Maintains a per-node ring buffer capped at `auto_inherit_window`.
    pub fn record_signal_for_node(&mut self, node: &str, id: &SignalId) {
        let history = self.node_history.entry(node.to_string()).or_default();
        if history.len() >= self.auto_inherit_window {
            history.pop_front();
        }
        history.push_back(id.clone());
    }

    /// Return the last N signals received by a node (for auto-inherit).
    ///
    /// Returns up to `auto_inherit_window` most recent signals for the node,
    /// in chronological order (oldest first).
    pub fn auto_inherit_parents(&self, node: &str) -> Vec<SignalId> {
        self.node_history
            .get(node)
            .map(|h| h.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Persist all edges to a JSONL file (one edge record per line).
    ///
    /// Each line: `{"child":"<hex>","parents":["<hex1>","<hex2>"]}`
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(path)?;

        let mut writer = std::io::BufWriter::new(file);
        for (child, parents) in &self.parents {
            let record = EdgeRecord {
                child: child.hex(),
                parents: parents.iter().map(SignalId::hex).collect(),
            };
            let mut line =
                serde_json::to_string(&record).map_err(|e| std::io::Error::other(e.to_string()))?;
            line.push('\n');
            writer.write_all(line.as_bytes())?;
        }
        writer.flush()?;
        Ok(())
    }

    /// Load a graph from a JSONL edge log file.
    ///
    /// Reconstructs the adjacency maps from the persisted edge records.
    /// The auto-inherit window defaults to 10; use [`with_window`] to customize.
    pub fn load(path: &Path) -> std::io::Result<Self> {
        Self::load_with_window(path, 10)
    }

    /// Load a graph from a JSONL edge log file with a custom auto-inherit window.
    pub fn load_with_window(path: &Path, window: usize) -> std::io::Result<Self> {
        let mut graph = Self::with_window(window);

        let file = OpenOptions::new().read(true).open(path)?;
        let reader = BufReader::new(file);

        for line in reader.lines() {
            let line = line?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let record: EdgeRecord =
                serde_json::from_str(trimmed).map_err(|e| std::io::Error::other(e.to_string()))?;

            let child = signal_id_from_hex(&record.child)?;
            let parents = record
                .parents
                .iter()
                .map(|h| signal_id_from_hex(h))
                .collect::<std::io::Result<Vec<_>>>()?;

            graph.add_edge(&child, &parents);
        }

        Ok(graph)
    }
}

/// Parse a hex string (64 lowercase hex chars) into a SignalId.
fn signal_id_from_hex(hex: &str) -> std::io::Result<SignalId> {
    if hex.len() != 64 {
        return Err(std::io::Error::other(format!(
            "expected 64-char hex string, got {} chars",
            hex.len()
        )));
    }
    let mut hash = [0u8; 32];
    for i in 0..32 {
        let byte = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16)
            .map_err(|e| std::io::Error::other(format!("invalid hex at byte {i}: {e}")))?;
        hash[i] = byte;
    }
    Ok(SignalId { hash })
}

impl Default for CausalityGraph {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_id(seed: u8) -> SignalId {
        let mut hash = [0u8; 32];
        hash[0] = seed;
        SignalId { hash }
    }

    #[test]
    fn linear_chain_ancestors_and_descendants() {
        // A → B → C
        let a = make_id(1);
        let b = make_id(2);
        let c = make_id(3);

        let mut graph = CausalityGraph::new();
        graph.add_edge(&b, std::slice::from_ref(&a));
        graph.add_edge(&c, std::slice::from_ref(&b));

        let ancestors_c = graph.ancestors(&c);
        assert_eq!(
            ancestors_c,
            vec![b.clone(), a.clone()],
            "ancestors(C) should be [B, A] in BFS order"
        );

        let descendants_a = graph.descendants(&a);
        assert_eq!(
            descendants_a,
            vec![b.clone(), c.clone()],
            "descendants(A) should be [B, C] in BFS order"
        );

        // Leaf node has no descendants
        assert!(graph.descendants(&c).is_empty());

        // Root node has no ancestors
        assert!(graph.ancestors(&a).is_empty());
    }

    #[test]
    fn diamond_ancestors() {
        // A → B, A → C, B → D, C → D
        let a = make_id(1);
        let b = make_id(2);
        let c = make_id(3);
        let d = make_id(4);

        let mut graph = CausalityGraph::new();
        graph.add_edge(&b, std::slice::from_ref(&a));
        graph.add_edge(&c, std::slice::from_ref(&a));
        graph.add_edge(&d, &[b.clone(), c.clone()]);

        let ancestors_d = graph.ancestors(&d);
        // BFS: direct parents first (B, C), then their parent (A)
        assert!(ancestors_d.contains(&b), "B should be an ancestor of D");
        assert!(ancestors_d.contains(&c), "C should be an ancestor of D");
        assert!(ancestors_d.contains(&a), "A should be an ancestor of D");
        assert_eq!(ancestors_d.len(), 3, "D should have exactly 3 ancestors");
    }

    #[test]
    fn auto_inherit_respects_window() {
        let mut graph = CausalityGraph::with_window(3);
        let s0 = make_id(0);
        let s1 = make_id(1);
        let s2 = make_id(2);
        let s3 = make_id(3);

        graph.record_signal_for_node("node-a", &s0);
        graph.record_signal_for_node("node-a", &s1);
        graph.record_signal_for_node("node-a", &s2);
        graph.record_signal_for_node("node-a", &s3);

        let inherited = graph.auto_inherit_parents("node-a");
        // Window is 3, so s0 should be evicted
        assert_eq!(inherited, vec![s1, s2, s3]);

        // Unknown node returns empty
        assert!(graph.auto_inherit_parents("unknown").is_empty());
    }

    #[test]
    fn save_load_roundtrip() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("causality.jsonl");

        let a = make_id(1);
        let b = make_id(2);
        let c = make_id(3);

        let mut graph = CausalityGraph::new();
        graph.add_edge(&b, std::slice::from_ref(&a));
        graph.add_edge(&c, std::slice::from_ref(&b));

        graph.save(&path).expect("save");

        let loaded = CausalityGraph::load(&path).expect("load");

        // Verify adjacency is preserved
        assert_eq!(loaded.ancestors(&c), vec![b.clone(), a.clone()]);
        assert_eq!(loaded.descendants(&a), vec![b.clone(), c.clone()]);
    }

    #[test]
    fn save_load_empty_graph() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("empty.jsonl");

        let graph = CausalityGraph::new();
        graph.save(&path).expect("save empty");

        let loaded = CausalityGraph::load(&path).expect("load empty");
        let id = make_id(99);
        assert!(loaded.ancestors(&id).is_empty());
        assert!(loaded.descendants(&id).is_empty());
    }

    #[test]
    fn duplicate_edges_ignored() {
        let a = make_id(1);
        let b = make_id(2);

        let mut graph = CausalityGraph::new();
        graph.add_edge(&b, std::slice::from_ref(&a)); // duplicate
        graph.add_edge(&b, std::slice::from_ref(&a)); // duplicate

        let parents = graph.parents.get(&b).expect("should have parents");
        assert_eq!(parents.len(), 1, "duplicate edge should be ignored");
    }

    #[test]
    fn default_is_new() {
        let graph = CausalityGraph::default();
        assert_eq!(graph.auto_inherit_window, 10);
        assert!(graph.parents.is_empty());
        assert!(graph.children.is_empty());
    }
}
