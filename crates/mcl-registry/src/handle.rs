//! Thread-safe registry handle wrapping the internal state.

use std::sync::{Arc, RwLock};

use mcl_graph::id::NodeId;
use mcl_spec::signal::MclSignal;

use crate::log::{SignalLog, SignalLogError};
use crate::types::{FsyncPolicy, SignalLogEntry, Verdict};

/// Internal state held by the registry handle.
pub struct RegistryInner {
    log: SignalLog,
}

/// Thread-safe handle to the signal registry.
///
/// Wraps the internal state in `Arc<RwLock>` so it can be shared across
/// threads (e.g., between the executor and arbiter nodes).
pub struct RegistryHandle {
    inner: Arc<RwLock<RegistryInner>>,
}

impl RegistryHandle {
    /// Create a new registry backed by a JSONL file at `path`.
    pub fn new(path: &std::path::Path, policy: FsyncPolicy) -> Result<Self, SignalLogError> {
        let log = SignalLog::open(path, policy)?;
        Ok(Self {
            inner: Arc::new(RwLock::new(RegistryInner { log })),
        })
    }

    /// Append a signal to the registry log.
    pub fn append(
        &self,
        signal: MclSignal,
        origin: NodeId,
        turn: u64,
        channel: String,
        verdict: Verdict,
    ) -> Result<SignalLogEntry, SignalLogError> {
        let mut inner = self.inner.write().expect("registry lock poisoned");
        inner.log.append(signal, origin, turn, channel, verdict)
    }

    /// Read all entries from the registry log.
    pub fn read(&self) -> Result<Vec<SignalLogEntry>, SignalLogError> {
        let inner = self.inner.read().expect("registry lock poisoned");
        inner.log.read()
    }

    /// Number of entries in the registry log.
    pub fn len(&self) -> u64 {
        let inner = self.inner.read().expect("registry lock poisoned");
        inner.log.len()
    }

    /// Whether the registry log is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Clone this handle (shares the same underlying state).
    pub fn clone_handle(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
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
    fn handle_append_and_read() {
        let tmp = NamedTempFile::new().expect("temp file");
        let path = tmp.path().to_path_buf();

        let handle = RegistryHandle::new(&path, FsyncPolicy::OnShutdown).expect("new");

        handle
            .append(
                test_signal("attack"),
                NodeId::new("player"),
                1,
                "actions".to_string(),
                Verdict::Accept,
            )
            .expect("append");

        let entries = handle.read().expect("read");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].origin, NodeId::new("player"));
    }

    #[test]
    fn handle_clone_shares_state() {
        let tmp = NamedTempFile::new().expect("temp file");
        let path = tmp.path().to_path_buf();

        let handle = RegistryHandle::new(&path, FsyncPolicy::OnShutdown).expect("new");

        handle
            .append(
                test_signal("a"),
                NodeId::new("n1"),
                0,
                "ch".to_string(),
                Verdict::Accept,
            )
            .expect("append 1");

        let cloned = handle.clone_handle();

        cloned
            .append(
                test_signal("b"),
                NodeId::new("n2"),
                1,
                "ch".to_string(),
                Verdict::Defer,
            )
            .expect("append 2");

        assert_eq!(handle.len(), 2);
        assert_eq!(cloned.len(), 2);

        let entries = handle.read().expect("read");
        assert_eq!(entries[0].origin, NodeId::new("n1"));
        assert_eq!(entries[1].origin, NodeId::new("n2"));
    }
}
