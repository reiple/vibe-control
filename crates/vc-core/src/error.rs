use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("Validation failed: {0}")]
    Validation(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Ambiguous: {0}")]
    Ambiguous(String),

    #[error("Corrupt data: {0}")]
    Corrupt(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

impl CoreError {
    pub fn validation(msg: impl Into<String>) -> Self {
        Self::Validation(msg.into())
    }

    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::NotFound(msg.into())
    }

    pub fn ambiguous(msg: impl Into<String>) -> Self {
        Self::Ambiguous(msg.into())
    }

    pub fn corrupt(msg: impl Into<String>) -> Self {
        Self::Corrupt(msg.into())
    }

    pub fn permission_denied(msg: impl Into<String>) -> Self {
        Self::PermissionDenied(msg.into())
    }
}

pub type Result<T> = std::result::Result<T, CoreError>;
