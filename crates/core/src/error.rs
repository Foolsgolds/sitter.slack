use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("not authenticated")]
    NotAuthenticated,
    #[error("slack api error: {0}")]
    Slack(String),
    #[error("rate limited, retry after {0:?}s")]
    RateLimited(Option<u64>),
    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("persistence error: {0}")]
    Persistence(String),
    #[error("credential store error: {0}")]
    Credential(String),
    #[error("database error: {0}")]
    Database(String),
}

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        AppError::Database(err.to_string())
    }
}

impl From<sqlx::migrate::MigrateError> for AppError {
    fn from(err: sqlx::migrate::MigrateError) -> Self {
        AppError::Database(format!("migration failed: {err}"))
    }
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
