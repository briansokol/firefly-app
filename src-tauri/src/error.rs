use serde::{Serialize, Serializer};

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),
    #[error("keychain error: {0}")]
    Keychain(#[from] keyring::Error),
    #[error("{0}")]
    KeychainUnavailable(String),
    #[error("network error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("no API token set")]
    NoToken,
    #[error("{0}")]
    Other(String),
}

// Commands must return a serializable error so the frontend can display it.
impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

pub type Result<T> = std::result::Result<T, AppError>;
