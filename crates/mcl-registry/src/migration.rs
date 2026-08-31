//! Schema migration framework for on-disk registry data.
//!
//! Each migration transforms data from one schema version to the next.
//! The migration chain is ordered: v0→v1, v1→v2, etc.
//! The [`SchemaMigrator`] reads the current version from disk, runs any
//! pending migrations, and writes the new version.

use std::fs;
use std::io::Write;
use std::path::Path;

use thiserror::Error;

/// Errors that can occur during schema migration.
#[derive(Debug, Error)]
pub enum MigrationError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("version mismatch: expected {expected}, found {found}")]
    VersionMismatch { expected: u32, found: u32 },
    #[error("migration failed: {0}")]
    Custom(String),
}

/// A single schema migration step.
///
/// Implementations transform on-disk data from `from_version` to `to_version`.
pub trait Migration: Send + Sync {
    /// Schema version this migration starts from.
    fn source_version(&self) -> u32;
    /// Schema version this migration produces.
    fn target_version(&self) -> u32;
    /// Apply the migration to the data at `path`.
    fn up(&self, path: &Path) -> Result<(), MigrationError>;
    /// Human-readable description of what this migration does.
    fn description(&self) -> &str;
}

/// Identity migration v0 → v1.
///
/// Does nothing — proves the migration framework works without touching data.
pub struct IdentityV1Migration;

impl Migration for IdentityV1Migration {
    fn source_version(&self) -> u32 {
        0
    }

    fn target_version(&self) -> u32 {
        1
    }

    fn up(&self, _path: &Path) -> Result<(), MigrationError> {
        Ok(())
    }

    fn description(&self) -> &str {
        "identity v0→v1 (no-op)"
    }
}

const VERSION_FILE: &str = "schema-version";

/// Manages the ordered list of schema migrations and applies them to a
/// campaign directory.
pub struct SchemaMigrator {
    migrations: Vec<Box<dyn Migration>>,
}

impl SchemaMigrator {
    /// Build the default migrator with all known migrations.
    pub fn new() -> Self {
        Self {
            migrations: vec![Box::new(IdentityV1Migration)],
        }
    }

    /// Read the current schema version from `path/schema-version`.
    ///
    /// Returns `Ok(0)` if the file does not exist (fresh registry).
    pub fn current_version(&self, path: &Path) -> Result<u32, MigrationError> {
        let version_path = path.join(VERSION_FILE);
        if !version_path.exists() {
            return Ok(0);
        }
        let text = fs::read_to_string(&version_path)?;
        let trimmed = text.trim();
        trimmed
            .parse::<u32>()
            .map_err(|_| MigrationError::Custom(format!("invalid schema version: {trimmed:?}")))
    }

    /// Run all pending migrations and return the final schema version.
    pub fn run_migrations(&self, path: &Path) -> Result<u32, MigrationError> {
        let mut version = self.current_version(path)?;

        for migration in &self.migrations {
            if version == migration.source_version() {
                log::info!(
                    "migrating schema v{} → v{}: {}",
                    migration.source_version(),
                    migration.target_version(),
                    migration.description(),
                );
                migration.up(path)?;
                version = migration.target_version();
                Self::write_version(path, version)?;
            }
        }

        if !path.join(VERSION_FILE).exists() {
            Self::write_version(path, version)?;
        }

        Ok(version)
    }

    fn write_version(path: &Path, version: u32) -> Result<(), MigrationError> {
        let version_path = path.join(VERSION_FILE);
        let mut f = fs::File::create(&version_path)?;
        f.write_all(version.to_string().as_bytes())?;
        f.sync_all()?;
        Ok(())
    }
}

impl Default for SchemaMigrator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn migrator() -> SchemaMigrator {
        SchemaMigrator::new()
    }

    #[test]
    fn fresh_registry_gets_version_1() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let version = migrator().run_migrations(tmp.path()).expect("migrate");
        assert_eq!(version, 1);

        let on_disk = fs::read_to_string(tmp.path().join(VERSION_FILE)).expect("read version");
        assert_eq!(on_disk, "1");
    }

    #[test]
    fn migration_from_v0_to_v1_is_noop() {
        let tmp = tempfile::tempdir().expect("tempdir");

        let sentinel = tmp.path().join("sentinel.txt");
        fs::write(&sentinel, "untouched").expect("write sentinel");

        let version = migrator().run_migrations(tmp.path()).expect("migrate");
        assert_eq!(version, 1);

        assert_eq!(fs::read_to_string(&sentinel).expect("read"), "untouched");
    }

    #[test]
    fn already_at_v1_does_not_rerun() {
        let tmp = tempfile::tempdir().expect("tempdir");

        SchemaMigrator::write_version(tmp.path(), 1).expect("write v1");

        let version = migrator().run_migrations(tmp.path()).expect("migrate");
        assert_eq!(version, 1);

        let on_disk = fs::read_to_string(tmp.path().join(VERSION_FILE)).expect("read");
        assert_eq!(on_disk, "1");
    }

    #[test]
    fn version_file_written_correctly() {
        let tmp = tempfile::tempdir().expect("tempdir");

        assert!(!tmp.path().join(VERSION_FILE).exists());

        migrator().run_migrations(tmp.path()).expect("migrate");

        let contents = fs::read_to_string(tmp.path().join(VERSION_FILE)).expect("read");
        assert_eq!(contents, "1");
    }

    #[test]
    fn current_version_returns_zero_when_no_file() {
        let tmp = tempfile::tempdir().expect("tempdir");
        assert_eq!(migrator().current_version(tmp.path()).expect("version"), 0);
    }

    #[test]
    fn identity_v1_migration_trait_impl() {
        let m = IdentityV1Migration;
        assert_eq!(m.source_version(), 0);
        assert_eq!(m.target_version(), 1);
        assert_eq!(m.description(), "identity v0→v1 (no-op)");
    }
}
