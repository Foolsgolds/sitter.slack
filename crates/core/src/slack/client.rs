//! Slack HTTP client + retry engine. Port of `SlackService.ExecuteWithRetryAsync`
//! and its retryability/backoff rules. All Slack Web API calls go through plain
//! `reqwest` (no SlackNet-equivalent crate) with a Bearer token attached.

use std::future::Future;
use std::time::Duration;

use serde_json::Value;

use crate::error::{AppError, AppResult};

pub const BASE_URL: &str = "https://slack.com/api";

/// Up to 3 retries (4 attempts total).
const MAX_RETRY_COUNT: u32 = 3;
/// Fallback wait when a 429 response has no `Retry-After` header.
const DEFAULT_RATE_LIMIT_WAIT_SECONDS: u64 = 30;

#[derive(Clone)]
pub struct SlackClient {
    http: reqwest::Client,
    token: String,
    base_url: String,
}

impl SlackClient {
    pub fn new(token: impl Into<String>) -> Self {
        Self {
            http: reqwest::Client::new(),
            token: token.into(),
            base_url: BASE_URL.to_string(),
        }
    }

    /// A client that talks to `base_url` (e.g. `http://127.0.0.1:9999/api`) instead of
    /// slack.com. For tests and local fakes only: the token is sent there.
    pub fn with_base_url(token: impl Into<String>, base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            ..Self::new(token)
        }
    }

    pub fn token(&self) -> &str {
        &self.token
    }

    /// GET a Slack API method with query params, returning the raw JSON body.
    /// Not retried by itself — callers decide whether to wrap in `with_retry`.
    pub async fn get(&self, method: &str, query: &[(&str, &str)]) -> AppResult<Value> {
        let response = self
            .http
            .get(format!("{}/{method}", self.base_url))
            .bearer_auth(&self.token)
            .query(query)
            .send()
            .await?;
        Self::handle_response(response).await
    }

    /// POST a Slack API method with a JSON body.
    pub async fn post_json(&self, method: &str, body: &Value) -> AppResult<Value> {
        let response = self
            .http
            .post(format!("{}/{method}", self.base_url))
            .bearer_auth(&self.token)
            .json(body)
            .send()
            .await?;
        Self::handle_response(response).await
    }

    async fn handle_response(response: reqwest::Response) -> AppResult<Value> {
        if response.status().as_u16() == 429 {
            let retry_after = response
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok());
            return Err(AppError::RateLimited(retry_after));
        }

        let status = response.status();
        let body: Value = response.json().await?;

        if !status.is_success() {
            return Err(AppError::Slack(format!("http {status}: {body}")));
        }

        let ok = body.get("ok").and_then(Value::as_bool).unwrap_or(false);
        if !ok {
            let error = body
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("unknown_error")
                .to_string();
            return Err(AppError::Slack(error));
        }

        Ok(body)
    }

    /// Runs `operation` with the legacy app's retry policy: retry on 429 /
    /// `ratelimited` / transient network errors, up to 3 retries. Wait time is
    /// the `Retry-After` value when present (or 30s default on an
    /// unqualified 429), else exponential backoff `2^attempt` seconds.
    pub async fn with_retry<F, Fut, T>(&self, mut operation: F) -> AppResult<T>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = AppResult<T>>,
    {
        let mut attempt = 0u32;
        loop {
            match operation().await {
                Ok(value) => return Ok(value),
                Err(err) if attempt < MAX_RETRY_COUNT && is_retryable(&err) => {
                    let wait = wait_seconds(&err, attempt);
                    tracing::warn!(attempt, wait_seconds = wait, %err, "retrying slack api call");
                    tokio::time::sleep(Duration::from_secs(wait)).await;
                    attempt += 1;
                }
                Err(err) => return Err(err),
            }
        }
    }
}

fn is_retryable(err: &AppError) -> bool {
    match err {
        AppError::RateLimited(_) => true,
        AppError::Network(e) => e.is_timeout() || e.is_connect() || e.is_request(),
        AppError::Slack(message) => message.to_ascii_lowercase().contains("ratelimited"),
        _ => false,
    }
}

fn wait_seconds(err: &AppError, attempt: u32) -> u64 {
    match err {
        AppError::RateLimited(Some(seconds)) => *seconds,
        AppError::RateLimited(None) => DEFAULT_RATE_LIMIT_WAIT_SECONDS,
        _ => 2u64.saturating_pow(attempt), // attempts 0..3 -> 1, 2, 4, 8 seconds
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_is_exponential_for_generic_errors() {
        let err = AppError::Slack("ratelimited".to_string());
        assert_eq!(wait_seconds(&err, 0), 1);
        assert_eq!(wait_seconds(&err, 1), 2);
        assert_eq!(wait_seconds(&err, 2), 4);
        assert_eq!(wait_seconds(&err, 3), 8);
    }

    #[test]
    fn rate_limited_prefers_retry_after_header_value() {
        let err = AppError::RateLimited(Some(12));
        assert_eq!(wait_seconds(&err, 0), 12);
    }

    #[test]
    fn rate_limited_falls_back_to_default_without_header() {
        let err = AppError::RateLimited(None);
        assert_eq!(wait_seconds(&err, 0), DEFAULT_RATE_LIMIT_WAIT_SECONDS);
    }

    #[test]
    fn ratelimited_message_is_retryable() {
        assert!(is_retryable(&AppError::Slack("Ratelimited".to_string())));
        assert!(!is_retryable(&AppError::Slack("missing_scope".to_string())));
    }

    #[tokio::test]
    async fn with_retry_stops_after_max_retries() {
        let client = SlackClient::new("xoxp-test");
        let mut calls = 0;
        let result: AppResult<()> = client
            .with_retry(|| {
                calls += 1;
                async { Err(AppError::RateLimited(Some(0))) }
            })
            .await;
        assert!(result.is_err());
        assert_eq!(calls, 4); // initial attempt + 3 retries
    }
}
