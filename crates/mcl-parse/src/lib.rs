//! Legacy .mcl file parser for backward compatibility.
//!
//! Parses the original .mcl format into the MCL AST.

pub mod blocks;
pub mod conditions;
pub mod document;
pub mod expr;
pub mod frontmatter;
pub mod inline;
pub mod lexer;
pub mod policy;

pub use document::{parse_document, parse_file};
