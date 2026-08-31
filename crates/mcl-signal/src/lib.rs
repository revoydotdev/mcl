//! Signal extraction from LLM output with retry logic.
//!
//! Extracts structured signals from LLM responses and validates them.

pub mod bench;
pub mod extract;
pub mod parse;
pub mod validate;

pub use bench::classify_turn;
pub use bench::{BenchmarkScenario, ReliabilityReport, TurnClassification, DEFAULT_SCENARIOS};
pub use extract::extract_signals;
pub use extract::extract_signals_from;
pub use extract::extract_signals_from_json;
pub use extract::{IncrementalExtractor, IncrementalUpdate};
pub use validate::validate_signal;
