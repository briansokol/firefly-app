use crate::error::Result;
use chrono::Utc;
use serde::Serialize;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool};
use sqlx::Row;
use std::path::Path;
use uuid::Uuid;

/// Contract timestamp format: ISO-8601 UTC, millisecond precision, `Z` suffix
/// (e.g. `2026-06-06T14:03:21.118Z`). Required so the server's lexical sync
/// cursor compares correctly. See docs/API-CONTRACT.md §3.0.
pub fn now_iso() -> String {
    Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}

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

#[derive(Clone, Debug, Serialize, serde::Deserialize)]
pub struct ConvRow {
    pub id: String,
    pub user_id: String,
    pub title: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Serialize, serde::Deserialize)]
pub struct MsgRow {
    pub id: String,
    pub conversation_id: String,
    pub role: String,
    pub content: String,
    pub model: Option<String>,
    pub created_at: String,
}

#[derive(Clone, Debug, Serialize, serde::Deserialize)]
pub struct MemRow {
    pub id: String,
    pub user_id: String,
    pub text: String,
    pub source_conversation: Option<String>,
    pub updated_at: String,
}

pub struct SyncState {
    pub device_id: String,
    pub user_id: String,
    pub cursor: String,
}

const SCHEMA_VERSION: i64 = 3;

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

    if version < 3 {
        sqlx::raw_sql(
            "ALTER TABLE conversations ADD COLUMN user_id TEXT;\n\
             ALTER TABLE conversations ADD COLUMN pending_push INTEGER NOT NULL DEFAULT 1;\n\
             ALTER TABLE messages ADD COLUMN pending_push INTEGER NOT NULL DEFAULT 1;\n\
             CREATE TABLE IF NOT EXISTS sync_state (\n\
               id        INTEGER PRIMARY KEY CHECK (id = 1),\n\
               device_id TEXT NOT NULL DEFAULT '',\n\
               user_id   TEXT NOT NULL DEFAULT '',\n\
               cursor    TEXT NOT NULL DEFAULT ''\n\
             );\n\
             INSERT OR IGNORE INTO sync_state (id, device_id, user_id, cursor) VALUES (1, '', '', '');\n\
             CREATE TABLE IF NOT EXISTS memories (\n\
               id                  TEXT PRIMARY KEY,\n\
               user_id             TEXT NOT NULL,\n\
               text                TEXT NOT NULL,\n\
               source_conversation TEXT,\n\
               updated_at          TEXT NOT NULL\n\
             );",
        )
        .execute(&pool)
        .await?;
        version = 3;
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
    let now = now_iso();
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
    pending: bool,
) -> Result<Message> {
    let id = Uuid::new_v4().to_string();
    let now = now_iso();
    sqlx::query(
        "INSERT INTO messages (id, conversation_id, role, content, created_at, tier, served_model, pending_push) \
         VALUES (?, ?, ?, ?, ?, NULL, NULL, ?)",
    )
    .bind(&id)
    .bind(conversation_id)
    .bind(role)
    .bind(content)
    .bind(&now)
    .bind(if pending { 1 } else { 0 })
    .execute(pool)
    .await?;
    sqlx::query("UPDATE conversations SET updated_at = ?, pending_push = 1 WHERE id = ?")
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
    sqlx::query("UPDATE messages SET content = ?, pending_push = 1 WHERE id = ?")
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

pub async fn get_pending_conversations(pool: &SqlitePool, user_id: &str) -> Result<Vec<ConvRow>> {
    let rows = sqlx::query(
        "SELECT id, title, created_at, updated_at FROM conversations WHERE pending_push = 1",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| ConvRow {
            id: r.get("id"),
            user_id: user_id.to_string(),
            title: Some(r.get::<String, _>("title")),
            created_at: r.get("created_at"),
            updated_at: r.get("updated_at"),
        })
        .collect())
}

pub async fn get_pending_messages(pool: &SqlitePool) -> Result<Vec<MsgRow>> {
    let rows = sqlx::query(
        "SELECT id, conversation_id, role, content, served_model, created_at \
         FROM messages WHERE pending_push = 1 ORDER BY created_at ASC, id ASC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| MsgRow {
            id: r.get("id"),
            conversation_id: r.get("conversation_id"),
            role: r.get("role"),
            content: r.get("content"),
            model: r.get::<Option<String>, _>("served_model"),
            created_at: r.get("created_at"),
        })
        .collect())
}

async fn mark_pushed(pool: &SqlitePool, table: &str, ids: &[String]) -> Result<()> {
    if ids.is_empty() {
        return Ok(());
    }
    let placeholders = std::iter::repeat("?").take(ids.len()).collect::<Vec<_>>().join(",");
    // `table` is a trusted, hard-coded caller argument; ids are bound params.
    let sql = format!("UPDATE {table} SET pending_push = 0 WHERE id IN ({placeholders})");
    let mut q = sqlx::query(sqlx::AssertSqlSafe(sql));
    for id in ids {
        q = q.bind(id);
    }
    q.execute(pool).await?;
    Ok(())
}

pub async fn mark_conversations_pushed(pool: &SqlitePool, ids: &[String]) -> Result<()> {
    mark_pushed(pool, "conversations", ids).await
}

pub async fn mark_messages_pushed(pool: &SqlitePool, ids: &[String]) -> Result<()> {
    mark_pushed(pool, "messages", ids).await
}

pub async fn upsert_pulled_conversation(pool: &SqlitePool, c: &ConvRow) -> Result<()> {
    sqlx::query(
        "INSERT INTO conversations (id, user_id, title, created_at, updated_at, pending_push) \
         VALUES (?, ?, ?, ?, ?, 0) \
         ON CONFLICT(id) DO UPDATE SET \
           title = excluded.title, \
           user_id = excluded.user_id, \
           updated_at = excluded.updated_at, \
           pending_push = 0 \
         WHERE excluded.updated_at >= conversations.updated_at",
    )
    .bind(&c.id)
    .bind(&c.user_id)
    .bind(c.title.clone().unwrap_or_default())
    .bind(&c.created_at)
    .bind(&c.updated_at)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn insert_pulled_message(pool: &SqlitePool, m: &MsgRow) -> Result<()> {
    sqlx::query(
        "INSERT OR IGNORE INTO messages \
           (id, conversation_id, role, content, created_at, tier, served_model, pending_push) \
         VALUES (?, ?, ?, ?, ?, NULL, ?, 0)",
    )
    .bind(&m.id)
    .bind(&m.conversation_id)
    .bind(&m.role)
    .bind(&m.content)
    .bind(&m.created_at)
    .bind(&m.model)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn upsert_pulled_memory(pool: &SqlitePool, m: &MemRow) -> Result<()> {
    sqlx::query(
        "INSERT INTO memories (id, user_id, text, source_conversation, updated_at) \
         VALUES (?, ?, ?, ?, ?) \
         ON CONFLICT(id) DO UPDATE SET \
           text = excluded.text, \
           source_conversation = excluded.source_conversation, \
           updated_at = excluded.updated_at \
         WHERE excluded.updated_at >= memories.updated_at",
    )
    .bind(&m.id)
    .bind(&m.user_id)
    .bind(&m.text)
    .bind(&m.source_conversation)
    .bind(&m.updated_at)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_sync_state(pool: &SqlitePool) -> Result<SyncState> {
    let r = sqlx::query("SELECT device_id, user_id, cursor FROM sync_state WHERE id = 1")
        .fetch_one(pool)
        .await?;
    Ok(SyncState {
        device_id: r.get("device_id"),
        user_id: r.get("user_id"),
        cursor: r.get("cursor"),
    })
}

pub async fn set_sync_identity(pool: &SqlitePool, device_id: &str, user_id: &str) -> Result<()> {
    sqlx::query("UPDATE sync_state SET device_id = ?, user_id = ? WHERE id = 1")
        .bind(device_id)
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn set_sync_cursor(pool: &SqlitePool, cursor: &str) -> Result<()> {
    sqlx::query("UPDATE sync_state SET cursor = ? WHERE id = 1")
        .bind(cursor)
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

        let version: i64 = sqlx::query_scalar("PRAGMA user_version")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(version, 3);

        let msg_cols: Vec<String> =
            sqlx::query_scalar("SELECT name FROM pragma_table_info('messages')")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert!(msg_cols.iter().any(|c| c == "tier"));
        assert!(msg_cols.iter().any(|c| c == "served_model"));
        assert!(msg_cols.iter().any(|c| c == "pending_push"));

        let conv_cols: Vec<String> =
            sqlx::query_scalar("SELECT name FROM pragma_table_info('conversations')")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert!(conv_cols.iter().any(|c| c == "user_id"));
        assert!(conv_cols.iter().any(|c| c == "pending_push"));

        // sync_state seeded with exactly one row
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sync_state")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(n, 1);

        // memories table exists
        sqlx::query("SELECT id, user_id, text, source_conversation, updated_at FROM memories")
            .fetch_all(&pool)
            .await
            .unwrap();

        let _ = std::fs::remove_file(&path);
    }

    async fn fresh_pool(name: &str) -> SqlitePool {
        let path = std::env::temp_dir().join(name);
        let _ = std::fs::remove_file(&path);
        init_pool(&path).await.unwrap()
    }

    #[tokio::test]
    async fn pending_then_pushed_clears_queue() {
        let pool = fresh_pool("firefly_test_queue.db").await;
        let c = create_conversation(&pool, "hi").await.unwrap();
        insert_message(&pool, &c.id, "user", "yo", true).await.unwrap();

        let pc = get_pending_conversations(&pool, "user-1").await.unwrap();
        let pm = get_pending_messages(&pool).await.unwrap();
        assert_eq!(pc.len(), 1);
        assert_eq!(pc[0].user_id, "user-1");
        assert_eq!(pm.len(), 1);

        mark_conversations_pushed(&pool, &[c.id.clone()]).await.unwrap();
        mark_messages_pushed(&pool, &[pm[0].id.clone()]).await.unwrap();

        assert!(get_pending_conversations(&pool, "user-1").await.unwrap().is_empty());
        assert!(get_pending_messages(&pool).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn pulled_message_is_insert_or_ignore_idempotent() {
        let pool = fresh_pool("firefly_test_msgmerge.db").await;
        upsert_pulled_conversation(
            &pool,
            &ConvRow {
                id: "conv-1".into(),
                user_id: "u".into(),
                title: Some("t".into()),
                created_at: "2026-01-01T00:00:00.000Z".into(),
                updated_at: "2026-01-01T00:00:00.000Z".into(),
            },
        )
        .await
        .unwrap();
        let m = MsgRow {
            id: "msg-1".into(),
            conversation_id: "conv-1".into(),
            role: "assistant".into(),
            content: "first".into(),
            model: Some("chat-heavy".into()),
            created_at: "2026-01-01T00:00:01.000Z".into(),
        };
        insert_pulled_message(&pool, &m).await.unwrap();
        // re-push with mutated content is a no-op (append-only)
        let mut m2 = m.clone();
        m2.content = "MUTATED".into();
        insert_pulled_message(&pool, &m2).await.unwrap();

        let rows = get_messages(&pool, "conv-1").await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].content, "first");
        assert_eq!(rows[0].served_model.as_deref(), Some("chat-heavy"));
        // pulled rows are not re-queued for push
        assert!(get_pending_messages(&pool).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn conversation_merge_is_last_write_wins() {
        let pool = fresh_pool("firefly_test_convmerge.db").await;
        let base = ConvRow {
            id: "conv-1".into(),
            user_id: "u".into(),
            title: Some("old".into()),
            created_at: "2026-01-01T00:00:00.000Z".into(),
            updated_at: "2026-01-01T00:00:00.000Z".into(),
        };
        upsert_pulled_conversation(&pool, &base).await.unwrap();

        // older update must NOT overwrite
        let mut older = base.clone();
        older.title = Some("stale".into());
        older.updated_at = "2025-12-31T00:00:00.000Z".into();
        upsert_pulled_conversation(&pool, &older).await.unwrap();
        assert_eq!(title_of(&pool, "conv-1").await, "old");

        // newer update wins
        let mut newer = base.clone();
        newer.title = Some("new".into());
        newer.updated_at = "2026-02-01T00:00:00.000Z".into();
        upsert_pulled_conversation(&pool, &newer).await.unwrap();
        assert_eq!(title_of(&pool, "conv-1").await, "new");
    }

    async fn title_of(pool: &SqlitePool, id: &str) -> String {
        sqlx::query_scalar("SELECT title FROM conversations WHERE id = ?")
            .bind(id)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn sync_state_round_trips() {
        let pool = fresh_pool("firefly_test_state.db").await;
        let s0 = get_sync_state(&pool).await.unwrap();
        assert_eq!(s0.device_id, "");
        assert_eq!(s0.cursor, "");

        set_sync_identity(&pool, "dev-1", "user-1").await.unwrap();
        set_sync_cursor(&pool, "2026-06-06T14:03:21.118Z").await.unwrap();

        let s1 = get_sync_state(&pool).await.unwrap();
        assert_eq!(s1.device_id, "dev-1");
        assert_eq!(s1.user_id, "user-1");
        assert_eq!(s1.cursor, "2026-06-06T14:03:21.118Z");
    }

    #[test]
    fn now_iso_is_millisecond_z() {
        let ts = now_iso();
        // e.g. 2026-06-06T14:03:21.118Z
        assert!(ts.ends_with('Z'), "must end with Z: {ts}");
        assert!(ts.contains('T'), "must contain T: {ts}");
        // millisecond precision: ".###Z" => the 5th char from the end is '.'
        let dot = ts.as_bytes()[ts.len() - 5];
        assert_eq!(dot, b'.', "expected ms precision in {ts}");
        // and it round-trips through chrono's RFC3339 parser
        chrono::DateTime::parse_from_rfc3339(&ts).expect("parseable");
    }
}
