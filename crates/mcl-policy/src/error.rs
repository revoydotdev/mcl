use thiserror::Error;

/// Errors from the policy compilation pipeline.
#[derive(Error, Debug)]
pub enum PolicyError {
    #[error("type error: {0}")]
    TypeErr(String),

    #[error("evaluation error: {0}")]
    EvalErr(String),

    #[error("compile error: {0}")]
    CompileErr(String),
}
