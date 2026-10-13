//! The one error type of the backend.

use serde::{Serialize, Serializer};

/// Every failure the backend can report. The frontend gets it as a plain message.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error(transparent)]
    Keychain(#[from] keyring::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Message(String),
}

impl From<&str> for AppError {
    fn from(message: &str) -> Self {
        Self::Message(message.to_owned())
    }
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;
