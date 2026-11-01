//! API errors: a status code and `{ "error": "..." }`. Messages never carry
//! secrets (Slack errors are short codes; internal failures are logged, not echoed).

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;
use slacksitter_core::error::AppError;
use slacksitter_core::sync::CycleError;

#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub message: String,
}

impl ApiError {
    pub fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, message)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(json!({ "error": self.message }))).into_response()
    }
}

impl From<AppError> for ApiError {
    fn from(err: AppError) -> Self {
        match &err {
            AppError::Slack(message) => {
                let not_found = message.starts_with("channel not found")
                    || message.contains("not in the catalog")
                    || matches!(message.as_str(), "user_not_found" | "channel_not_found" | "message_not_found");
                if not_found {
                    ApiError::not_found(message.clone())
                } else if message.starts_with("message text must not be empty") {
                    ApiError::bad_request(message.clone())
                } else {
                    ApiError::new(StatusCode::BAD_GATEWAY, message.clone())
                }
            }
            AppError::RateLimited(_) => ApiError::new(StatusCode::TOO_MANY_REQUESTS, err.to_string()),
            AppError::Network(_) => ApiError::new(StatusCode::BAD_GATEWAY, "network error talking to Slack"),
            AppError::NotAuthenticated => ApiError::new(StatusCode::UNAUTHORIZED, err.to_string()),
            _ => {
                tracing::error!("internal error: {err}");
                ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "internal error")
            }
        }
    }
}

impl From<CycleError> for ApiError {
    fn from(err: CycleError) -> Self {
        match err {
            CycleError::AlreadyRunning => ApiError::new(StatusCode::CONFLICT, err.to_string()),
            CycleError::Failed(_) => ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }
}

pub type ApiResult<T> = Result<T, ApiError>;
