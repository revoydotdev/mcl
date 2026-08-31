#[derive(Debug, thiserror::Error)]
pub enum GraphError {
    #[error("unknown node type: {0}")]
    UnknownNodeType(String),

    #[error("node not found: {0}")]
    NodeNotFound(String),

    #[error("channel not found: {0}")]
    ChannelNotFound(String),

    #[error("node error: {0}")]
    NodeError(String),

    #[error("graph definition error: {0}")]
    DefError(String),

    #[error("execution error: {0}")]
    Execution(String),

    #[error("io error: {0}")]
    Io(String),
}

impl From<std::io::Error> for GraphError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e.to_string())
    }
}

impl From<mcl_spec::MclError> for GraphError {
    fn from(e: mcl_spec::MclError) -> Self {
        Self::NodeError(e.to_string())
    }
}
