//! Language parser: lexer, parser, validator, and emitter.
//!
//! Parses, validates, and emits the MCL language format.

pub mod ast;
pub mod constraints;
pub mod emit;
pub mod lexer;
pub mod parser;
pub mod validate;

pub use ast::MclFile;
pub use constraints::{
    compile_constraints_to_prompt, parse_constraint_tags, ConstraintTag, ConstraintTagType,
};
pub use emit::{emit_graph_def, emit_mcl_document};
pub use parser::Parser;
pub use validate::validate;
