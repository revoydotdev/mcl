//! Registry telemetry aggregator — accumulates signal stats across registry operations.
//!
//! [`TelemetryAggregator`] records per-signal verdicts with arbiter, origin, and signal-kind
//! metadata. [`RegistryStats`] is the serializable snapshot with computed methods like
//! [`accept_rate`](RegistryStats::accept_rate) and [`merge`](RegistryStats::merge).

use std::path::Path;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::types::Verdict;

// ---------------------------------------------------------------------------
// RegistryStats
// ---------------------------------------------------------------------------

/// Serializable snapshot of registry telemetry counters.
///
/// Counters are split by verdict category, rejecting arbiter, origin node, and
/// signal kind. Use [`accept_rate`](Self::accept_rate) for the overall accept
/// fraction and [`merge`](Self::merge) to combine snapshots.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryStats {
    /// Total signals processed.
    pub total_signals: u64,
    /// Signals with verdict [`Verdict::Accept`].
    pub accept_count: u64,
    /// Signals with verdict [`Verdict::Reject`].
    pub reject_count: u64,
    /// Signals with verdict [`Verdict::Defer`].
    pub defer_count: u64,
    /// Arbiter name → rejection count (only incremented on `Reject`).
    pub by_arbiter: IndexMap<String, u64>,
    /// Origin node → total signal count.
    pub by_origin: IndexMap<String, u64>,
    /// Signal kind label → total signal count.
    pub by_signal_kind: IndexMap<String, u64>,
}

impl RegistryStats {
    /// Create an empty stats snapshot.
    pub fn new() -> Self {
        Self {
            total_signals: 0,
            accept_count: 0,
            reject_count: 0,
            defer_count: 0,
            by_arbiter: IndexMap::new(),
            by_origin: IndexMap::new(),
            by_signal_kind: IndexMap::new(),
        }
    }

    /// Accept rate as a fraction in `[0.0, 1.0]`.
    ///
    /// Returns `0.0` when no signals have been recorded.
    pub fn accept_rate(&self) -> f64 {
        if self.total_signals == 0 {
            return 0.0;
        }
        self.accept_count as f64 / self.total_signals as f64
    }

    /// Merge another stats snapshot into this one (additive).
    ///
    /// Counters are summed; per-map entries are combined key-by-key.
    pub fn merge(&mut self, other: &RegistryStats) {
        self.total_signals += other.total_signals;
        self.accept_count += other.accept_count;
        self.reject_count += other.reject_count;
        self.defer_count += other.defer_count;

        for (k, &v) in &other.by_arbiter {
            *self.by_arbiter.entry(k.clone()).or_insert(0) += v;
        }
        for (k, &v) in &other.by_origin {
            *self.by_origin.entry(k.clone()).or_insert(0) += v;
        }
        for (k, &v) in &other.by_signal_kind {
            *self.by_signal_kind.entry(k.clone()).or_insert(0) += v;
        }
    }
}

impl Default for RegistryStats {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// TelemetryAggregator
// ---------------------------------------------------------------------------

/// Accumulates telemetry across registry operations (in-memory, sync).
///
/// Call [`record`](Self::record) for each signal that passes through the registry,
/// then [`stats`](Self::stats) to take a snapshot or [`export_json`](Self::export_json)
/// to write one to disk.
pub struct TelemetryAggregator {
    total_signals: u64,
    accept_count: u64,
    reject_count: u64,
    defer_count: u64,
    by_arbiter: IndexMap<String, u64>,
    by_origin: IndexMap<String, u64>,
    by_signal_kind: IndexMap<String, u64>,
}

impl TelemetryAggregator {
    /// Create a new aggregator with all counters at zero.
    pub fn new() -> Self {
        Self {
            total_signals: 0,
            accept_count: 0,
            reject_count: 0,
            defer_count: 0,
            by_arbiter: IndexMap::new(),
            by_origin: IndexMap::new(),
            by_signal_kind: IndexMap::new(),
        }
    }

    /// Record a single signal's verdict.
    ///
    /// * `verdict` — the [`Verdict`] returned by the arbiter chain.
    /// * `arbiter_name` — the name of the arbiter that produced a non-Accept verdict
    ///   (pass `None` for accepts or if the rejecting arbiter is unknown).
    /// * `origin` — the node that emitted the signal.
    /// * `signal_kind` — a human-readable label for the signal kind
    ///   (e.g. `"action"`, `"mutate"`, `"noop"`).
    pub fn record(
        &mut self,
        verdict: &Verdict,
        arbiter_name: Option<&str>,
        origin: &str,
        signal_kind: &str,
    ) {
        self.total_signals += 1;

        match verdict {
            Verdict::Accept => self.accept_count += 1,
            Verdict::Reject { .. } => {
                self.reject_count += 1;
                if let Some(name) = arbiter_name {
                    *self.by_arbiter.entry(name.to_string()).or_insert(0) += 1;
                }
            }
            Verdict::Defer => self.defer_count += 1,
            Verdict::RouteTo { .. } => {
                self.reject_count += 1;
                if let Some(name) = arbiter_name {
                    *self.by_arbiter.entry(name.to_string()).or_insert(0) += 1;
                }
            }
        }

        *self.by_origin.entry(origin.to_string()).or_insert(0) += 1;
        *self
            .by_signal_kind
            .entry(signal_kind.to_string())
            .or_insert(0) += 1;
    }

    /// Return a snapshot of the current telemetry counters.
    pub fn stats(&self) -> RegistryStats {
        RegistryStats {
            total_signals: self.total_signals,
            accept_count: self.accept_count,
            reject_count: self.reject_count,
            defer_count: self.defer_count,
            by_arbiter: self.by_arbiter.clone(),
            by_origin: self.by_origin.clone(),
            by_signal_kind: self.by_signal_kind.clone(),
        }
    }

    /// Write the current stats snapshot to `path` as pretty-printed JSON.
    ///
    /// Creates parent directories if they don't exist.
    pub fn export_json(&self, path: &Path) -> Result<(), std::io::Error> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let snapshot = self.stats();
        let json = serde_json::to_string_pretty(&snapshot)?;
        std::fs::write(path, json)
    }
}

impl Default for TelemetryAggregator {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- helpers --

    fn accept() -> Verdict {
        Verdict::Accept
    }

    fn reject(reason: &str) -> Verdict {
        Verdict::Reject {
            reason: reason.to_string(),
        }
    }

    fn defer() -> Verdict {
        Verdict::Defer
    }

    // -- tests --

    #[test]
    fn record_accept_increments_accept_count() {
        let mut agg = TelemetryAggregator::new();
        agg.record(&accept(), None, "player", "action");
        let s = agg.stats();
        assert_eq!(s.total_signals, 1);
        assert_eq!(s.accept_count, 1);
        assert_eq!(s.reject_count, 0);
        assert_eq!(s.defer_count, 0);
    }

    #[test]
    fn record_reject_increments_reject_count() {
        let mut agg = TelemetryAggregator::new();
        agg.record(&reject("bad"), Some("schema"), "player", "action");
        let s = agg.stats();
        assert_eq!(s.reject_count, 1);
        assert_eq!(s.accept_count, 0);
    }

    #[test]
    fn record_defer_increments_defer_count() {
        let mut agg = TelemetryAggregator::new();
        agg.record(&defer(), None, "llm", "narrate");
        let s = agg.stats();
        assert_eq!(s.defer_count, 1);
    }

    #[test]
    fn per_arbiter_breakdown() {
        let mut agg = TelemetryAggregator::new();
        agg.record(&reject("a"), Some("schema"), "n1", "action");
        agg.record(&reject("b"), Some("schema"), "n1", "action");
        agg.record(&reject("c"), Some("dedup"), "n1", "action");

        let s = agg.stats();
        assert_eq!(s.by_arbiter.get("schema"), Some(&2));
        assert_eq!(s.by_arbiter.get("dedup"), Some(&1));
        assert_eq!(s.by_arbiter.len(), 2);
    }

    #[test]
    fn per_origin_counts() {
        let mut agg = TelemetryAggregator::new();
        agg.record(&accept(), None, "player", "action");
        agg.record(&accept(), None, "player", "action");
        agg.record(&accept(), None, "guard", "action");

        let s = agg.stats();
        assert_eq!(s.by_origin.get("player"), Some(&2));
        assert_eq!(s.by_origin.get("guard"), Some(&1));
    }

    #[test]
    fn export_json_round_trip() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("telemetry.json");

        let mut agg = TelemetryAggregator::new();
        agg.record(&accept(), None, "player", "action");
        agg.record(&reject("nope"), Some("hallucination"), "llm", "action");
        agg.export_json(&path).expect("export");

        let raw = std::fs::read_to_string(&path).expect("read");
        let loaded: RegistryStats = serde_json::from_str(&raw).expect("parse");

        let expected = agg.stats();
        assert_eq!(loaded.total_signals, expected.total_signals);
        assert_eq!(loaded.accept_count, expected.accept_count);
        assert_eq!(loaded.reject_count, expected.reject_count);
        assert_eq!(loaded.by_arbiter, expected.by_arbiter);
        assert_eq!(loaded.by_origin, expected.by_origin);
        assert_eq!(loaded.by_signal_kind, expected.by_signal_kind);
    }

    #[test]
    fn empty_stats_accept_rate_is_zero() {
        let agg = TelemetryAggregator::new();
        let s = agg.stats();
        assert_eq!(s.accept_rate(), 0.0);
        assert_eq!(s.total_signals, 0);
    }

    #[test]
    fn mixed_verdicts_accept_rate() {
        let mut agg = TelemetryAggregator::new();
        // 5 accepts, 3 rejects, 2 defers = 10 total
        for _ in 0..5 {
            agg.record(&accept(), None, "player", "action");
        }
        for _ in 0..3 {
            agg.record(&reject("r"), Some("schema"), "llm", "mutate");
        }
        for _ in 0..2 {
            agg.record(&defer(), None, "guard", "narrate");
        }

        let s = agg.stats();
        assert_eq!(s.total_signals, 10);
        assert_eq!(s.accept_count, 5);
        assert_eq!(s.reject_count, 3);
        assert_eq!(s.defer_count, 2);

        let rate = s.accept_rate();
        assert!(
            (rate - 0.5).abs() < f64::EPSILON,
            "expected 0.5, got {rate}"
        );
    }

    #[test]
    fn merge_combines_snapshots() {
        let mut agg1 = TelemetryAggregator::new();
        agg1.record(&accept(), None, "player", "action");
        agg1.record(&accept(), None, "player", "action");

        let mut agg2 = TelemetryAggregator::new();
        agg2.record(&reject("x"), Some("schema"), "guard", "mutate");

        let mut combined = agg1.stats();
        combined.merge(&agg2.stats());

        assert_eq!(combined.total_signals, 3);
        assert_eq!(combined.accept_count, 2);
        assert_eq!(combined.reject_count, 1);
        assert_eq!(combined.by_origin.get("player"), Some(&2));
        assert_eq!(combined.by_origin.get("guard"), Some(&1));
    }

    #[test]
    fn arbiter_not_recorded_on_accept() {
        let mut agg = TelemetryAggregator::new();
        agg.record(&accept(), Some("irrelevant"), "player", "action");
        let s = agg.stats();
        assert!(
            s.by_arbiter.is_empty(),
            "arbiter should not be recorded for Accept verdict"
        );
    }

    #[test]
    fn export_creates_parent_dirs() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("nested").join("deep").join("stats.json");

        let agg = TelemetryAggregator::new();
        agg.export_json(&path).expect("export with mkdir");

        assert!(path.exists(), "file should have been created");
        let raw = std::fs::read_to_string(&path).expect("read");
        let loaded: RegistryStats = serde_json::from_str(&raw).expect("parse");
        assert_eq!(loaded.total_signals, 0);
    }
}
