//! `Authorization: Bearer <api key>` check for everything under `/api` except `/api/health`.

use axum::extract::{Request, State};
use axum::http::{header, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use subtle::ConstantTimeEq;

use crate::config::Secret;
use crate::error::ApiError;

pub async fn require_api_key(State(api_key): State<Secret>, request: Request, next: Next) -> Response {
    let presented = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(bearer_token);
    match presented {
        Some(token) if keys_match(token, api_key.expose()) => next.run(request).await,
        _ => {
            let mut response = ApiError::new(StatusCode::UNAUTHORIZED, "missing or invalid API key").into_response();
            response
                .headers_mut()
                .insert(header::WWW_AUTHENTICATE, "Bearer".parse().unwrap());
            response
        }
    }
}

fn bearer_token(header_value: &str) -> Option<&str> {
    let (scheme, token) = header_value.split_once(' ')?;
    scheme.eq_ignore_ascii_case("bearer").then(|| token.trim())
}

/// Constant-time for equal lengths; a length mismatch is rejected up front.
fn keys_match(presented: &str, expected: &str) -> bool {
    presented.len() == expected.len() && bool::from(presented.as_bytes().ct_eq(expected.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bearer_scheme_is_case_insensitive_and_required() {
        assert_eq!(bearer_token("Bearer abc"), Some("abc"));
        assert_eq!(bearer_token("bearer abc"), Some("abc"));
        assert_eq!(bearer_token("Basic abc"), None);
        assert_eq!(bearer_token("abc"), None);
    }

    #[test]
    fn keys_must_match_exactly() {
        assert!(keys_match("secret", "secret"));
        assert!(!keys_match("secre", "secret"));
        assert!(!keys_match("secreT", "secret"));
        assert!(!keys_match("", "secret"));
    }
}
