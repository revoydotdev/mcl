//! MCL Policy DSL compiler, evaluator, and type checker.
//!
//! This crate provides the policy compilation pipeline:
//! parse → type-check → compile → evaluate

pub mod ast;
pub mod compile;
pub mod error;
pub mod eval;
pub mod typeck;

#[cfg(test)]
mod tests {
    #[test]
    fn empty_crate_loads() {}
}
