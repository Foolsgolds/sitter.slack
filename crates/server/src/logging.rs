//! Daily-rotating log file plus stdout. Nothing here ever prints the Slack
//! token or the API key (the config's `Debug` masks both).

use std::fs;
use std::path::Path;

use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{Builder, Rotation};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

use crate::config::LogConfig;

pub const LOG_FILE_PREFIX: &str = "slacksitter-server";
const LOG_FILE_SUFFIX: &str = "log";
const MAX_LOG_FILES: usize = 14;

/// Starts logging to stdout and to `<dir>/slacksitter-server.<date>.log`.
/// Keep the returned guard alive for the life of the process.
pub fn init(config: &LogConfig) -> WorkerGuard {
    fs::create_dir_all(&config.dir).ok();
    let file_appender = Builder::new()
        .rotation(Rotation::DAILY)
        .filename_prefix(LOG_FILE_PREFIX)
        .filename_suffix(LOG_FILE_SUFFIX)
        .max_log_files(MAX_LOG_FILES)
        .build(&config.dir)
        .unwrap_or_else(|_| tracing_appender::rolling::never(&config.dir, "slacksitter-server.log"));
    let (file_writer, guard) = tracing_appender::non_blocking(file_appender);

    let filter = EnvFilter::try_new(&config.level).unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer().with_target(false))
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(file_writer)
                .with_ansi(false)
                .with_target(false),
        )
        .init();
    guard
}

/// The last `max_lines` lines of the newest log file in `dir` (empty when there is none).
pub fn read_recent(dir: &Path, max_lines: usize) -> std::io::Result<String> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(String::new()),
        Err(err) => return Err(err),
    };
    let newest = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().starts_with(LOG_FILE_PREFIX))
        .filter_map(|entry| Some((entry.metadata().ok()?.modified().ok()?, entry.path())))
        .max_by_key(|(modified, _)| *modified)
        .map(|(_, path)| path);
    match newest {
        Some(path) => Ok(tail_lines(&String::from_utf8_lossy(&fs::read(path)?), max_lines)),
        None => Ok(String::new()),
    }
}

fn tail_lines(text: &str, max_lines: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(max_lines)..].join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tail_keeps_only_the_last_lines() {
        assert_eq!(tail_lines("a\nb\nc\nd", 2), "c\nd");
        assert_eq!(tail_lines("a\nb", 10), "a\nb");
        assert_eq!(tail_lines("", 10), "");
    }

    #[test]
    fn reading_a_missing_directory_gives_nothing() {
        assert_eq!(read_recent(Path::new("definitely/not/here"), 10).unwrap(), "");
    }

    #[test]
    fn the_newest_server_log_is_read() {
        let dir = std::env::temp_dir().join(format!("slacksitter-logtest-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("slacksitter-server.2020-01-01.log"), "old\n").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        fs::write(dir.join("slacksitter-server.2020-01-02.log"), "l1\nl2\nl3\n").unwrap();
        fs::write(dir.join("unrelated.log"), "nope\n").unwrap();
        assert_eq!(read_recent(&dir, 2).unwrap(), "l2\nl3");
        fs::remove_dir_all(&dir).ok();
    }
}
