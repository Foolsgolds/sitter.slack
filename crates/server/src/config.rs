//! Server settings: a TOML file overlaid with `SLACKSITTER__SECTION__KEY`
//! environment variables. The Slack token and the API key are `Secret`s whose
//! `Debug` output is masked, so logging a config never leaks them.

use std::fmt;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use figment::providers::{Env, Format, Toml};
use figment::Figment;
use serde::Deserialize;
use slacksitter_core::sync::SyncConfig;

pub const CONFIG_PATH_ENV: &str = "SLACKSITTER_CONFIG";
pub const DEFAULT_CONFIG_PATH: &str = "slacksitter.toml";
const ENV_PREFIX: &str = "SLACKSITTER__";
/// The placeholder in slacksitter.example.toml; starting with it would be an open door.
const EXAMPLE_API_KEY: &str = "change-me";

#[derive(Clone, Default, Deserialize)]
#[serde(transparent)]
pub struct Secret(String);

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// The raw value. Only for sending to Slack or comparing with a request.
    pub fn expose(&self) -> &str {
        &self.0
    }

    pub fn is_blank(&self) -> bool {
        self.0.trim().is_empty()
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(if self.is_blank() { "Secret(<empty>)" } else { "Secret(***)" })
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    pub bind: String,
    pub api_key: Secret,
    pub cors_origins: Vec<String>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind: "127.0.0.1:8787".to_string(),
            api_key: Secret::default(),
            cors_origins: vec![
                "tauri://localhost".to_string(),
                "http://tauri.localhost".to_string(),
                "http://localhost:1420".to_string(),
            ],
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct SlackConfig {
    pub token: Secret,
    /// Overrides `https://slack.com/api`. Only for pointing the server at a local fake Slack.
    pub api_base_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct DatabaseConfig {
    pub path: PathBuf,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            path: PathBuf::from("./data/slacksitter.db"),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SyncSettings {
    pub channel_prefix: String,
    pub history_limit: u32,
    pub request_spacing_seconds: u64,
    pub cycle_interval_seconds: u64,
    pub catalog_interval_seconds: u64,
    pub max_pages_per_channel: u32,
}

impl Default for SyncSettings {
    fn default() -> Self {
        let core = SyncConfig::default();
        Self {
            channel_prefix: core.channel_prefix,
            history_limit: core.history_limit,
            request_spacing_seconds: core.request_spacing.as_secs(),
            cycle_interval_seconds: core.cycle_interval.as_secs(),
            catalog_interval_seconds: core.catalog_interval.as_secs(),
            max_pages_per_channel: core.max_pages_per_channel,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct LogConfig {
    pub dir: PathBuf,
    pub level: String,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            dir: PathBuf::from("./logs"),
            level: "info".to_string(),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Config {
    pub server: ServerConfig,
    pub slack: SlackConfig,
    pub database: DatabaseConfig,
    pub sync: SyncSettings,
    pub log: LogConfig,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("config file not found: {0}")]
    FileNotFound(PathBuf),
    #[error("cannot read the settings: {0}")]
    Read(String),
    #[error("invalid settings: {0}")]
    Invalid(String),
}

impl Config {
    /// Reads the settings. The file is `explicit_path`, else `$SLACKSITTER_CONFIG`,
    /// else `./slacksitter.toml`. A missing default file is fine (environment
    /// variables alone can carry everything); a missing explicit one is an error.
    pub fn load(explicit_path: Option<&Path>) -> Result<Config, ConfigError> {
        let from_env = std::env::var_os(CONFIG_PATH_ENV).filter(|v| !v.is_empty()).map(PathBuf::from);
        let (path, required) = match (explicit_path, from_env) {
            (Some(path), _) => (path.to_path_buf(), true),
            (None, Some(path)) => (path, true),
            (None, None) => (PathBuf::from(DEFAULT_CONFIG_PATH), false),
        };
        if required && !path.is_file() {
            return Err(ConfigError::FileNotFound(path));
        }
        let figment = Figment::new()
            .merge(Toml::file(path))
            .merge(Env::prefixed(ENV_PREFIX).split("__"));
        Self::from_figment(figment)
    }

    pub fn from_figment(figment: Figment) -> Result<Config, ConfigError> {
        let config: Config = figment.extract().map_err(|e| ConfigError::Read(e.to_string()))?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.server.api_key.is_blank() {
            return Err(ConfigError::Invalid(
                "server.api_key is required (set it in the file or in SLACKSITTER__SERVER__API_KEY)".into(),
            ));
        }
        if self.slack.token.is_blank() {
            return Err(ConfigError::Invalid(
                "slack.token is required (set it in the file or in SLACKSITTER__SLACK__TOKEN)".into(),
            ));
        }
        if self.server.api_key.expose().trim() == EXAMPLE_API_KEY {
            return Err(ConfigError::Invalid(format!(
                "server.api_key is still the example value {EXAMPLE_API_KEY:?}; set your own"
            )));
        }
        self.server
            .bind
            .parse::<SocketAddr>()
            .map_err(|e| ConfigError::Invalid(format!("server.bind {:?}: {e}", self.server.bind)))?;
        if let Some(base) = &self.slack.api_base_url {
            if !(base.starts_with("http://") || base.starts_with("https://")) {
                return Err(ConfigError::Invalid("slack.api_base_url must start with http:// or https://".into()));
            }
        }
        if self.sync.history_limit == 0 || self.sync.history_limit > 200 {
            return Err(ConfigError::Invalid("sync.history_limit must be between 1 and 200".into()));
        }
        if self.sync.max_pages_per_channel == 0 {
            return Err(ConfigError::Invalid("sync.max_pages_per_channel must be at least 1".into()));
        }
        Ok(())
    }

    pub fn bind_addr(&self) -> SocketAddr {
        self.server.bind.parse().expect("validated in Config::validate")
    }

    pub fn sync_config(&self) -> SyncConfig {
        SyncConfig {
            channel_prefix: self.sync.channel_prefix.clone(),
            history_limit: self.sync.history_limit,
            max_pages_per_channel: self.sync.max_pages_per_channel,
            request_spacing: Duration::from_secs(self.sync.request_spacing_seconds),
            cycle_interval: Duration::from_secs(self.sync.cycle_interval_seconds),
            catalog_interval: Duration::from_secs(self.sync.catalog_interval_seconds),
            ..SyncConfig::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use figment::providers::Toml;

    fn parse(toml: &str) -> Result<Config, ConfigError> {
        Config::from_figment(Figment::new().merge(Toml::string(toml)))
    }

    #[test]
    fn defaults_fill_everything_but_the_secrets() {
        let config = parse("[server]\napi_key = \"k\"\n[slack]\ntoken = \"xoxp-t\"\n").unwrap();
        assert_eq!(config.server.bind, "127.0.0.1:8787");
        assert_eq!(config.database.path, PathBuf::from("./data/slacksitter.db"));
        assert_eq!(config.sync.channel_prefix, "times");
        assert_eq!(config.sync.history_limit, 15);
        assert_eq!(config.sync.request_spacing_seconds, 60);
        assert_eq!(config.sync.cycle_interval_seconds, 300);
        assert_eq!(config.sync.catalog_interval_seconds, 3600);
        assert_eq!(config.sync.max_pages_per_channel, 10);
        assert_eq!(config.log.level, "info");
        assert_eq!(config.server.cors_origins.len(), 3);
        assert_eq!(config.sync_config().request_spacing, Duration::from_secs(60));
    }

    #[test]
    fn an_empty_api_key_or_token_refuses_to_start() {
        let err = parse("[slack]\ntoken = \"xoxp-t\"\n").unwrap_err();
        assert!(err.to_string().contains("server.api_key"));
        let err = parse("[server]\napi_key = \"  \"\n[slack]\ntoken = \"xoxp-t\"\n").unwrap_err();
        assert!(err.to_string().contains("server.api_key"));
        let err = parse("[server]\napi_key = \"k\"\n").unwrap_err();
        assert!(err.to_string().contains("slack.token"));
    }

    #[test]
    fn the_example_api_key_is_refused() {
        let err = parse("[server]\napi_key = \"change-me\"\n[slack]\ntoken = \"xoxp-t\"\n").unwrap_err();
        assert!(err.to_string().contains("example"));
    }

    #[test]
    fn bad_values_are_rejected() {
        let base = "[server]\napi_key = \"k\"\nbind = \"nope\"\n[slack]\ntoken = \"t\"\n";
        assert!(parse(base).unwrap_err().to_string().contains("server.bind"));
        let base = "[server]\napi_key = \"k\"\n[slack]\ntoken = \"t\"\n[sync]\nhistory_limit = 0\n";
        assert!(parse(base).unwrap_err().to_string().contains("history_limit"));
    }

    #[test]
    fn debug_output_masks_the_secrets() {
        let config = parse("[server]\napi_key = \"super-secret-key\"\n[slack]\ntoken = \"xoxp-super-secret\"\n").unwrap();
        let printed = format!("{config:?}");
        assert!(!printed.contains("super-secret"));
        assert!(printed.contains("Secret(***)"));
        assert_eq!(config.server.api_key.expose(), "super-secret-key");
    }

    #[test]
    fn environment_variables_override_the_file_and_can_stand_alone() {
        figment::Jail::expect_with(|jail| {
            jail.create_file(
                "slacksitter.toml",
                "[server]\napi_key = \"from-file\"\n[slack]\ntoken = \"file-token\"\n[sync]\nhistory_limit = 5\n",
            )?;
            jail.set_env("SLACKSITTER__SERVER__API_KEY", "from-env");
            jail.set_env("SLACKSITTER__SYNC__CHANNEL_PREFIX", "random");
            let config = Config::load(None).unwrap();
            assert_eq!(config.server.api_key.expose(), "from-env");
            assert_eq!(config.slack.token.expose(), "file-token");
            assert_eq!(config.sync.history_limit, 5);
            assert_eq!(config.sync.channel_prefix, "random");
            Ok(())
        });

        figment::Jail::expect_with(|jail| {
            jail.set_env("SLACKSITTER__SERVER__API_KEY", "only-env");
            jail.set_env("SLACKSITTER__SLACK__TOKEN", "xoxp-only-env");
            let config = Config::load(None).unwrap();
            assert_eq!(config.server.api_key.expose(), "only-env");
            assert_eq!(config.slack.token.expose(), "xoxp-only-env");
            Ok(())
        });
    }

    #[test]
    fn an_explicit_missing_file_is_an_error() {
        figment::Jail::expect_with(|_jail| {
            let err = Config::load(Some(Path::new("nope.toml"))).unwrap_err();
            assert!(matches!(err, ConfigError::FileNotFound(_)));
            Ok(())
        });
    }
}
