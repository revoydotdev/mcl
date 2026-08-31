//! Crash recovery for the signal registry.
//!
//! Handles partial writes (truncated JSONL lines) and provides index rebuild
//! from the authoritative JSONL log, plus consistency verification.

use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

use thiserror::Error;

use crate::index::RegistryIndex;
use crate::log::SignalLog;
use crate::types::SignalLogEntry;

#[derive(Debug, Error)]
pub enum RecoveryError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Serialization(String),
}

/// Result of a JSONL repair pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepairReport {
    /// Number of valid lines found.
    pub lines_valid: u64,
    /// Number of malformed/truncated lines removed.
    pub lines_removed: u64,
    /// Whether any repair was performed.
    pub was_repaired: bool,
}

/// Result of a consistency verification pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifyReport {
    /// Number of entries in the JSONL log.
    pub entries_in_log: u64,
    /// Number of entries in the index.
    pub entries_in_index: u64,
    /// Whether log and index counts match.
    pub consistent: bool,
}

/// Detect and repair truncated last line in a JSONL file.
///
/// Reads the file line by line. Valid lines (parseable as [`SignalLogEntry`])
/// are kept. The last line, if it fails to parse as complete JSON, is treated
/// as a partial write and removed. Blank lines are ignored.
///
/// Returns a [`RepairReport`] describing what was found and whether repair
/// was needed.
pub fn repair_jsonl(path: &Path) -> Result<RepairReport, RecoveryError> {
    let file = OpenOptions::new().read(true).open(path)?;
    let reader = BufReader::new(file);

    let mut valid_lines: Vec<String> = Vec::new();
    let mut lines_removed: u64 = 0;

    for line in reader.lines() {
        let line = line?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if serde_json::from_str::<SignalLogEntry>(trimmed).is_ok() {
            valid_lines.push(line);
        } else {
            lines_removed += 1;
        }
    }

    let was_repaired = lines_removed > 0;
    let lines_valid = valid_lines.len() as u64;

    if was_repaired {
        let mut file = OpenOptions::new().write(true).truncate(true).open(path)?;
        for line in &valid_lines {
            file.write_all(line.as_bytes())?;
            file.write_all(b"\n")?;
        }
        file.flush()?;
    }

    Ok(RepairReport {
        lines_valid,
        lines_removed,
        was_repaired,
    })
}

/// Rebuild the semantic index from the JSONL log.
///
/// Reads all entries from `log` and ingests each one into `index`.
/// The index is expected to be freshly constructed (empty); this function
/// does not clear it first — the caller should provide a clean index.
pub fn rebuild_index(log: &SignalLog, index: &mut RegistryIndex) -> Result<(), RecoveryError> {
    let entries = log
        .read()
        .map_err(|e| RecoveryError::Serialization(e.to_string()))?;

    for entry in &entries {
        index.ingest(entry);
    }

    Ok(())
}

/// Verify consistency between the JSONL log and the semantic index.
///
/// Compares entry counts. If the number of entries in the log matches
/// the number in the index, the data is considered consistent.
pub fn verify_consistency(
    log: &SignalLog,
    index: &RegistryIndex,
) -> Result<VerifyReport, RecoveryError> {
    let entries = log
        .read()
        .map_err(|e| RecoveryError::Serialization(e.to_string()))?;

    let entries_in_log = entries.len() as u64;
    let entries_in_index = index.len() as u64;

    Ok(VerifyReport {
        entries_in_log,
        entries_in_index,
        consistent: entries_in_log == entries_in_index,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{FsyncPolicy, SignalId, Verdict};
    use mcl_graph::id::NodeId;
    use mcl_spec::label::LabelSet;
    use mcl_spec::signal::{MclSignal, SignalKind};
    use mcl_spec::span::Span;

    fn test_signal(name: &str) -> MclSignal {
        MclSignal {
            kind: SignalKind::Action {
                name: name.to_string(),
                args: Default::default(),
            },
            span: Span::new(0, 1, 1, 1),
            labels: LabelSet::default(),
        }
    }

    fn make_entry(origin: &str, turn: u64, channel: &str, name: &str) -> SignalLogEntry {
        let signal = test_signal(name);
        SignalLogEntry {
            id: SignalId::from_signal(&NodeId::new(origin), turn, channel, &signal),
            signal,
            origin: NodeId::new(origin),
            turn,
            channel: channel.to_string(),
            timestamp: 1714000000000 + turn,
            verdict: Verdict::Accept,
            policy_bindings: None,
        }
    }

    fn write_jsonl(path: &Path, lines: &[&str]) {
        let mut f = std::fs::File::create(path).expect("create");
        for line in lines {
            std::io::Write::write_all(&mut f, line.as_bytes()).expect("write");
            std::io::Write::write_all(&mut f, b"\n").expect("newline");
        }
    }

    #[test]
    fn truncated_last_line_is_removed() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("test.jsonl");

        let entry = make_entry("player", 1, "actions", "attack");
        let valid_line = serde_json::to_string(&entry).expect("serialize");

        write_jsonl(&path, &[valid_line.as_str(), r#"{"id":"abc","incomplete"#]);

        let report = repair_jsonl(&path).expect("repair");
        assert!(report.was_repaired);
        assert_eq!(report.lines_valid, 1);
        assert_eq!(report.lines_removed, 1);

        let contents = std::fs::read_to_string(&path).expect("read");
        let remaining: Vec<&str> = contents.lines().filter(|l| !l.trim().is_empty()).collect();
        assert_eq!(remaining.len(), 1);
        let parsed: SignalLogEntry =
            serde_json::from_str(remaining[0]).expect("remaining line should be valid");
        assert_eq!(parsed.id, entry.id);
    }

    #[test]
    fn valid_jsonl_passes_unchanged() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("test.jsonl");

        let e1 = make_entry("a", 1, "ch", "attack");
        let e2 = make_entry("b", 2, "ch", "defend");
        let line1 = serde_json::to_string(&e1).expect("serialize");
        let line2 = serde_json::to_string(&e2).expect("serialize");

        write_jsonl(&path, &[line1.as_str(), line2.as_str()]);

        let report = repair_jsonl(&path).expect("repair");
        assert!(!report.was_repaired);
        assert_eq!(report.lines_valid, 2);
        assert_eq!(report.lines_removed, 0);
    }

    #[test]
    fn rebuild_index_populates_from_log() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("test.jsonl");

        let mut log = SignalLog::open(&path, FsyncPolicy::EveryStep).expect("open log");
        let e1 = log
            .append(
                test_signal("attack"),
                NodeId::new("player"),
                1,
                "actions".to_string(),
                Verdict::Accept,
            )
            .expect("append 1");
        log.append(
            test_signal("defend"),
            NodeId::new("guard"),
            2,
            "actions".to_string(),
            Verdict::Accept,
        )
        .expect("append 2");

        let mut index = RegistryIndex::new().expect("new index");
        assert!(index.is_empty());

        rebuild_index(&log, &mut index).expect("rebuild");

        assert_eq!(index.len(), 2);
        let results = index.query_semantic("attack", 1);
        assert!(!results.is_empty());
        assert_eq!(results[0].0, e1.id);
    }

    #[test]
    fn verify_detects_mismatch() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("test.jsonl");

        let mut log = SignalLog::open(&path, FsyncPolicy::EveryStep).expect("open log");
        log.append(
            test_signal("attack"),
            NodeId::new("player"),
            1,
            "actions".to_string(),
            Verdict::Accept,
        )
        .expect("append");
        log.append(
            test_signal("defend"),
            NodeId::new("guard"),
            2,
            "actions".to_string(),
            Verdict::Accept,
        )
        .expect("append");

        let index = RegistryIndex::new().expect("new index");

        let report = verify_consistency(&log, &index).expect("verify");
        assert_eq!(report.entries_in_log, 2);
        assert_eq!(report.entries_in_index, 0);
        assert!(!report.consistent);
    }

    #[test]
    fn verify_passes_for_consistent_data() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("test.jsonl");

        let mut log = SignalLog::open(&path, FsyncPolicy::EveryStep).expect("open log");
        log.append(
            test_signal("attack"),
            NodeId::new("player"),
            1,
            "actions".to_string(),
            Verdict::Accept,
        )
        .expect("append");
        log.append(
            test_signal("defend"),
            NodeId::new("guard"),
            2,
            "actions".to_string(),
            Verdict::Accept,
        )
        .expect("append");

        let mut index = RegistryIndex::new().expect("new index");
        rebuild_index(&log, &mut index).expect("rebuild");

        let report = verify_consistency(&log, &index).expect("verify");
        assert_eq!(report.entries_in_log, 2);
        assert_eq!(report.entries_in_index, 2);
        assert!(report.consistent);
    }

    #[test]
    fn repair_handles_empty_file() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("empty.jsonl");

        std::fs::File::create(&path).expect("create empty");

        let report = repair_jsonl(&path).expect("repair");
        assert!(!report.was_repaired);
        assert_eq!(report.lines_valid, 0);
        assert_eq!(report.lines_removed, 0);
    }

    #[test]
    fn repair_handles_multiple_malformed_lines() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("multi.jsonl");

        let entry = make_entry("x", 1, "ch", "a");
        let valid = serde_json::to_string(&entry).expect("serialize");

        write_jsonl(
            &path,
            &[
                valid.as_str(),
                "not json at all",
                r#"{"broken"#,
                valid.as_str(),
            ],
        );

        let report = repair_jsonl(&path).expect("repair");
        assert!(report.was_repaired);
        assert_eq!(report.lines_valid, 2);
        assert_eq!(report.lines_removed, 2);
    }
}
