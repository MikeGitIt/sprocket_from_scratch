use thiserror::Error;

#[derive(Error, Debug)]
pub enum SprocketError {
    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Execution error: {0}")]
    ExecutionError(String),

    #[error("Variable not found: {0}")]
    VariableNotFound(String),

    #[error("Type mismatch: expected {expected}, got {actual}")]
    TypeMismatch { expected: String, actual: String },

    #[error("Task not found: {0}")]
    TaskNotFound(String),

    #[error("Workflow not found: {0}")]
    WorkflowNotFound(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("Command execution failed: {0}")]
    CommandFailed(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Workflow execution error: {0}")]
    WorkflowExecutionError(String),

    #[error("Cache error: {0}")]
    CacheError(String),
}

pub type Result<T> = std::result::Result<T, SprocketError>;
