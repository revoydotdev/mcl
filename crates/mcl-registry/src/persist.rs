//! Cross-session persistence with file locking.
//!
//! [`RegistryPersistence`] manages the on-disk layout for a campaign registry,
//! coordinating directory creation, schema versioning, and exclusive write access
//! via POSIX file locks (through `fd-lock`).

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

use thiserror::Error;

const SCHEMA_VERSION: &str = "1";

#[derive(Debug, Error)]
pub enum PersistError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("lock acquisition failed: {0}")]
    LockFailed(String),
}

/// Manages the `.mcl/registry/<campaign_id>/` directory and holds an exclusive
/// write lock to enforce single-writer discipline across sessions.
///
/// The POSIX advisory flock is held as long as the underlying file descriptor
/// is open. Dropping this struct (or calling [`close`](RegistryPersistence::close))
/// closes the fd and releases the lock.
#[derive(Debug)]
pub struct RegistryPersistence {
    base_dir: PathBuf,
    campaign_id: String,
    /// Holds the lock file open. The advisory flock is released when this
    /// is dropped (fd closed). The write-lock guard was `mem::forget`-ten
    /// to prevent explicit unlock — the fd-close path is the release mechanism.
    lock: Option<fd_lock::RwLock<File>>,
}

impl RegistryPersistence {
    pub fn signals_path(&self) -> PathBuf {
        self.base_dir.join("signals.jsonl")
    }

    pub fn causality_path(&self) -> PathBuf {
        self.base_dir.join("causality.jsonl")
    }

    pub fn index_path(&self) -> PathBuf {
        self.base_dir.join("signals.usearch")
    }

    pub fn meta_path(&self) -> PathBuf {
        self.base_dir.join("signals.meta.json")
    }

    pub fn summaries_dir(&self) -> PathBuf {
        self.base_dir.join("summaries")
    }

    pub fn campaign_id(&self) -> &str {
        &self.campaign_id
    }

    pub fn base_dir(&self) -> &PathBuf {
        &self.base_dir
    }

    pub fn open(campaign_id: &str) -> Result<Self, PersistError> {
        let base_dir = PathBuf::from(".mcl").join("registry").join(campaign_id);
        Self::open_at(base_dir, campaign_id)
    }

    #[cfg(test)]
    fn open_in(root: &std::path::Path, campaign_id: &str) -> Result<Self, PersistError> {
        let base_dir = root.join("registry").join(campaign_id);
        Self::open_at(base_dir, campaign_id)
    }

    fn open_at(base_dir: PathBuf, campaign_id: &str) -> Result<Self, PersistError> {
        fs::create_dir_all(&base_dir)?;
        fs::create_dir_all(base_dir.join("summaries"))?;

        let version_path = base_dir.join("schema-version");
        if !version_path.exists() {
            let mut f = File::create(&version_path)?;
            f.write_all(SCHEMA_VERSION.as_bytes())?;
            f.sync_all()?;
        }

        let lock_path = base_dir.join("lock");
        let lock_file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .read(true)
            .write(true)
            .open(&lock_path)?;
        let mut lock = fd_lock::RwLock::new(lock_file);

        let guard = lock.try_write().map_err(|_| {
            PersistError::LockFailed(format!(
                "campaign '{}' is locked by another session",
                campaign_id
            ))
        })?;

        // Prevent the guard from calling unlock() on drop. The POSIX advisory
        // flock stays held until the file descriptor is closed (when `lock`
        // is dropped). This avoids the self-referential struct problem.
        std::mem::forget(guard);

        Ok(Self {
            base_dir,
            campaign_id: campaign_id.to_string(),
            lock: Some(lock),
        })
    }

    pub fn flush(&self) -> Result<(), PersistError> {
        Ok(())
    }

    pub fn close(mut self) -> Result<(), PersistError> {
        self.lock.take();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_base(tmp: &tempfile::TempDir, campaign: &str) -> PathBuf {
        tmp.path().join("registry").join(campaign)
    }

    #[test]
    fn open_creates_directory_structure() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let _persist = RegistryPersistence::open_in(tmp.path(), "test-campaign").expect("open");

        let base = test_base(&tmp, "test-campaign");
        assert!(base.is_dir());
        assert!(base.join("schema-version").is_file());
        assert!(base.join("summaries").is_dir());
        assert!(base.join("lock").is_file());
    }

    #[test]
    fn open_writes_schema_version() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let _persist = RegistryPersistence::open_in(tmp.path(), "sv-test").expect("open");

        let version = std::fs::read_to_string(test_base(&tmp, "sv-test").join("schema-version"))
            .expect("read version");
        assert_eq!(version.trim(), "1");
    }

    #[test]
    fn open_acquires_lock_second_open_fails() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let _persist1 = RegistryPersistence::open_in(tmp.path(), "lock-test").expect("first open");

        let err = RegistryPersistence::open_in(tmp.path(), "lock-test")
            .expect_err("second open should fail");
        assert!(
            err.to_string().contains("locked"),
            "error should mention locking"
        );
    }

    #[test]
    fn close_releases_lock_second_open_succeeds() {
        let tmp = tempfile::tempdir().expect("tempdir");
        {
            let persist =
                RegistryPersistence::open_in(tmp.path(), "close-test").expect("first open");
            persist.close().expect("close");
        }

        let _persist2 = RegistryPersistence::open_in(tmp.path(), "close-test")
            .expect("second open after close");
    }

    #[test]
    fn data_persists_across_sessions() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let signals_path = test_base(&tmp, "persist-test").join("signals.jsonl");

        {
            let _persist =
                RegistryPersistence::open_in(tmp.path(), "persist-test").expect("session 1");
            fs::write(&signals_path, "{\"session\":1}\n").expect("write");
        }

        {
            let persist =
                RegistryPersistence::open_in(tmp.path(), "persist-test").expect("session 2");
            assert_eq!(persist.signals_path(), signals_path);
            use std::io::Write as IoWrite;
            let mut f = OpenOptions::new()
                .append(true)
                .open(&signals_path)
                .expect("open for append");
            f.write_all(b"{\"session\":2}\n").expect("write");
        }

        let contents = fs::read_to_string(&signals_path).expect("read");
        let lines: Vec<&str> = contents.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("\"session\":1"));
        assert!(lines[1].contains("\"session\":2"));
    }

    #[test]
    fn drop_releases_lock() {
        let tmp = tempfile::tempdir().expect("tempdir");
        {
            let _persist = RegistryPersistence::open_in(tmp.path(), "drop-test").expect("open");
        }

        let _persist2 =
            RegistryPersistence::open_in(tmp.path(), "drop-test").expect("open after drop");
    }

    #[test]
    fn path_methods_return_correct_paths() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let persist = RegistryPersistence::open_in(tmp.path(), "path-test").expect("open");

        let base = test_base(&tmp, "path-test");
        assert_eq!(persist.signals_path(), base.join("signals.jsonl"));
        assert_eq!(persist.causality_path(), base.join("causality.jsonl"));
        assert_eq!(persist.index_path(), base.join("signals.usearch"));
        assert_eq!(persist.meta_path(), base.join("signals.meta.json"));
        assert_eq!(persist.summaries_dir(), base.join("summaries"));
        assert_eq!(persist.campaign_id(), "path-test");
    }
}
