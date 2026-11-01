//! Authenticated image proxy. Slack's private file URLs need the Bearer token,
//! which a webview cannot attach to `<img src>`, so the client asks the server.
//!
//! The token is only sent to Slack's own hosts (re-checked on every redirect hop),
//! at most 2 downloads run at once, and requests aimed at loopback / private
//! addresses are refused so the proxy cannot be used to probe the server's network.

use std::net::{Ipv4Addr, Ipv6Addr};
use std::time::Duration;

use axum::body::Bytes;
use axum::http::StatusCode;
use tokio::sync::Semaphore;
use url::{Host, Url};

use crate::config::Secret;
use crate::error::{ApiError, ApiResult};

const MAX_REDIRECTS: usize = 5;
const MAX_CONCURRENT_DOWNLOADS: usize = 2;
const MAX_IMAGE_BYTES: u64 = 25 * 1024 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

pub struct ImageProxy {
    http: reqwest::Client,
    token: Secret,
    permits: Semaphore,
}

pub struct FetchedImage {
    pub content_type: String,
    pub bytes: Bytes,
}

impl ImageProxy {
    pub fn new(token: Secret) -> Self {
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(REQUEST_TIMEOUT)
            .build()
            .expect("reqwest client");
        Self {
            http,
            token,
            permits: Semaphore::new(MAX_CONCURRENT_DOWNLOADS),
        }
    }

    pub async fn fetch(&self, raw_url: &str) -> ApiResult<FetchedImage> {
        let mut current = parse_fetchable(raw_url)?;
        let _permit = self.permits.acquire().await.expect("semaphore is never closed");

        for _ in 0..=MAX_REDIRECTS {
            let mut request = self.http.get(current.clone());
            if wants_slack_token(&current) {
                request = request.bearer_auth(self.token.expose());
            }
            let response = request.send().await.map_err(|err| {
                tracing::warn!(host = current.host_str().unwrap_or(""), "image download failed: {err}");
                ApiError::new(StatusCode::BAD_GATEWAY, "could not download the image")
            })?;
            let status = response.status();

            if status.is_redirection() {
                let location = response
                    .headers()
                    .get(reqwest::header::LOCATION)
                    .and_then(|v| v.to_str().ok())
                    .ok_or_else(|| ApiError::new(StatusCode::BAD_GATEWAY, "redirect without a location"))?;
                let next = current
                    .join(location)
                    .map_err(|_| ApiError::new(StatusCode::BAD_GATEWAY, "invalid redirect location"))?;
                current = parse_fetchable(next.as_str())?;
                continue;
            }
            if !status.is_success() {
                return Err(ApiError::new(
                    StatusCode::BAD_GATEWAY,
                    format!("image download failed with status {status}"),
                ));
            }

            let content_type = response
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_string();
            if !content_type.to_ascii_lowercase().starts_with("image/") {
                return Err(ApiError::new(
                    StatusCode::UNSUPPORTED_MEDIA_TYPE,
                    format!("not an image: {content_type}"),
                ));
            }
            if response.content_length().is_some_and(|len| len > MAX_IMAGE_BYTES) {
                return Err(ApiError::new(StatusCode::PAYLOAD_TOO_LARGE, "image is too large"));
            }
            let bytes = response
                .bytes()
                .await
                .map_err(|_| ApiError::new(StatusCode::BAD_GATEWAY, "could not read the image"))?;
            if bytes.len() as u64 > MAX_IMAGE_BYTES {
                return Err(ApiError::new(StatusCode::PAYLOAD_TOO_LARGE, "image is too large"));
            }
            return Ok(FetchedImage { content_type, bytes });
        }
        Err(ApiError::new(StatusCode::BAD_GATEWAY, "too many redirects"))
    }
}

/// Parses `raw` and refuses anything but http(s) to a public-looking host.
pub(crate) fn parse_fetchable(raw: &str) -> ApiResult<Url> {
    let url = Url::parse(raw).map_err(|_| ApiError::bad_request("invalid image url"))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(ApiError::bad_request("only http(s) image urls are allowed"));
    }
    if is_blocked_host(&url) {
        return Err(ApiError::bad_request("this host is not allowed"));
    }
    Ok(url)
}

/// The token goes to `slack.com` and its subdomains, over https only.
pub(crate) fn wants_slack_token(url: &Url) -> bool {
    url.scheme() == "https"
        && match url.host() {
            Some(Host::Domain(domain)) => {
                let domain = domain.to_ascii_lowercase();
                domain == "slack.com" || domain.ends_with(".slack.com")
            }
            _ => false,
        }
}

fn is_blocked_host(url: &Url) -> bool {
    match url.host() {
        None => true,
        Some(Host::Domain(domain)) => {
            let domain = domain.to_ascii_lowercase();
            domain == "localhost" || domain.ends_with(".localhost")
        }
        Some(Host::Ipv4(ip)) => is_blocked_v4(ip),
        Some(Host::Ipv6(ip)) => is_blocked_v6(ip),
    }
}

fn is_blocked_v4(ip: Ipv4Addr) -> bool {
    ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_unspecified()
        || ip.is_broadcast()
        || ip.is_documentation()
        || ip.octets()[0] == 0
        || (ip.octets()[0] == 100 && (64..128).contains(&ip.octets()[1])) // 100.64.0.0/10 (CGNAT)
}

fn is_blocked_v6(ip: Ipv6Addr) -> bool {
    if let Some(mapped) = ip.to_ipv4_mapped() {
        return is_blocked_v4(mapped);
    }
    let first = ip.segments()[0];
    ip.is_loopback()
        || ip.is_unspecified()
        || (first & 0xfe00) == 0xfc00 // fc00::/7 unique local
        || (first & 0xffc0) == 0xfe80 // fe80::/10 link local
}

#[cfg(test)]
mod tests {
    use super::*;

    fn allowed(url: &str) -> bool {
        parse_fetchable(url).is_ok()
    }

    #[test]
    fn only_public_http_urls_are_fetchable() {
        assert!(allowed("https://files.slack.com/files-pri/T1-F1/a.png"));
        assert!(allowed("https://emoji.slack-edge.com/T1/party/abc.png"));
        assert!(allowed("http://example.com/a.png"));

        assert!(!allowed("file:///etc/passwd"));
        assert!(!allowed("ftp://example.com/a.png"));
        assert!(!allowed("not a url"));
        assert!(!allowed("http://localhost:8787/api/channels"));
        assert!(!allowed("http://app.localhost/x"));
        assert!(!allowed("http://127.0.0.1:8787/api/health"));
        assert!(!allowed("http://10.0.0.5/x"));
        assert!(!allowed("http://192.168.1.1/x"));
        assert!(!allowed("http://172.16.0.1/x"));
        assert!(!allowed("http://169.254.169.254/latest/meta-data"));
        assert!(!allowed("http://0.0.0.0/x"));
        assert!(!allowed("http://[::1]/x"));
        assert!(!allowed("http://[fd00::1]/x"));
        assert!(!allowed("http://[::ffff:127.0.0.1]/x"));
    }

    #[test]
    fn the_token_is_only_for_slack_hosts_over_https() {
        let token_for = |u: &str| wants_slack_token(&Url::parse(u).unwrap());
        assert!(token_for("https://files.slack.com/a.png"));
        assert!(token_for("https://slack.com/a.png"));
        assert!(token_for("https://FILES.SLACK.COM/a.png"));
        assert!(!token_for("http://files.slack.com/a.png"));
        assert!(!token_for("https://slack.com.evil.example/a.png"));
        assert!(!token_for("https://evilslack.com/a.png"));
        assert!(!token_for("https://example.com/slack.com/a.png"));
        assert!(!token_for("https://avatars.slack-edge.com/a.png"));
    }
}
