//! SQLite persistence: connection setup, migrations, write-side upserts
//! (`store`) and read-side assembly of the display models (`view`).

use std::path::Path;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
pub use sqlx::SqlitePool;

use crate::error::{AppError, AppResult};

pub mod store;
pub mod view;

/// Opens (creating if needed) the database file, enables WAL + foreign keys
/// and applies the pending migrations.
pub async fn connect(path: &Path) -> AppResult<SqlitePool> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)
            .map_err(|e| AppError::Persistence(format!("cannot create {}: {e}", parent.display())))?;
    }
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new().max_connections(4).connect_with(options).await?;
    migrate(&pool).await?;
    Ok(pool)
}

/// A private in-memory database (for tests). Every in-memory connection is
/// its own database, so the pool is pinned to one connection that never expires.
pub async fn connect_memory() -> AppResult<SqlitePool> {
    let options = SqliteConnectOptions::from_str("sqlite::memory:")?.foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .idle_timeout(None)
        .max_lifetime(None)
        // Tests that pause tokio time would otherwise see the default 30s acquire
        // timeout "elapse" while the sqlite worker thread is busy.
        .acquire_timeout(std::time::Duration::from_secs(24 * 3600))
        .connect_with(options)
        .await?;
    migrate(&pool).await?;
    Ok(pool)
}

pub async fn migrate(pool: &SqlitePool) -> AppResult<()> {
    sqlx::migrate!("./migrations").run(pool).await?;
    Ok(())
}

/// Current time as unix epoch seconds (the unit of every `*_at` column).
pub fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn migrations_apply_and_create_every_table() {
        let pool = connect_memory().await.unwrap();
        let tables: Vec<String> = sqlx::query_scalar(
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE '\\_%' ESCAPE '\\' \
             AND name NOT LIKE 'sqlite_%' ORDER BY name",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(
            tables,
            ["channels", "emoji", "files", "messages", "reactions", "sync_state", "sync_targets", "users"]
        );
    }

    #[tokio::test]
    async fn migrating_twice_is_harmless() {
        let pool = connect_memory().await.unwrap();
        migrate(&pool).await.unwrap();
    }

    #[tokio::test]
    async fn foreign_keys_are_enforced() {
        let pool = connect_memory().await.unwrap();
        let result = sqlx::query(
            "INSERT INTO messages (channel_id, ts, raw_json, fetched_at) VALUES ('CX', '1.1', '{}', 0)",
        )
        .execute(&pool)
        .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn file_database_uses_wal() {
        let dir = std::env::temp_dir().join(format!("slacksitter-test-{}", now_unix()));
        let path = dir.join("nested").join("test.db");
        let pool = connect(&path).await.unwrap();
        let mode: String = sqlx::query_scalar("PRAGMA journal_mode").fetch_one(&pool).await.unwrap();
        assert_eq!(mode.to_lowercase(), "wal");
        let fk: i64 = sqlx::query_scalar("PRAGMA foreign_keys").fetch_one(&pool).await.unwrap();
        assert_eq!(fk, 1);
        pool.close().await;
        std::fs::remove_dir_all(&dir).ok();
    }
}
