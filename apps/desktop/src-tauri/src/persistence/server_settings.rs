//! Connection to `slacksitter-server`: the URL lives in a JSON file next to the
//! custom boards, the API key in the OS credential store (never on disk in
//! plaintext, never logged).

use std::fs;
use std::path::PathBuf;

use keyring::Entry;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::error::{AppError, AppResult};

const FILE_NAME: &str = "server-settings.json";
const SERVICE: &str = "SlackSitter";
const USERNAME: &str = "server_api_key";

/// What the frontend needs to talk to the server.
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerSettings {
    pub server_url: String,
    pub api_key: String,
}

// The key must not show up if this is ever logged with `{:?}`.
impl std::fmt::Debug for ServerSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerSettings")
            .field("server_url", &self.server_url)
            .field("api_key", &"***")
            .finish()
    }
}

/// What is written to the settings file (the key is not part of it).
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredSettings {
    server_url: String,
}

fn entry() -> AppResult<Entry> {
    Entry::new(SERVICE, USERNAME).map_err(|e| AppError::Credential(e.to_string()))
}

fn settings_dir(app: &AppHandle) -> AppResult<PathBuf> {
    app.path()
        .app_data_dir()
        .map_err(|e| AppError::Persistence(e.to_string()))
}

/// Trims, requires http(s) and drops trailing slashes. `http://host:8787/` -> `http://host:8787`.
pub fn normalize_server_url(raw: &str) -> AppResult<String> {
    let trimmed = raw.trim().trim_end_matches('/');
    let lower = trimmed.to_ascii_lowercase();
    let has_host = lower
        .strip_prefix("http://")
        .or_else(|| lower.strip_prefix("https://"))
        .is_some_and(|rest| !rest.is_empty());
    if !has_host {
        return Err(AppError::Persistence(
            "server url must start with http:// or https://".to_string(),
        ));
    }
    Ok(trimmed.to_string())
}

pub fn load(app: &AppHandle) -> AppResult<Option<ServerSettings>> {
    let path = settings_dir(app)?.join(FILE_NAME);
    let Some(stored) = read_stored(&path)? else {
        return Ok(None);
    };
    match entry()?.get_password() {
        Ok(api_key) if !api_key.is_empty() => Ok(Some(ServerSettings {
            server_url: stored.server_url,
            api_key,
        })),
        Ok(_) | Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(AppError::Credential(e.to_string())),
    }
}

pub fn save(app: &AppHandle, server_url: &str, api_key: &str) -> AppResult<()> {
    let server_url = normalize_server_url(server_url)?;
    if api_key.trim().is_empty() {
        return Err(AppError::Persistence("api key must not be empty".to_string()));
    }

    let dir = settings_dir(app)?;
    fs::create_dir_all(&dir).map_err(|e| AppError::Persistence(e.to_string()))?;
    entry()?
        .set_password(api_key.trim())
        .map_err(|e| AppError::Credential(e.to_string()))?;
    write_stored(&dir.join(FILE_NAME), &StoredSettings { server_url })
}

pub fn clear(app: &AppHandle) -> AppResult<()> {
    let path = settings_dir(app)?.join(FILE_NAME);
    if path.exists() {
        fs::remove_file(&path).map_err(|e| AppError::Persistence(e.to_string()))?;
    }
    match entry()?.delete_password() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(AppError::Credential(e.to_string())),
    }
}

/// A missing or unparsable file means "not configured", so a damaged file
/// sends the user back to the connection screen instead of blocking startup.
fn read_stored(path: &std::path::Path) -> AppResult<Option<StoredSettings>> {
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(path).map_err(|e| AppError::Persistence(e.to_string()))?;
    Ok(serde_json::from_str::<StoredSettings>(&text)
        .ok()
        .filter(|stored| !stored.server_url.trim().is_empty()))
}

fn write_stored(path: &std::path::Path, stored: &StoredSettings) -> AppResult<()> {
    let json = serde_json::to_string_pretty(stored).map_err(|e| AppError::Persistence(e.to_string()))?;
    fs::write(path, json).map_err(|e| AppError::Persistence(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_are_trimmed_and_lose_trailing_slashes() {
        assert_eq!(normalize_server_url(" http://127.0.0.1:8787/ ").unwrap(), "http://127.0.0.1:8787");
        assert_eq!(normalize_server_url("https://sitter.example.com//").unwrap(), "https://sitter.example.com");
        assert_eq!(normalize_server_url("HTTP://Host").unwrap(), "HTTP://Host");
    }

    #[test]
    fn urls_without_http_scheme_or_host_are_rejected() {
        for bad in ["", "   ", "127.0.0.1:8787", "ftp://host", "http://", "https:///"] {
            assert!(normalize_server_url(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn the_api_key_is_masked_in_debug_output() {
        let settings = ServerSettings {
            server_url: "http://h".to_string(),
            api_key: "super-secret".to_string(),
        };
        let printed = format!("{settings:?}");
        assert!(!printed.contains("super-secret"));
        assert!(printed.contains("http://h"));
    }

    #[test]
    fn only_the_url_goes_to_the_settings_file() {
        let dir = std::env::temp_dir().join(format!("slacksitter-settings-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(FILE_NAME);

        assert!(read_stored(&path).unwrap().is_none());
        write_stored(&path, &StoredSettings { server_url: "http://h:1".to_string() }).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("serverUrl"));
        assert!(!text.to_lowercase().contains("key"));
        assert_eq!(read_stored(&path).unwrap().unwrap().server_url, "http://h:1");

        fs::write(&path, "not json").unwrap();
        assert!(read_stored(&path).unwrap().is_none());
        fs::remove_dir_all(&dir).ok();
    }
}
