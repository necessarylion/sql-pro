//! The one error type of the backend.

use serde::{Serialize, Serializer};

use crate::constants::QUERY_CANCELLED;

/// Every failure the backend can report. The frontend gets it as a plain message.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{}", database_message(.0))]
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

/// The text of a database error. A query that the user cancelled gets a short,
/// plain text instead of the long message of the server.
fn database_message(error: &sqlx::Error) -> String {
    if let sqlx::Error::Database(database) = error {
        let code = database.code();
        let cancelled = match code.as_deref() {
            // MySQL and MariaDB: "Query execution was interrupted". SQLite: SQLITE_INTERRUPT.
            Some("70100" | "9") => true,
            // PostgreSQL. A statement timeout has the same code, but another message.
            Some("57014") => database.message().contains("user request"),
            _ => false,
        };
        if cancelled {
            return QUERY_CANCELLED.to_owned();
        }
    }
    error.to_string()
}
