//! Slack access token storage. Unlike the legacy app (plaintext `.env` file
//! whose location depended on fragile `.csproj`/`.sln` directory-walking),
//! the token is stored in the OS credential store via the `keyring` crate —
//! never written to disk in plaintext.

use keyring::Entry;

use slacksitter_core::error::{AppError, AppResult};

const SERVICE: &str = "SlackSitter";
const USERNAME: &str = "slack_access_token";

fn entry() -> AppResult<Entry> {
    Entry::new(SERVICE, USERNAME).map_err(|e| AppError::Credential(e.to_string()))
}

pub fn save_token(token: &str) -> AppResult<()> {
    entry()?
        .set_password(token)
        .map_err(|e| AppError::Credential(e.to_string()))
}

pub fn load_token() -> AppResult<Option<String>> {
    match entry()?.get_password() {
        Ok(token) => Ok(Some(token)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(AppError::Credential(e.to_string())),
    }
}

pub fn delete_token() -> AppResult<()> {
    match entry()?.delete_password() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(AppError::Credential(e.to_string())),
    }
}
