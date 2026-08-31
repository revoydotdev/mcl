#![warn(clippy::unwrap_used)]
//! Signal registry for MCL — append-only JSONL log with content-addressed entries.
//!
//! Provides [`SignalLog`] for writing/reading entries, [`RegistryHandle`] for
//! thread-safe access, [`CausalityGraph`] for tracking cause-effect relationships,
//! [`Registry`] for unified querying, and core types ([`SignalLogEntry`], [`Verdict`], [`SignalId`]).

mod arbiter;
mod causality;
mod handle;
mod idempotency;
mod index;
mod label_arbiter;
mod log;
mod migration;
mod nodes;
mod persist;
mod policy_arbiter;
mod pools;
mod query;
mod recovery;
mod source_arbiter;
mod telemetry;
mod types;

pub use arbiter::{
    ActionWhitelistArbiter, Arbiter, ArbiterChain, DedupArbiter, HallucinationArbiter,
    IngestContext, SchemaArbiter,
};
pub use causality::CausalityGraph;
pub use handle::{RegistryHandle, RegistryInner};
pub use idempotency::{IdempotencyTracker, IdempotencyViolation};
pub use index::RegistryIndex;
pub use label_arbiter::LabelArbiter;
pub use log::{SignalLog, SignalLogError};
pub use migration::{Migration, MigrationError, SchemaMigrator};
pub use persist::{PersistError, RegistryPersistence};
pub use policy_arbiter::PolicyArbiter;
pub use pools::{MemoryPoolRegistry, PoolError, DEFAULT_MAX_POOL_SIZE};
pub use query::{QueryResult, Registry, RegistryError, RegistryQuery};
pub use recovery::{
    rebuild_index, repair_jsonl, verify_consistency, RecoveryError, RepairReport, VerifyReport,
};
pub use source_arbiter::SourceArbiter;
pub use telemetry::{RegistryStats, TelemetryAggregator};
pub use types::{FsyncPolicy, SignalId, SignalLogEntry, Verdict};

pub use nodes::scheduler::{
    NodeState, SchedulerNode, DEFAULT_MAX_CONSECUTIVE_FAILURES, DEFAULT_MAX_WAIT,
    DEFAULT_TIMEOUT_STEPS,
};
pub use nodes::summarizer::{RollupPolicy, SummarizerNode, SummaryEntry, SummaryLevel};
