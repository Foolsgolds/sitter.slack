use serde::Serialize;

/// Errors of the Tauri shell (local settings, boards and logs). Slack and the
/// database live in the server now, so there is nothing network-related here.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("persistence error: {0}")]
    Persistence(String),
    #[error("credential store error: {0}")]
    Credential(String),
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;
