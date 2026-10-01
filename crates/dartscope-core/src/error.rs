//! The error type of the crate.

use thiserror::Error;

#[derive(Debug, Error, Clone, Eq, PartialEq)]
pub enum DartScopeError {
    #[error("I/O error: {0}")]
    Io(String),
    #[error("JSON error: {0}")]
    Json(String),
}
