//! SQLite persistence (§14): tokens, OAuth state, import runs.

use std::str::FromStr;

use chrono::{DateTime, SecondsFormat, Utc};
use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};

#[derive(Clone)]
pub struct Db {
    pub pool: SqlitePool,
}

/// Timestamps are stored as RFC 3339 UTC text.
pub fn ts(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Secs, true)
}

pub fn now_ts() -> String {
    ts(Utc::now())
}

impl Db {
    /// Open (creating if needed) the database at `url` and run the migrations.
    pub async fn connect(url: &str) -> anyhow::Result<Db> {
        let opts = SqliteConnectOptions::from_str(url)?
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .foreign_keys(true)
            .busy_timeout(std::time::Duration::from_secs(5));
        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(opts)
            .await?;
        sqlx::migrate!("./migrations").run(&pool).await?;
        Ok(Db { pool })
    }

    /// A private in-memory database (demo mode and tests): a single connection, since every
    /// in-memory connection would otherwise be its own database.
    pub async fn memory() -> anyhow::Result<Db> {
        let opts = SqliteConnectOptions::from_str("sqlite::memory:")?.foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .min_connections(1)
            .idle_timeout(None)
            .max_lifetime(None)
            .connect_with(opts)
            .await?;
        sqlx::migrate!("./migrations").run(&pool).await?;
        Ok(Db { pool })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn migrations_create_every_table() {
        let db = Db::memory().await.unwrap();
        for table in [
            "pat_tokens",
            "oauth_clients",
            "oauth_codes",
            "oauth_grants",
            "oauth_refresh_tokens",
            "import_runs",
        ] {
            let n: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name = ?",
            )
            .bind(table)
            .fetch_one(&db.pool)
            .await
            .unwrap();
            assert_eq!(n, 1, "{table}");
        }
    }

    #[tokio::test]
    async fn file_database_uses_wal() {
        let dir = std::env::temp_dir().join(format!("annee-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let url = format!("sqlite://{}", dir.join("annee.db").display());
        let db = Db::connect(&url).await.unwrap();
        let mode: String = sqlx::query_scalar("PRAGMA journal_mode")
            .fetch_one(&db.pool)
            .await
            .unwrap();
        assert_eq!(mode, "wal");
        std::fs::remove_dir_all(dir).ok();
    }
}
