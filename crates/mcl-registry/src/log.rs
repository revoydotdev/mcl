//! Append-only JSONL signal log with configurable fsync policy.
//!
//! [`SignalLog`] writes [`SignalLogEntry`] values as one-JSON-object-per-line
//! (JSONL format) and supports reading back the full history.

use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::Path;

use mcl_graph::id::NodeId;
use mcl_spec::signal::MclSignal;
use thiserror::Error;

use crate::types::{FsyncPolicy, SignalId, SignalLogEntry, Verdict};

#[derive(Debug, Error)]
pub enum SignalLogError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Serialization(String),
}

/// Append-only JSONL log for signal entries.
///
/// Each call to [`append`](SignalLog::append) serializes the entry as a single
/// JSON line and writes it to disk. The `fsync` behavior is controlled by
/// the configured [`FsyncPolicy`].
pub struct SignalLog {
    path: std::path::PathBuf,
    writer: BufWriter<std::fs::File>,
    policy: FsyncPolicy,
    len: u64,
    steps_since_sync: u64,
}

impl SignalLog {
    pub fn open(path: &Path, policy: FsyncPolicy) -> Result<Self, SignalLogError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let file = OpenOptions::new().create(true).append(true).open(path)?;
        let mut log = Self {
            path: path.to_path_buf(),
            writer: BufWriter::new(file),
            policy,
            len: 0,
            steps_since_sync: 0,
        };

        log.recount()?;
        Ok(log)
    }

    /// Append an entry to the log. Returns the entry with its computed [`SignalId`].
    pub fn append(
        &mut self,
        signal: MclSignal,
        origin: NodeId,
        turn: u64,
        channel: String,
        verdict: Verdict,
    ) -> Result<SignalLogEntry, SignalLogError> {
        self.append_with_policy_bindings(signal, origin, turn, channel, verdict, None)
    }

    /// Append an entry to the log along with policy expression bindings.
    ///
    /// Used by the policy arbiter so a violation audit entry preserves the
    /// match-time environment for later auditor review (R2 §2.3).
    pub fn append_with_policy_bindings(
        &mut self,
        signal: MclSignal,
        origin: NodeId,
        turn: u64,
        channel: String,
        verdict: Verdict,
        policy_bindings: Option<serde_json::Value>,
    ) -> Result<SignalLogEntry, SignalLogError> {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or(std::time::Duration::ZERO)
            .as_millis() as u64;

        let id = SignalId::from_signal(&origin, turn, &channel, &signal);

        let entry = SignalLogEntry {
            id: id.clone(),
            signal,
            origin,
            turn,
            channel,
            timestamp,
            verdict,
            policy_bindings,
        };

        self.write_entry(&entry)?;
        self.writer.flush().map_err(SignalLogError::Io)?;
        self.len += 1;

        self.steps_since_sync += 1;
        match &self.policy {
            FsyncPolicy::EveryStep => {
                self.fsync()?;
                self.steps_since_sync = 0;
            }
            FsyncPolicy::EveryNSteps(n) => {
                if self.steps_since_sync >= *n {
                    self.fsync()?;
                    self.steps_since_sync = 0;
                }
            }
            FsyncPolicy::OnShutdown => {}
        }

        Ok(entry)
    }

    /// Read all entries from the log file.
    pub fn read(&self) -> Result<Vec<SignalLogEntry>, SignalLogError> {
        let file = OpenOptions::new().read(true).open(&self.path)?;
        let reader = BufReader::new(file);
        let mut entries = Vec::new();

        for line in reader.lines() {
            let line = line?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let entry: SignalLogEntry = serde_json::from_str(trimmed).map_err(|e| {
                SignalLogError::Serialization(format!("failed to parse JSONL line: {e}"))
            })?;
            entries.push(entry);
        }

        Ok(entries)
    }

    /// Number of entries currently in the log (tracked in-memory, validated on open).
    pub fn len(&self) -> u64 {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Get the current fsync policy.
    pub fn policy(&self) -> &FsyncPolicy {
        &self.policy
    }

    fn write_entry(&mut self, entry: &SignalLogEntry) -> Result<(), SignalLogError> {
        let mut line = serde_json::to_string(entry).map_err(|e| {
            SignalLogError::Serialization(format!("failed to serialize entry: {e}"))
        })?;
        line.push('\n');
        self.writer
            .write_all(line.as_bytes())
            .map_err(SignalLogError::Io)?;
        Ok(())
    }

    fn fsync(&mut self) -> Result<(), SignalLogError> {
        self.writer.flush().map_err(SignalLogError::Io)?;
        let file = self.writer.get_ref();
        file.sync_all().map_err(SignalLogError::Io)?;
        Ok(())
    }

    fn recount(&mut self) -> Result<(), SignalLogError> {
        let file = OpenOptions::new().read(true).open(&self.path)?;
        let reader = BufReader::new(file);
        let mut count: u64 = 0;
        for line in reader.lines() {
            let line = line?;
            if !line.trim().is_empty() {
                count += 1;
            }
        }
        self.len = count;
        Ok(())
    }
}

impl Drop for SignalLog {
    fn drop(&mut self) {
        if matches!(self.policy, FsyncPolicy::OnShutdown) {
            let _ = self.fsync();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mcl_spec::label::LabelSet;
    use mcl_spec::signal::SignalKind;
    use mcl_spec::span::Span;
    use tempfile::NamedTempFile;

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

    #[test]
    fn append_and_read_roundtrip() {
        let tmp = NamedTempFile::new().expect("temp file");
        let path = tmp.path().to_path_buf();

        let mut log = SignalLog::open(&path, FsyncPolicy::EveryStep).expect("open");

        let entry = log
            .append(
                test_signal("attack"),
                NodeId::new("player"),
                1,
                "actions".to_string(),
                Verdict::Accept,
            )
            .expect("append");

        let entries = log.read().expect("read");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, entry.id);
        assert_eq!(entries[0].origin, NodeId::new("player"));
        assert_eq!(entries[0].turn, 1);
        assert_eq!(entries[0].channel, "actions");
        assert_eq!(entries[0].verdict, Verdict::Accept);
    }

    #[test]
    fn len_tracks_appends() {
        let tmp = NamedTempFile::new().expect("temp file");
        let path = tmp.path().to_path_buf();

        let mut log = SignalLog::open(&path, FsyncPolicy::OnShutdown).expect("open");

        assert!(log.is_empty());
        assert_eq!(log.len(), 0);

        log.append(
            test_signal("a"),
            NodeId::new("n1"),
            0,
            "ch".to_string(),
            Verdict::Accept,
        )
        .expect("append 1");
        assert_eq!(log.len(), 1);

        log.append(
            test_signal("b"),
            NodeId::new("n2"),
            1,
            "ch".to_string(),
            Verdict::Defer,
        )
        .expect("append 2");
        assert_eq!(log.len(), 2);

        log.append(
            test_signal("c"),
            NodeId::new("n3"),
            2,
            "ch".to_string(),
            Verdict::Reject {
                reason: "nope".to_string(),
            },
        )
        .expect("append 3");
        assert_eq!(log.len(), 3);
    }

    #[test]
    fn fsync_policy_every_n_steps() {
        let tmp = NamedTempFile::new().expect("temp file");
        let path = tmp.path().to_path_buf();

        let mut log = SignalLog::open(&path, FsyncPolicy::EveryNSteps(2)).expect("open");

        for i in 0..5 {
            log.append(
                test_signal("s"),
                NodeId::new("n"),
                i,
                "ch".to_string(),
                Verdict::Accept,
            )
            .expect("append");
        }

        assert_eq!(log.len(), 5);
        let entries = log.read().expect("read");
        assert_eq!(entries.len(), 5);
    }

    #[test]
    fn reopend_preserves_existing_entries() {
        let tmp = NamedTempFile::new().expect("temp file");
        let path = tmp.path().to_path_buf();

        {
            let mut log = SignalLog::open(&path, FsyncPolicy::OnShutdown).expect("open");
            log.append(
                test_signal("first"),
                NodeId::new("a"),
                1,
                "ch".to_string(),
                Verdict::Accept,
            )
            .expect("append");
        }

        {
            let mut log = SignalLog::open(&path, FsyncPolicy::OnShutdown).expect("reopen");
            assert_eq!(log.len(), 1);
            log.append(
                test_signal("second"),
                NodeId::new("b"),
                2,
                "ch".to_string(),
                Verdict::Defer,
            )
            .expect("append");
        }

        let log = SignalLog::open(&path, FsyncPolicy::OnShutdown).expect("reopen 2");
        assert_eq!(log.len(), 2);
        let entries = log.read().expect("read");
        assert_eq!(entries[0].origin, NodeId::new("a"));
        assert_eq!(entries[1].origin, NodeId::new("b"));
    }

    #[test]
    fn jsonl_format_is_valid() {
        let tmp = NamedTempFile::new().expect("temp file");
        let path = tmp.path().to_path_buf();

        let mut log = SignalLog::open(&path, FsyncPolicy::OnShutdown).expect("open");

        log.append(
            test_signal("x"),
            NodeId::new("n"),
            0,
            "ch".to_string(),
            Verdict::Accept,
        )
        .expect("append");

        let contents = std::fs::read_to_string(&path).expect("read file");
        for line in contents.lines() {
            let val: serde_json::Value =
                serde_json::from_str(line).expect("each line must be valid JSON");
            assert!(val.is_object());
            assert!(val.get("id").is_some());
            assert!(val.get("signal").is_some());
            assert!(val.get("origin").is_some());
            assert!(val.get("turn").is_some());
            assert!(val.get("channel").is_some());
            assert!(val.get("timestamp").is_some());
            assert!(val.get("verdict").is_some());
        }
    }

    #[test]
    fn all_verdict_types_roundtrip() {
        let tmp = NamedTempFile::new().expect("temp file");
        let path = tmp.path().to_path_buf();

        let mut log = SignalLog::open(&path, FsyncPolicy::OnShutdown).expect("open");

        log.append(
            test_signal("a"),
            NodeId::new("n"),
            0,
            "ch".to_string(),
            Verdict::Accept,
        )
        .expect("accept");

        log.append(
            test_signal("b"),
            NodeId::new("n"),
            1,
            "ch".to_string(),
            Verdict::Reject {
                reason: "blocked".to_string(),
            },
        )
        .expect("reject");

        log.append(
            test_signal("c"),
            NodeId::new("n"),
            2,
            "ch".to_string(),
            Verdict::Defer,
        )
        .expect("defer");

        let entries = log.read().expect("read");
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].verdict, Verdict::Accept);
        assert_eq!(
            entries[1].verdict,
            Verdict::Reject {
                reason: "blocked".to_string()
            }
        );
        assert_eq!(entries[2].verdict, Verdict::Defer);
    }
}
