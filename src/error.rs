//! Library-level error type. `RivError.code` maps to the process exit code.

use std::fmt;

/// Broad error category; doubles as the CLI exit code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    General,
    Validation,
    Auth,
    PlanRejected,
    PartialFailure,
}

impl ErrorCode {
    pub fn exit_code(self) -> i32 {
        match self {
            ErrorCode::General => 1,
            ErrorCode::Validation => 2,
            ErrorCode::Auth => 3,
            ErrorCode::PlanRejected => 4,
            ErrorCode::PartialFailure => 5,
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct RivError {
    pub code: ErrorCode,
    pub message: String,
}

impl RivError {
    pub fn new(code: ErrorCode, message: impl fmt::Display) -> Self {
        Self { code, message: message.to_string() }
    }
    pub fn general(message: impl fmt::Display) -> Self {
        Self::new(ErrorCode::General, message)
    }
    pub fn validation(message: impl fmt::Display) -> Self {
        Self::new(ErrorCode::Validation, message)
    }
    pub fn auth(message: impl fmt::Display) -> Self {
        Self::new(ErrorCode::Auth, message)
    }
    pub fn plan_rejected(message: impl fmt::Display) -> Self {
        Self::new(ErrorCode::PlanRejected, message)
    }
}

pub type Result<T> = std::result::Result<T, RivError>;

impl From<rusqlite::Error> for RivError {
    fn from(e: rusqlite::Error) -> Self {
        RivError::general(format!("database error: {e}"))
    }
}
impl From<std::io::Error> for RivError {
    fn from(e: std::io::Error) -> Self {
        RivError::general(format!("io error: {e}"))
    }
}
impl From<serde_json::Error> for RivError {
    fn from(e: serde_json::Error) -> Self {
        RivError::general(format!("json error: {e}"))
    }
}
