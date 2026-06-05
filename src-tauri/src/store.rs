use crate::error::Result;
use chrono::Utc;
use serde::Serialize;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool};
use sqlx::Row;
use std::path::Path;
use uuid::Uuid;

const MIGRATION: &str = "
CREATE TABLE IF NOT EXISTS conversations (
  id         TEXT PRIMARY KEY,
  title      TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS messages (
  id              TEXT PRIMARY KEY,
  conversation_id TEXT NOT NULL REFERENCES conversations(id),
  role            TEXT NOT NULL CHECK(role IN ('user','assistant','system')),
  content         TEXT NOT NULL DEFAULT '',
  created_at      TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_messages_conversation ON messages(conversation_id);
CREATE TABLE IF NOT EXISTS settings (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
";

#[derive(Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Conversation {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub id: String,
    pub conversation_id: String,
    pub role: String,
    pub content: String,
    pub created_at: String,
    pub tier: Option<String>,
    pub served_model: Option<String>,
}

const SCHEMA_VERSION: i64 = 2;

pub async fn init_pool(db_path: &Path) -> Result<SqlitePool> {
    let opts = SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(true);
    let pool = SqlitePool::connect_with(opts).await?;

    let mut version: i64 = sqlx::query_scalar("PRAGMA user_version")
        .fetch_one(&pool)
        .await?;

    if version < 1 {
        sqlx::raw_sql(MIGRATION).execute(&pool).await?;
        version = 1;
    }
    if version < 2 {
        sqlx::raw_sql(
            "ALTER TABLE messages ADD COLUMN tier TEXT;\n\
             ALTER TABLE messages ADD COLUMN served_model TEXT;",
        )
        .execute(&pool)
        .await?;
        version = 2;
    }

    // PRAGMA does not accept bind params; SCHEMA_VERSION is a trusted constant,
    // so asserting the formatted string is injection-safe is correct here.
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "PRAGMA user_version = {SCHEMA_VERSION}"
    )))
    .execute(&pool)
    .await?;
    let _ = version;
    Ok(pool)
}

pub async fn list_conversations(pool: &SqlitePool) -> Result<Vec<Conversation>> {
    let rows = sqlx::query_as::<_, Conversation>(
        "SELECT id, title, created_at, updated_at FROM conversations ORDER BY updated_at DESC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn get_messages(pool: &SqlitePool, conversation_id: &str) -> Result<Vec<Message>> {
    let rows = sqlx::query_as::<_, Message>(
        "SELECT id, conversation_id, role, content, created_at, tier, served_model \
         FROM messages WHERE conversation_id = ? ORDER BY created_at ASC, id ASC",
    )
    .bind(conversation_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn create_conversation(pool: &SqlitePool, title: &str) -> Result<Conversation> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    sqlx::query("INSERT INTO conversations (id, title, created_at, updated_at) VALUES (?, ?, ?, ?)")
        .bind(&id)
        .bind(title)
        .bind(&now)
        .bind(&now)
        .execute(pool)
        .await?;
    Ok(Conversation {
        id,
        title: title.to_string(),
        created_at: now.clone(),
        updated_at: now,
    })
}

pub async fn insert_message(
    pool: &SqlitePool,
    conversation_id: &str,
    role: &str,
    content: &str,
) -> Result<Message> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO messages (id, conversation_id, role, content, created_at, tier, served_model) \
         VALUES (?, ?, ?, ?, ?, NULL, NULL)",
    )
    .bind(&id)
    .bind(conversation_id)
    .bind(role)
    .bind(content)
    .bind(&now)
    .execute(pool)
    .await?;
    sqlx::query("UPDATE conversations SET updated_at = ? WHERE id = ?")
        .bind(&now)
        .bind(conversation_id)
        .execute(pool)
        .await?;
    Ok(Message {
        id,
        conversation_id: conversation_id.to_string(),
        role: role.to_string(),
        content: content.to_string(),
        created_at: now,
        tier: None,
        served_model: None,
    })
}

pub async fn update_message_content(
    pool: &SqlitePool,
    message_id: &str,
    content: &str,
) -> Result<()> {
    sqlx::query("UPDATE messages SET content = ? WHERE id = ?")
        .bind(content)
        .bind(message_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn set_message_routing(
    pool: &SqlitePool,
    message_id: &str,
    tier: &str,
    served_model: &str,
) -> Result<()> {
    sqlx::query("UPDATE messages SET tier = ?, served_model = ? WHERE id = ?")
        .bind(tier)
        .bind(served_model)
        .bind(message_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn get_setting(pool: &SqlitePool, key: &str) -> Result<Option<String>> {
    let row = sqlx::query("SELECT value FROM settings WHERE key = ?")
        .bind(key)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|r| r.get::<String, _>("value")))
}

pub async fn set_setting(pool: &SqlitePool, key: &str, value: &str) -> Result<()> {
    sqlx::query(
        "INSERT INTO settings (key, value) VALUES (?, ?) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )
    .bind(key)
    .bind(value)
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn migration_creates_tables_and_routing_columns() {
        let path = std::env::temp_dir().join("firefly_test_migration.db");
        let _ = std::fs::remove_file(&path);
        let pool = init_pool(&path).await.unwrap();

        // user_version advanced to latest
        let version: i64 = sqlx::query_scalar("PRAGMA user_version")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(version, 2);

        // messages has the new columns
        let cols: Vec<String> = sqlx::query_scalar("SELECT name FROM pragma_table_info('messages')")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert!(cols.iter().any(|c| c == "tier"));
        assert!(cols.iter().any(|c| c == "served_model"));

        let _ = std::fs::remove_file(&path);
    }
}
