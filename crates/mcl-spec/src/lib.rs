//! Core types: AST, signals, values, expressions, spans.
//!
//! This crate defines the fundamental data structures used across MCL.

pub mod document;
pub mod effects;
pub mod error;
pub mod expression;
pub mod label;
pub mod narrator;
pub mod policy;
pub mod signal;
pub mod source;
pub mod span;
pub mod value;

pub use document::*;
pub use effects::*;
pub use error::MclError;
pub use expression::*;
pub use label::*;
pub use narrator::*;
pub use signal::*;
pub use source::*;
pub use span::Span;
pub use value::LabeledValue;
pub use value::MclValue;
