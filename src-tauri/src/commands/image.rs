//! Authenticated image proxy. Slack's private file/avatar URLs require a
//! Bearer token, which a webview cannot attach to a plain `<img src>` — so
//! every image the frontend shows is fetched here and handed back as a data
//! URL. Port of `MainWindow.DownloadSlackImageAsync` / `LoadInlineMessageBitmapAsync`.

use base64::Engine;
use tauri::State;

use slacksitter_core::error::{AppError, AppResult};
use slacksitter_core::slack::client::SlackClient;
use crate::state::AppState;

const MAX_REDIRECTS: u32 = 5;

/// Tries each candidate URL in priority order until one downloads
/// successfully as an `image/*` response; gated by the app-wide semaphore so
/// at most 2 full-size image downloads run concurrently.
#[tauri::command]
pub async fn fetch_image(state: State<'_, AppState>, candidate_urls: Vec<String>) -> AppResult<String> {
    let client = state
        .client
        .read()
        .unwrap()
        .clone()
        .ok_or(AppError::NotAuthenticated)?;

    let _permit = state.image_semaphore.acquire().await.unwrap();

    let mut last_error = AppError::Slack("no candidate urls provided".to_string());
    for url in &candidate_urls {
        match try_download_image(&client, url).await {
            Ok(data_url) => return Ok(data_url),
            Err(err) => last_error = err,
        }
    }
    Err(last_error)
}

async fn try_download_image(client: &SlackClient, start_url: &str) -> AppResult<String> {
    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()?;

    let mut current_url = start_url.to_string();
    for _ in 0..=MAX_REDIRECTS {
        let mut request = http.get(&current_url);
        // Only reattach the bearer token when the redirect target still looks
        // like a Slack host, so the token isn't leaked to arbitrary CDNs.
        if current_url.contains("slack") {
            request = request.bearer_auth(client.token());
        }
        let response = request.send().await?;
        let status = response.status();

        if status.is_redirection() {
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|v| v.to_str().ok())
                .ok_or_else(|| AppError::Slack("redirect response missing location header".to_string()))?
                .to_string();
            current_url = location;
            continue;
        }

        if !status.is_success() {
            return Err(AppError::Slack(format!("image download failed with status {status}")));
        }

        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        if !content_type.starts_with("image/") {
            return Err(AppError::Slack(format!("unexpected content-type: {content_type}")));
        }

        let bytes = response.bytes().await?;
        let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
        return Ok(format!("data:{content_type};base64,{encoded}"));
    }

    Err(AppError::Slack("exceeded maximum redirect count".to_string()))
}
