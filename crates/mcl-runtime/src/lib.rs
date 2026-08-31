//! WorldState, prompt builder, context manager, and session management.
//!
//! Core runtime for managing MCL sessions, state, and knowledge boundaries.

pub mod audit;
pub mod context;
pub mod degradation;
pub mod effect_check;
pub mod knowledge_filter;
pub mod narrator_filter;
pub mod prompt;
pub mod resolver;
pub mod session;
pub mod snapshot;
pub mod state;

pub use audit::AuditLog;
pub use context::ContextManager;
pub use degradation::degrade_value;
pub use effect_check::{
    check_effect_compliance, check_effect_compliance_for_mode, infer_effects, EffectCheckResult,
    EffectViolation, Severity,
};
pub use knowledge_filter::filter_state_for_knowledge;
pub use knowledge_filter::{
    enforce_scope, filter_entries_by_scope, filter_with_registry, tag_entry_scope, Scope,
};
pub use resolver::{ActionResolver, ResolverRegistry};
pub use session::{Session, SessionConfig, SignalMode};
pub use snapshot::{BackendConfig, SessionSnapshot};
pub use state::WorldState;
