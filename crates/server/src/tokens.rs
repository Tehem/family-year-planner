//! Personal access tokens (§11.3): `fy_pat_` + 32 random bytes, stored as a SHA-256 hash.

use chrono::{Duration, Utc};
use serde::Serialize;
use sqlx::Row;

use crate::crypto::{random_token, sha256};
use crate::storage::{Db, now_ts, ts};

pub const PAT_PREFIX: &str = "fy_pat_";
pub const MAX_NAME: usize = 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    Read,
    ReadWrite,
}

impl Scope {
    pub fn as_str(self) -> &'static str {
        match self {
            Scope::Read => "read",
            Scope::ReadWrite => "read_write",
        }
    }

    pub fn parse(s: &str) -> Option<Scope> {
        match s {
            "read" => Some(Scope::Read),
            "read_write" => Some(Scope::ReadWrite),
            _ => None,
        }
    }

    pub fn can_write(self) -> bool {
        self == Scope::ReadWrite
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Pat {
    pub id: String,
    pub name: String,
    pub scope: Scope,
    pub created_at: String,
    pub last_used_at: Option<String>,
    pub kiosk: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TokenError {
    #[error("Donnez un nom au jeton.")]
    NameRequired,
    #[error("Le nom est trop long (60 caractères maximum).")]
    NameTooLong,
    #[error("Un jeton actif porte déjà ce nom.")]
    NameTaken,
    #[error("Une erreur est survenue.")]
    Database(String),
}

impl TokenError {
    /// The message in `lang` (the `Display` text is the French one).
    pub fn message(&self, lang: annee_domain::Lang) -> String {
        match self {
            TokenError::NameRequired => {
                lang.pick("Donnez un nom au jeton.", "Give the token a name.")
            }
            TokenError::NameTooLong => lang.pick(
                "Le nom est trop long (60 caractères maximum).",
                "The name is too long (60 characters maximum).",
            ),
            TokenError::NameTaken => lang.pick(
                "Un jeton actif porte déjà ce nom.",
                "An active token already has this name.",
            ),
            TokenError::Database(_) => {
                lang.pick("Une erreur est survenue.", "Something went wrong.")
            }
        }
        .to_string()
    }
}

impl From<sqlx::Error> for TokenError {
    fn from(e: sqlx::Error) -> Self {
        match &e {
            sqlx::Error::Database(d) if d.is_unique_violation() => TokenError::NameTaken,
            _ => TokenError::Database(e.to_string()),
        }
    }
}

fn pat_from_row(r: &sqlx::sqlite::SqliteRow) -> Option<Pat> {
    Some(Pat {
        id: r.get("id"),
        name: r.get("name"),
        scope: Scope::parse(r.get::<String, _>("scope").as_str())?,
        created_at: r.get("created_at"),
        last_used_at: r.get("last_used_at"),
        kiosk: r.get::<i64, _>("kiosk") != 0,
    })
}

#[derive(Clone)]
pub struct Tokens {
    db: Db,
}

impl Tokens {
    pub fn new(db: Db) -> Self {
        Tokens { db }
    }

    /// Create a token. The clear token is returned once and never stored.
    pub async fn create(
        &self,
        name: &str,
        scope: Scope,
        kiosk: bool,
        created_by: &str,
    ) -> Result<(Pat, String), TokenError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(TokenError::NameRequired);
        }
        if name.chars().count() > MAX_NAME {
            return Err(TokenError::NameTooLong);
        }
        let scope = if kiosk { Scope::Read } else { scope };
        let clear = format!("{PAT_PREFIX}{}", random_token(32));
        let id = uuid::Uuid::new_v4().to_string();
        let created_at = now_ts();
        sqlx::query(
            "INSERT INTO pat_tokens (id, name, token_hash, scope, kiosk, created_at, created_by) VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(name)
        .bind(sha256(clear.as_bytes()))
        .bind(scope.as_str())
        .bind(kiosk as i64)
        .bind(&created_at)
        .bind(created_by)
        .execute(&self.db.pool)
        .await?;
        let pat = Pat {
            id,
            name: name.to_string(),
            scope,
            created_at,
            last_used_at: None,
            kiosk,
        };
        Ok((pat, clear))
    }

    /// Active (non-revoked) tokens, newest first.
    pub async fn list(&self) -> Result<Vec<Pat>, TokenError> {
        let rows = sqlx::query(
            "SELECT id, name, scope, kiosk, created_at, last_used_at FROM pat_tokens \
             WHERE revoked_at IS NULL ORDER BY created_at DESC, rowid DESC",
        )
        .fetch_all(&self.db.pool)
        .await?;
        Ok(rows.iter().filter_map(pat_from_row).collect())
    }

    /// Revoke a token. `false` if it does not exist or was already revoked.
    pub async fn revoke(&self, id: &str) -> Result<bool, TokenError> {
        let r =
            sqlx::query("UPDATE pat_tokens SET revoked_at = ? WHERE id = ? AND revoked_at IS NULL")
                .bind(now_ts())
                .bind(id)
                .execute(&self.db.pool)
                .await?;
        Ok(r.rows_affected() > 0)
    }

    /// Find the active token for a clear token string, recording its use (at most once a minute).
    pub async fn authenticate(&self, clear: &str) -> Result<Option<Pat>, TokenError> {
        if !clear.starts_with(PAT_PREFIX) {
            return Ok(None);
        }
        let row = sqlx::query(
            "SELECT id, name, scope, kiosk, created_at, last_used_at FROM pat_tokens \
             WHERE token_hash = ? AND revoked_at IS NULL",
        )
        .bind(sha256(clear.as_bytes()))
        .fetch_optional(&self.db.pool)
        .await?;
        let Some(pat) = row.as_ref().and_then(pat_from_row) else {
            return Ok(None);
        };
        let cutoff = ts(Utc::now() - Duration::seconds(60));
        sqlx::query("UPDATE pat_tokens SET last_used_at = ? WHERE id = ? AND (last_used_at IS NULL OR last_used_at < ?)")
            .bind(now_ts())
            .bind(&pat.id)
            .bind(cutoff)
            .execute(&self.db.pool)
            .await?;
        Ok(Some(pat))
    }

    /// Whether an active token with this id exists (kiosk sessions are bound to a token id).
    pub async fn is_active(&self, id: &str) -> Result<Option<Pat>, TokenError> {
        let row = sqlx::query(
            "SELECT id, name, scope, kiosk, created_at, last_used_at FROM pat_tokens \
             WHERE id = ? AND revoked_at IS NULL",
        )
        .bind(id)
        .fetch_optional(&self.db.pool)
        .await?;
        Ok(row.as_ref().and_then(pat_from_row))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn tokens() -> Tokens {
        Tokens::new(Db::memory().await.unwrap())
    }

    #[tokio::test]
    async fn create_returns_the_clear_token_once_and_stores_only_its_hash() {
        let t = tokens().await;
        let (pat, clear) = t
            .create("Grok API", Scope::ReadWrite, false, "Alex")
            .await
            .unwrap();
        assert!(clear.starts_with("fy_pat_"));
        assert_eq!(clear.len(), "fy_pat_".len() + 43);
        let stored: Vec<u8> = sqlx::query_scalar("SELECT token_hash FROM pat_tokens WHERE id = ?")
            .bind(&pat.id)
            .fetch_one(&t.db.pool)
            .await
            .unwrap();
        assert_eq!(stored, sha256(clear.as_bytes()));
        assert!(
            !stored
                .windows(clear.len().min(8))
                .any(|w| w == &clear.as_bytes()[..8])
        );
        // The listing never contains the token.
        let listed = t.list().await.unwrap();
        assert_eq!(listed.len(), 1);
        assert!(!serde_json::to_string(&listed).unwrap().contains(&clear));
    }

    #[tokio::test]
    async fn authenticate_and_revoke() {
        let t = tokens().await;
        let (pat, clear) = t.create("Codex", Scope::Read, false, "Alex").await.unwrap();
        let found = t.authenticate(&clear).await.unwrap().unwrap();
        assert_eq!(found.id, pat.id);
        assert_eq!(found.scope, Scope::Read);
        assert!(t.authenticate("fy_pat_wrong").await.unwrap().is_none());
        assert!(t.authenticate("not-a-pat").await.unwrap().is_none());
        assert!(t.revoke(&pat.id).await.unwrap());
        assert!(!t.revoke(&pat.id).await.unwrap(), "already revoked");
        assert!(t.authenticate(&clear).await.unwrap().is_none());
        assert!(t.list().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn last_used_is_updated_at_most_once_a_minute() {
        let t = tokens().await;
        let (pat, clear) = t.create("Agent", Scope::Read, false, "Alex").await.unwrap();
        assert!(t.list().await.unwrap()[0].last_used_at.is_none());
        t.authenticate(&clear).await.unwrap();
        let first = t.list().await.unwrap()[0].last_used_at.clone().unwrap();
        // A second use within a minute does not move the timestamp.
        sqlx::query("UPDATE pat_tokens SET last_used_at = '2999-01-01T00:00:00Z' WHERE id = ?")
            .bind(&pat.id)
            .execute(&t.db.pool)
            .await
            .unwrap();
        t.authenticate(&clear).await.unwrap();
        assert_eq!(
            t.list().await.unwrap()[0].last_used_at.as_deref(),
            Some("2999-01-01T00:00:00Z")
        );
        // A stale timestamp is refreshed.
        sqlx::query("UPDATE pat_tokens SET last_used_at = '2020-01-01T00:00:00Z' WHERE id = ?")
            .bind(&pat.id)
            .execute(&t.db.pool)
            .await
            .unwrap();
        t.authenticate(&clear).await.unwrap();
        assert!(t.list().await.unwrap()[0].last_used_at.clone().unwrap() >= first);
    }

    #[tokio::test]
    async fn names_are_validated_and_unique_among_active_tokens() {
        let t = tokens().await;
        assert_eq!(
            t.create("  ", Scope::Read, false, "T").await.unwrap_err(),
            TokenError::NameRequired
        );
        assert_eq!(
            t.create(&"x".repeat(61), Scope::Read, false, "T")
                .await
                .unwrap_err(),
            TokenError::NameTooLong
        );
        assert!(
            t.create(&"x".repeat(60), Scope::Read, false, "T")
                .await
                .is_ok()
        );
        let (pat, _) = t.create("Grok", Scope::Read, false, "T").await.unwrap();
        assert_eq!(
            t.create("Grok", Scope::ReadWrite, false, "T")
                .await
                .unwrap_err(),
            TokenError::NameTaken
        );
        t.revoke(&pat.id).await.unwrap();
        assert!(
            t.create("Grok", Scope::Read, false, "T").await.is_ok(),
            "a revoked name can be reused"
        );
    }

    #[tokio::test]
    async fn kiosk_tokens_are_always_read_only() {
        let t = tokens().await;
        let (pat, _) = t
            .create("Kiosque – 16 octobre", Scope::ReadWrite, true, "T")
            .await
            .unwrap();
        assert!(pat.kiosk);
        assert_eq!(pat.scope, Scope::Read);
    }
}
