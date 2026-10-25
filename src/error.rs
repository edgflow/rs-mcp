//! The application error type.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    /// A required setting is missing or invalid (startup only).
    #[error("configuration error: {0}")]
    Config(String),

    /// The caller sent something the server cannot accept; the message says
    /// what to change.
    #[error("invalid input: {0}")]
    InvalidInput(String),

    /// The requested record does not exist.
    #[error("{0}")]
    NotFound(String),

    /// The database failed. The details are logged, never sent to clients.
    #[error("database error")]
    Database(#[from] sqlx::Error),
}
