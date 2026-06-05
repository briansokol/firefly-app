# Phase 2 — Client Router Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Route each chat request to the correct tier (on-device / home-base / cloud) based on a task hint and live Firefly reachability, enforce `private` as on-device-only in the Rust core, and show which tier+model actually served each response.

**Architecture:** A pure `resolve_route()` function in `router.rs` maps `(TaskHint, settings, firefly_reachable) → Route{tier, endpoint, model, use_token, degraded}` per the §6 table. `send_message` checks cached reachability, resolves the route, fetches the device token only for home-base/cloud, streams from the chosen endpoint, persists the served tier+model on the assistant message, and emits a `routed` event so the UI badge updates live. SQLite gains a versioned migration (`PRAGMA user_version`) adding `tier` + `served_model` columns.

**Tech Stack:** Rust (Tauri 2 commands), `reqwest` (reachability probe + streaming, already a dep), `tokio::sync::Mutex` (reachability cache), `sqlx` (migration), Svelte 5 runes (task selector + badges). No new dependencies.

---

## Context

Phase 1 shipped a working streaming chat client that always routes to Firefly's home-base LiteLLM with a single logical model. Phase 2 implements the routing brain described in `PLAN-app-build.md` §6: the client picks a **tier**, LiteLLM picks the model within home-base/cloud. This is the guardrail that keeps `private` requests on-device and degrades gracefully when Firefly is unreachable.

**Decisions locked with the user:**
- On-device tier defaults to Ollama `http://localhost:11434`, model `qwen3:30b` (both editable in Settings).
- `best` when Firefly is unreachable → **refuse with an error** (queueing deferred).
- Per-task home-base model names are **configurable** with §6 defaults (`code`, `chat-heavy`, `frontier`). Only `fast` is confirmed to exist on the live LiteLLM today; making these editable lets the user point them at real aliases without code changes. The server-side aliases are owned by `PLAN-firefly-upgrade.md` (not yet in this repo).
- Served tier+model is **persisted per message** so the badge survives an app restart.

**Verified current state (from exploration):** No automated tests exist yet. `tokio` is present with `full` features (provides `#[tokio::test]`, `sync::Mutex`). `reqwest` is present. `router.rs` only has `resolve_endpoint()`. `Settings` has `firefly_endpoint` + `logical_model`. `Message` has no tier/model fields. Exact signatures are referenced inline below.

---

## File Structure

| File | Responsibility | Change |
|---|---|---|
| `src-tauri/src/router.rs` | TaskHint/Tier/Route types, `resolve_route` (pure), reachability probe + cache freshness | Heavy add |
| `src-tauri/src/store.rs` | Versioned migration; `Message` gains `tier`/`served_model`; `set_message_routing` | Modify |
| `src-tauri/src/llm.rs` | `token: Option<&str>`; new `Routed` stream event | Modify |
| `src-tauri/src/lib.rs` | `Settings` expansion; reachability cache in `AppState`; `send_message` rewrite; `check_firefly` command | Heavy modify |
| `src/lib/api.ts` | `TaskHint`, updated `Settings`/`Message`/`StreamEvent`, `task` arg, `checkFirefly` | Modify |
| `src/lib/Chat.svelte` | Task selector, `routed` handling, per-message tier/model badge | Modify |
| `src/routes/+page.svelte` | Settings inputs for new fields; header connectivity badge | Modify |

`error.rs`, `secrets.rs`, `sync.rs`, `ConversationList.svelte`, `capabilities/default.json`, `Cargo.toml` are unchanged.

---

## Task 0: Branch

- [ ] **Step 1: Create the Phase 2 branch off `main`**

```bash
cd /Users/bsokol/WebProjects/firefly-app
git checkout main
git checkout -b phase-2-router
git status   # clean, on phase-2-router
```

---

## Task 1: Routing types + `resolve_route` (pure, TDD)

**Files:**
- Modify: `src-tauri/src/router.rs` (currently 13 lines: doc comment + `resolve_endpoint`)

This is the heart of Phase 2. `resolve_route` is pure (no IO), so it is fully unit-testable. Keep the existing `resolve_endpoint` fn — it is reused to normalize endpoints.

- [ ] **Step 1: Write the failing tests**

Append to `src-tauri/src/router.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn inputs(reachable: bool) -> RouteInputs<'static> {
        RouteInputs {
            firefly_endpoint: "firefly.taild9c345.ts.net:4000",
            on_device_endpoint: "http://localhost:11434",
            on_device_model: "qwen3:30b",
            model_code: "code",
            model_chat_heavy: "chat-heavy",
            model_frontier: "frontier",
            firefly_reachable: reachable,
        }
    }

    #[test]
    fn private_stays_on_device_even_when_firefly_up() {
        let r = resolve_route(TaskHint::Private, &inputs(true)).unwrap();
        assert_eq!(r.tier, Tier::OnDevice);
        assert_eq!(r.endpoint, "http://localhost:11434");
        assert_eq!(r.model, "qwen3:30b");
        assert!(!r.use_token);
        assert!(!r.degraded);
    }

    #[test]
    fn quick_is_always_on_device() {
        assert_eq!(resolve_route(TaskHint::Quick, &inputs(true)).unwrap().tier, Tier::OnDevice);
        assert_eq!(resolve_route(TaskHint::Quick, &inputs(false)).unwrap().tier, Tier::OnDevice);
    }

    #[test]
    fn agentic_hits_home_base_when_up() {
        let r = resolve_route(TaskHint::Agentic, &inputs(true)).unwrap();
        assert_eq!(r.tier, Tier::HomeBase);
        assert_eq!(r.endpoint, "http://firefly.taild9c345.ts.net:4000");
        assert_eq!(r.model, "chat-heavy");
        assert!(r.use_token);
        assert!(!r.degraded);
    }

    #[test]
    fn agentic_degrades_to_on_device_when_down() {
        let r = resolve_route(TaskHint::Agentic, &inputs(false)).unwrap();
        assert_eq!(r.tier, Tier::OnDevice);
        assert_eq!(r.model, "qwen3:30b");
        assert!(!r.use_token);
        assert!(r.degraded);
    }

    #[test]
    fn write_uses_code_model_on_home_base() {
        let r = resolve_route(TaskHint::Write, &inputs(true)).unwrap();
        assert_eq!(r.tier, Tier::HomeBase);
        assert_eq!(r.model, "code");
    }

    #[test]
    fn explain_file_degrades_when_down() {
        let r = resolve_route(TaskHint::ExplainFile, &inputs(false)).unwrap();
        assert_eq!(r.tier, Tier::OnDevice);
        assert!(r.degraded);
    }

    #[test]
    fn code_complete_prefers_on_device_when_configured() {
        let r = resolve_route(TaskHint::CodeComplete, &inputs(true)).unwrap();
        assert_eq!(r.tier, Tier::OnDevice);
        assert!(!r.degraded);
    }

    #[test]
    fn code_complete_uses_home_base_when_no_on_device_and_reachable() {
        let mut i = inputs(true);
        i.on_device_endpoint = "";
        let r = resolve_route(TaskHint::CodeComplete, &i).unwrap();
        assert_eq!(r.tier, Tier::HomeBase);
        assert_eq!(r.model, "code");
    }

    #[test]
    fn code_complete_errors_when_no_on_device_and_unreachable() {
        let mut i = inputs(false);
        i.on_device_endpoint = "";
        assert!(matches!(
            resolve_route(TaskHint::CodeComplete, &i),
            Err(RouteError::NotConfigured(_))
        ));
    }

    #[test]
    fn best_uses_cloud_when_up() {
        let r = resolve_route(TaskHint::Best, &inputs(true)).unwrap();
        assert_eq!(r.tier, Tier::Cloud);
        assert_eq!(r.endpoint, "http://firefly.taild9c345.ts.net:4000");
        assert_eq!(r.model, "frontier");
        assert!(r.use_token);
    }

    #[test]
    fn best_refuses_when_down() {
        assert!(matches!(
            resolve_route(TaskHint::Best, &inputs(false)),
            Err(RouteError::Refused(_))
        ));
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail to compile**

Run: `cd src-tauri && source "$HOME/.cargo/env" && cargo test`
Expected: FAIL — `cannot find type RouteInputs / TaskHint / Tier / RouteError / function resolve_route`.

- [ ] **Step 3: Write the types + `resolve_route`**

Insert at the TOP of `src-tauri/src/router.rs` (above the existing doc comment / `resolve_endpoint`):

```rust
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum TaskHint {
    Quick,
    CodeComplete,
    Write,
    ExplainFile,
    Agentic,
    Private,
    Best,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Tier {
    OnDevice,
    HomeBase,
    Cloud,
}

impl Tier {
    pub fn as_str(self) -> &'static str {
        match self {
            Tier::OnDevice => "on-device",
            Tier::HomeBase => "home-base",
            Tier::Cloud => "cloud",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Route {
    pub tier: Tier,
    pub endpoint: String,
    pub model: String,
    pub use_token: bool,
    pub degraded: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum RouteError {
    Refused(String),
    NotConfigured(String),
}

pub struct RouteInputs<'a> {
    pub firefly_endpoint: &'a str,
    pub on_device_endpoint: &'a str,
    pub on_device_model: &'a str,
    pub model_code: &'a str,
    pub model_chat_heavy: &'a str,
    pub model_frontier: &'a str,
    pub firefly_reachable: bool,
}

/// Map a task hint + settings + live reachability to a concrete route per
/// PLAN-app-build.md §6. Pure: no IO, fully unit-tested.
pub fn resolve_route(task: TaskHint, i: &RouteInputs) -> Result<Route, RouteError> {
    let on_device = |degraded: bool| Route {
        tier: Tier::OnDevice,
        endpoint: resolve_endpoint(i.on_device_endpoint),
        model: i.on_device_model.to_string(),
        use_token: false,
        degraded,
    };
    let home_base = |model: &str| Route {
        tier: Tier::HomeBase,
        endpoint: resolve_endpoint(i.firefly_endpoint),
        model: model.to_string(),
        use_token: true,
        degraded: false,
    };

    match task {
        // Privacy-critical: on-device only, always. Enforced here, not just in UI.
        TaskHint::Private => Ok(on_device(false)),
        // One-liners stay local regardless of reachability.
        TaskHint::Quick => Ok(on_device(false)),
        TaskHint::CodeComplete => {
            if !i.on_device_endpoint.trim().is_empty() {
                Ok(on_device(false))
            } else if i.firefly_reachable {
                Ok(home_base(i.model_code))
            } else {
                Err(RouteError::NotConfigured(
                    "code-complete needs an on-device endpoint, and Firefly is unreachable".into(),
                ))
            }
        }
        TaskHint::Write | TaskHint::ExplainFile => {
            if i.firefly_reachable {
                Ok(home_base(i.model_code))
            } else {
                Ok(on_device(true))
            }
        }
        TaskHint::Agentic => {
            if i.firefly_reachable {
                Ok(home_base(i.model_chat_heavy))
            } else {
                Ok(on_device(true))
            }
        }
        TaskHint::Best => {
            if i.firefly_reachable {
                Ok(Route {
                    tier: Tier::Cloud,
                    endpoint: resolve_endpoint(i.firefly_endpoint),
                    model: i.model_frontier.to_string(),
                    use_token: true,
                    degraded: false,
                })
            } else {
                Err(RouteError::Refused(
                    "best/frontier requires Firefly (cloud via home-base); it is unreachable".into(),
                ))
            }
        }
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd src-tauri && cargo test`
Expected: PASS — all 11 `router::tests::*` pass.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/router.rs
git commit -m "$(cat <<'EOF'
Add pure resolve_route tier-selection with full table-driven tests

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 2: Reachability probe + cache freshness (TDD for freshness)

**Files:**
- Modify: `src-tauri/src/router.rs`

The probe itself does network IO (not unit-tested); the cache-freshness decision is a pure fn that IS tested.

- [ ] **Step 1: Write the failing freshness test**

Add inside the existing `#[cfg(test)] mod tests` in `router.rs`:

```rust
    use std::time::{Duration, Instant};

    #[test]
    fn cache_is_stale_when_never_checked() {
        let now = Instant::now();
        assert!(!is_fresh(None, now, Duration::from_secs(5)));
    }

    #[test]
    fn cache_is_fresh_within_ttl() {
        let t0 = Instant::now();
        assert!(is_fresh(Some(t0), t0 + Duration::from_secs(2), Duration::from_secs(5)));
    }

    #[test]
    fn cache_is_stale_past_ttl() {
        let t0 = Instant::now();
        assert!(!is_fresh(Some(t0), t0 + Duration::from_secs(6), Duration::from_secs(5)));
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cd src-tauri && cargo test`
Expected: FAIL — `cannot find function is_fresh`.

- [ ] **Step 3: Implement `is_fresh` + `check_reachable`**

Add to `router.rs` (outside the tests module), near the top after the imports add `use std::time::{Duration, Instant};`:

```rust
/// Pure: is a cached reachability result still within its TTL?
pub fn is_fresh(checked_at: Option<Instant>, now: Instant, ttl: Duration) -> bool {
    match checked_at {
        Some(t) => now.duration_since(t) < ttl,
        None => false,
    }
}

/// Probe Firefly with a short timeout. Any HTTP response (even 401) means
/// reachable; only a connection/timeout failure means unreachable.
pub async fn check_reachable(firefly_endpoint: &str) -> bool {
    let url = format!("{}/health", resolve_endpoint(firefly_endpoint).trim_end_matches('/'));
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_millis(1500))
        .build()
    {
        Ok(c) => c,
        Err(_) => return false,
    };
    client.get(&url).send().await.is_ok()
}
```

> Note: if the top-of-file `use serde::{...}` was added in Task 1, just add the `std::time` import alongside it. Don't duplicate `Duration`/`Instant` imports between module scope and the test module — the test module references them via the module-scope import path `super::*` already brings them in, so the `use std::time::{Duration, Instant};` inside the tests module (Step 1) can be removed if it causes an unused/dup warning. Keep whichever single import compiles cleanly.

- [ ] **Step 4: Run to verify pass**

Run: `cd src-tauri && cargo test`
Expected: PASS — freshness tests green; `check_reachable` compiles (not exercised by tests).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/router.rs
git commit -m "$(cat <<'EOF'
Add Firefly reachability probe and pure cache-freshness check

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 3: Versioned migration + persist tier/model on messages (TDD)

**Files:**
- Modify: `src-tauri/src/store.rs`

Replace the single re-runnable `CREATE IF NOT EXISTS` block with a `PRAGMA user_version` migration runner: v1 = current schema (still `CREATE IF NOT EXISTS`, so existing DBs are fine), v2 = add `tier` + `served_model` columns to `messages`.

- [ ] **Step 1: Write the failing migration test**

Append to `src-tauri/src/store.rs`:

```rust
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
```

- [ ] **Step 2: Run to verify failure**

Run: `cd src-tauri && cargo test store::`
Expected: FAIL — `user_version` is 0 and/or `tier`/`served_model` columns missing (test assertions fail).

- [ ] **Step 3: Replace `init_pool` with a versioned runner**

In `src-tauri/src/store.rs`, keep the existing `MIGRATION` const (it becomes the v1 body) and replace the current `init_pool` (lines ~49-56) with:

```rust
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

    // PRAGMA does not accept bind params; SCHEMA_VERSION is a trusted constant.
    sqlx::query(&format!("PRAGMA user_version = {SCHEMA_VERSION}"))
        .execute(&pool)
        .await?;
    let _ = version;
    Ok(pool)
}
```

- [ ] **Step 4: Add routing fields to `Message` and a setter**

Update the `Message` struct (currently lines ~39-47) to add two nullable fields at the end:

```rust
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
```

Update `get_messages` (currently lines ~67-76) SELECT to include the new columns:

```rust
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
```

Update `insert_message` (currently lines ~96-126) to set the new columns to NULL on insert and return them as `None`. Change the INSERT and the returned struct:

```rust
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
```

and the returned `Message { ... }` gains `tier: None, served_model: None,`.

Add a new setter after `update_message_content` (after line ~139):

```rust
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
```

- [ ] **Step 5: Run to verify pass**

Run: `cd src-tauri && cargo test store::`
Expected: PASS — `migration_creates_tables_and_routing_columns` green.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/store.rs
git commit -m "$(cat <<'EOF'
Add user_version migration and persist served tier/model per message

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 4: `llm.rs` — optional token + `Routed` event

**Files:**
- Modify: `src-tauri/src/llm.rs`

- [ ] **Step 1: Add the `Routed` variant to `StreamEvent`**

Update the enum (currently lines ~14-23) to add `Routed` right after `Started`:

```rust
#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum StreamEvent {
    Started,
    // Which tier/model is serving this response (for the UI badge).
    Routed { tier: String, model: String, degraded: bool },
    // Reasoning ("thinking") tokens from reasoning models; shown but not persisted.
    Reasoning { text: String },
    Token { text: String },
    Done,
    Error { message: String },
}
```

- [ ] **Step 2: Make the token optional in `stream_chat`**

Change the signature (currently lines ~27-33) and the auth line. New signature:

```rust
pub async fn stream_chat(
    endpoint: &str,
    token: Option<&str>,
    model: &str,
    messages: Vec<ChatMsg>,
    channel: &Channel<StreamEvent>,
) -> Result<String> {
```

Replace the request builder's `.bearer_auth(token)` with conditional auth:

```rust
    let mut req = reqwest::Client::new()
        .post(&url)
        .header("Accept", "text/event-stream")
        .json(&body);
    if let Some(t) = token {
        req = req.bearer_auth(t);
    }
    let resp = req.send().await?;
```

(The rest of `stream_chat` — status check, SSE loop, reasoning/content handling, `Done` — is unchanged.)

- [ ] **Step 3: Verify it compiles (call site updates come in Task 5)**

Run: `cd src-tauri && cargo build`
Expected: FAIL at `lib.rs` `send_message` call site (`stream_chat` now takes `Option<&str>` and there's an unused `Routed`). That is expected and fixed in Task 5. Do NOT commit yet — combine with Task 5.

---

## Task 5: Wire `send_message` + `Settings` + `check_firefly` command

**Files:**
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Expand `Settings` and defaults**

Replace the consts + `Settings` struct + `load_settings` (currently lines ~16-40) with:

```rust
const DEFAULT_ENDPOINT: &str = "http://firefly.taild9c345.ts.net:4000";
const DEFAULT_ON_DEVICE_ENDPOINT: &str = "http://localhost:11434";
const DEFAULT_ON_DEVICE_MODEL: &str = "qwen3:30b";
const DEFAULT_MODEL_CODE: &str = "code";
const DEFAULT_MODEL_CHAT_HEAVY: &str = "chat-heavy";
const DEFAULT_MODEL_FRONTIER: &str = "frontier";
const REACHABILITY_TTL: std::time::Duration = std::time::Duration::from_secs(5);

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Settings {
    firefly_endpoint: String,
    on_device_endpoint: String,
    on_device_model: String,
    model_code: String,
    model_chat_heavy: String,
    model_frontier: String,
}

async fn load_settings(pool: &SqlitePool) -> Result<Settings> {
    let get = |key: &'static str, default: &'static str| {
        let pool = pool.clone();
        async move {
            store::get_setting(&pool, key)
                .await
                .map(|v| v.unwrap_or_else(|| default.to_string()))
        }
    };
    Ok(Settings {
        firefly_endpoint: get("firefly_endpoint", DEFAULT_ENDPOINT).await?,
        on_device_endpoint: get("on_device_endpoint", DEFAULT_ON_DEVICE_ENDPOINT).await?,
        on_device_model: get("on_device_model", DEFAULT_ON_DEVICE_MODEL).await?,
        model_code: get("model_code", DEFAULT_MODEL_CODE).await?,
        model_chat_heavy: get("model_chat_heavy", DEFAULT_MODEL_CHAT_HEAVY).await?,
        model_frontier: get("model_frontier", DEFAULT_MODEL_FRONTIER).await?,
    })
}
```

> The Phase-1 `logical_model` setting is dropped; any old `logical_model` row in the DB is simply ignored.

- [ ] **Step 2: Add a reachability cache to `AppState` and imports**

Update the top imports and `AppState` (currently lines ~8-20):

```rust
use error::{AppError, Result};
use llm::{ChatMsg, StreamEvent};
use router::{RouteError, RouteInputs, TaskHint};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::time::Instant;
use store::{Conversation, Message};
use tauri::ipc::Channel;
use tauri::{Manager, State};
use tokio::sync::Mutex;

#[derive(Default)]
struct ReachabilityCache {
    checked_at: Option<Instant>,
    reachable: bool,
}

struct AppState {
    pool: SqlitePool,
    reachability: Mutex<ReachabilityCache>,
}
```

Update the `setup` closure where `AppState` is constructed (currently `handle.manage(AppState { pool });`) to:

```rust
                handle.manage(AppState {
                    pool,
                    reachability: Mutex::new(ReachabilityCache::default()),
                });
```

- [ ] **Step 3: Add a cached reachability helper + `check_firefly` command**

Add near the other commands:

```rust
async fn firefly_reachable(state: &AppState, endpoint: &str) -> bool {
    let mut cache = state.reachability.lock().await;
    if router::is_fresh(cache.checked_at, Instant::now(), REACHABILITY_TTL) {
        return cache.reachable;
    }
    let reachable = router::check_reachable(endpoint).await;
    cache.checked_at = Some(Instant::now());
    cache.reachable = reachable;
    reachable
}

#[tauri::command]
async fn check_firefly(state: State<'_, AppState>) -> Result<bool> {
    let settings = load_settings(&state.pool).await?;
    Ok(firefly_reachable(&state, &settings.firefly_endpoint).await)
}
```

- [ ] **Step 4: Update `set_settings` to persist all fields**

Replace `set_settings` (currently lines ~78-83) with:

```rust
#[tauri::command]
async fn set_settings(state: State<'_, AppState>, settings: Settings) -> Result<()> {
    let pool = &state.pool;
    store::set_setting(pool, "firefly_endpoint", &settings.firefly_endpoint).await?;
    store::set_setting(pool, "on_device_endpoint", &settings.on_device_endpoint).await?;
    store::set_setting(pool, "on_device_model", &settings.on_device_model).await?;
    store::set_setting(pool, "model_code", &settings.model_code).await?;
    store::set_setting(pool, "model_chat_heavy", &settings.model_chat_heavy).await?;
    store::set_setting(pool, "model_frontier", &settings.model_frontier).await?;
    Ok(())
}
```

- [ ] **Step 5: Rewrite `send_message` to route**

Replace the entire `send_message` command (currently lines ~85-127) with:

```rust
/// Persist the user message, resolve the tier/route, stream the assistant reply
/// over `on_token`, persist the served tier+model and final text, and return the
/// assistant message id. The device token never leaves the Rust core, and is only
/// read for home-base/cloud routes.
#[tauri::command]
async fn send_message(
    state: State<'_, AppState>,
    conversation_id: String,
    content: String,
    task: TaskHint,
    on_token: Channel<StreamEvent>,
) -> Result<String> {
    store::insert_message(&state.pool, &conversation_id, "user", &content).await?;

    let history = store::get_messages(&state.pool, &conversation_id).await?;
    let messages: Vec<ChatMsg> = history
        .iter()
        .map(|m| ChatMsg {
            role: m.role.clone(),
            content: m.content.clone(),
        })
        .collect();

    let settings = load_settings(&state.pool).await?;
    let reachable = firefly_reachable(&state, &settings.firefly_endpoint).await;

    let inputs = RouteInputs {
        firefly_endpoint: &settings.firefly_endpoint,
        on_device_endpoint: &settings.on_device_endpoint,
        on_device_model: &settings.on_device_model,
        model_code: &settings.model_code,
        model_chat_heavy: &settings.model_chat_heavy,
        model_frontier: &settings.model_frontier,
        firefly_reachable: reachable,
    };
    let route = router::resolve_route(task, &inputs).map_err(|e| match e {
        RouteError::Refused(m) | RouteError::NotConfigured(m) => AppError::Other(m),
    })?;

    let token = if route.use_token {
        Some(secrets::get_token()?)
    } else {
        None
    };

    on_token.send(StreamEvent::Started).ok();
    on_token
        .send(StreamEvent::Routed {
            tier: route.tier.as_str().to_string(),
            model: route.model.clone(),
            degraded: route.degraded,
        })
        .ok();

    let assistant = store::insert_message(&state.pool, &conversation_id, "assistant", "").await?;
    store::set_message_routing(&state.pool, &assistant.id, route.tier.as_str(), &route.model).await?;

    let full = llm::stream_chat(
        &route.endpoint,
        token.as_deref(),
        &route.model,
        messages,
        &on_token,
    )
    .await?;
    store::update_message_content(&state.pool, &assistant.id, &full).await?;
    Ok(assistant.id)
}
```

- [ ] **Step 6: Register `check_firefly` in the handler**

Add `check_firefly,` to the `tauri::generate_handler![...]` list (currently lines ~140-149).

- [ ] **Step 7: Build + run all tests**

Run: `cd src-tauri && cargo build && cargo test`
Expected: PASS — compiles; all router + store tests green.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/llm.rs src-tauri/src/lib.rs
git commit -m "$(cat <<'EOF'
Route send_message by tier with cached reachability and optional token

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 6: Frontend API layer

**Files:**
- Modify: `src/lib/api.ts`

- [ ] **Step 1: Update types, add `task` arg and `checkFirefly`**

Replace the `Message`, `Settings`, `StreamEvent` blocks and `sendMessage`, and add `TaskHint` + `checkFirefly`:

```typescript
export type TaskHint =
  | "quick"
  | "code-complete"
  | "write"
  | "explain-file"
  | "agentic"
  | "private"
  | "best";

export interface Message {
  id: string;
  conversationId: string;
  role: "user" | "assistant" | "system";
  content: string;
  createdAt: string;
  tier?: string | null;
  servedModel?: string | null;
}

export interface Settings {
  fireflyEndpoint: string;
  onDeviceEndpoint: string;
  onDeviceModel: string;
  modelCode: string;
  modelChatHeavy: string;
  modelFrontier: string;
}

export type StreamEvent =
  | { type: "started" }
  | { type: "routed"; tier: string; model: string; degraded: boolean }
  | { type: "reasoning"; text: string }
  | { type: "token"; text: string }
  | { type: "done" }
  | { type: "error"; message: string };

export const checkFirefly = () => invoke<boolean>("check_firefly");

export function sendMessage(
  conversationId: string,
  content: string,
  task: TaskHint,
  onEvent: (event: StreamEvent) => void,
): Promise<string> {
  const channel = new Channel<StreamEvent>();
  channel.onmessage = onEvent;
  return invoke<string>("send_message", {
    conversationId,
    content,
    task,
    onToken: channel,
  });
}
```

(Keep `listConversations`, `getMessages`, `createConversation`, `setToken`, `hasToken`, `getSettings`, `setSettings` unchanged.)

- [ ] **Step 2: Typecheck**

Run: `cd /Users/bsokol/WebProjects/firefly-app && npm run check`
Expected: errors in `Chat.svelte` and `+page.svelte` (they use the old `sendMessage` arity / old `Settings` shape). Fixed in Tasks 7–8.

---

## Task 7: Task selector + routing badge in Chat

**Files:**
- Modify: `src/lib/Chat.svelte`

- [ ] **Step 1: Add task state, import, and badge fields to the optimistic message**

In the `<script>` block: update the import and `ChatMessage` type, add a `task` state:

```svelte
  import {
    getMessages,
    sendMessage,
    type Message,
    type TaskHint,
  } from "./api";

  // `reasoning` is ephemeral; tier/servedModel/degraded drive the badge.
  type ChatMessage = Message & { reasoning?: string; degraded?: boolean };

  let { conversationId }: { conversationId: string | null } = $props();

  let messages = $state<ChatMessage[]>([]);
  let draft = $state("");
  let task = $state<TaskHint>("agentic");
  let sending = $state(false);
  let error = $state<string | null>(null);
```

- [ ] **Step 2: Pass `task` and handle the `routed` event**

In `submit()`, change the `sendMessage` call and event handler:

```svelte
      await sendMessage(id, content, task, (e) => {
        if (e.type === "token") {
          messages[assistantIdx].content += e.text;
        } else if (e.type === "reasoning") {
          messages[assistantIdx].reasoning =
            (messages[assistantIdx].reasoning ?? "") + e.text;
        } else if (e.type === "routed") {
          messages[assistantIdx].tier = e.tier;
          messages[assistantIdx].servedModel = e.model;
          messages[assistantIdx].degraded = e.degraded;
        } else if (e.type === "error") {
          error = e.message;
        }
      });
```

- [ ] **Step 3: Render the badge and the task selector**

In the markup, inside the assistant message block, add a badge above the bubble (after the existing `.role` div):

```svelte
          {#if m.role === "assistant" && m.tier}
            <div class="badge" class:degraded={m.degraded}>
              {m.tier}{m.servedModel ? ` · ${m.servedModel}` : ""}{m.degraded ? " · degraded" : ""}
            </div>
          {/if}
```

In the composer `<form>`, add a task selector before the textarea:

```svelte
      <select bind:value={task} class="task" title="Task tier">
        <option value="agentic">agentic</option>
        <option value="write">write</option>
        <option value="explain-file">explain-file</option>
        <option value="code-complete">code-complete</option>
        <option value="quick">quick</option>
        <option value="private">private (on-device)</option>
        <option value="best">best (cloud)</option>
      </select>
```

Add styles in the `<style>` block:

```svelte
  .badge {
    display: inline-block;
    font-size: 0.65rem;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--muted);
    border: 1px solid var(--border);
    border-radius: 999px;
    padding: 0.05rem 0.5rem;
    margin-bottom: 0.3rem;
  }
  .badge.degraded {
    color: #ffd7a8;
    border-color: #7a5a2a;
  }
  .task {
    border-radius: 8px;
    border: 1px solid var(--border);
    background: var(--panel);
    color: var(--text);
    padding: 0 0.4rem;
    font-family: inherit;
  }
```

- [ ] **Step 4: Typecheck**

Run: `npm run check`
Expected: remaining errors only in `+page.svelte` (Settings shape). Chat is clean.

---

## Task 8: Settings inputs + connectivity badge

**Files:**
- Modify: `src/routes/+page.svelte`

- [ ] **Step 1: Update Settings state default and add fields + reachability**

In the `<script>` block, update the `settings` default and add a reachability state + import:

```svelte
  import {
    listConversations,
    createConversation,
    getSettings,
    setSettings,
    setToken,
    hasToken,
    checkFirefly,
    type Conversation,
    type Settings,
  } from "$lib/api";

  let settings = $state<Settings>({
    fireflyEndpoint: "",
    onDeviceEndpoint: "",
    onDeviceModel: "",
    modelCode: "",
    modelChatHeavy: "",
    modelFrontier: "",
  });
  let reachable = $state<boolean | null>(null);
```

- [ ] **Step 2: Probe reachability on mount and persist new fields on save**

In the existing mount `$effect`, after `settings = await getSettings();` add:

```svelte
      checkFirefly().then((r) => (reachable = r));
```

Replace the `setSettings({...})` call inside `saveSettings()` with the full object:

```svelte
    await setSettings({
      fireflyEndpoint: settings.fireflyEndpoint,
      onDeviceEndpoint: settings.onDeviceEndpoint,
      onDeviceModel: settings.onDeviceModel,
      modelCode: settings.modelCode,
      modelChatHeavy: settings.modelChatHeavy,
      modelFrontier: settings.modelFrontier,
    });
```

After the token block in `saveSettings()`, re-probe: add `reachable = await checkFirefly();` before setting `savedNote`.

- [ ] **Step 3: Add the connectivity badge to the header and the new settings inputs**

In the header markup, replace the existing `.model` span with a connectivity badge:

```svelte
      <span class="conn" class:down={reachable === false}>
        {reachable === null ? "…" : reachable ? "Firefly online" : "Firefly offline"}
      </span>
```

In the `.settings` panel, after the existing Firefly endpoint input, add inputs for the new fields (each wrapped in a `<label>` like the existing ones):

```svelte
        <label>On-device endpoint
          <input bind:value={settings.onDeviceEndpoint} spellcheck="false" />
        </label>
        <label>On-device model
          <input bind:value={settings.onDeviceModel} spellcheck="false" />
        </label>
        <label>Home-base model — code/write
          <input bind:value={settings.modelCode} spellcheck="false" />
        </label>
        <label>Home-base model — agentic
          <input bind:value={settings.modelChatHeavy} spellcheck="false" />
        </label>
        <label>Cloud model — best
          <input bind:value={settings.modelFrontier} spellcheck="false" />
        </label>
```

Add styles:

```svelte
  .conn {
    font-size: 0.72rem;
    color: #9fe0a0;
    border: 1px solid var(--border);
    border-radius: 999px;
    padding: 0.1rem 0.5rem;
  }
  .conn.down {
    color: #ffb3b3;
  }
```

- [ ] **Step 4: Typecheck**

Run: `npm run check`
Expected: 0 errors (1 pre-existing `node` types warning is fine).

- [ ] **Step 5: Commit frontend**

```bash
git add src/lib/api.ts src/lib/Chat.svelte src/routes/+page.svelte
git commit -m "$(cat <<'EOF'
Add task selector, routing badge, connectivity indicator, on-device settings

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 9: End-to-end verification

- [ ] **Step 1: Tests + typecheck**

```bash
cd /Users/bsokol/WebProjects/firefly-app/src-tauri && source "$HOME/.cargo/env" && cargo test
cd /Users/bsokol/WebProjects/firefly-app && npm run check
```
Expected: all Rust tests pass; svelte-check 0 errors.

- [ ] **Step 2: Run the app**

```bash
cd /Users/bsokol/WebProjects/firefly-app && source "$HOME/.cargo/env" && npm run tauri dev
```

- [ ] **Step 3: Verify the acceptance criteria (manual, in the app window)**

1. **agentic → home-base (Firefly up):** header shows "Firefly online". Select task `agentic`, send a message. The assistant badge reads `home-base · chat-heavy`. (If LiteLLM lacks a `chat-heavy` alias and you get a model error, set **Home-base model — agentic** to `fast` in Settings and resend — tier routing is what's under test.)
2. **private → on-device even when Firefly is up:** select `private`, send. Badge reads `on-device · qwen3:30b`; the request hits Ollama at localhost (requires `ollama serve` running with `qwen3:30b`). Confirm it never contacts Firefly.
3. **degrade when Firefly unreachable:** in Settings set **Firefly endpoint** to `http://firefly.taild9c345.ts.net:4099` (wrong port) and Save → header flips to "Firefly offline" within ~5s. Select `agentic`, send → badge reads `on-device · qwen3:30b · degraded`. Restore the endpoint afterward.
4. **best refuses offline:** with the bad endpoint still set, select `best`, send → an inline error appears ("best/frontier requires Firefly…"). Restore endpoint; `best` then routes `cloud · frontier`.
5. **badge persists across restart:** quit the app, relaunch, reopen a conversation → assistant messages still show their tier/model badge (loaded from the `tier`/`served_model` columns).

- [ ] **Step 4: Finish the branch**

Use **superpowers:finishing-a-development-branch**. Per the user's stated preference, this phase is delivered via Pull Request — choose "Push and create a Pull Request" (a GitHub remote must be added first; if none exists, create the repo with `gh repo create` or keep the branch and open the PR once a remote is configured).

---

## Self-Review (spec coverage)

- §6 row "quick" → `quick_is_always_on_device` test + `Quick` arm. ✓
- §6 row "code-complete" → `code_complete_*` tests + `CodeComplete` arm. ✓
- §6 rows "write"/"explain-file" → `write_*`/`explain_file_*` tests + arms. ✓
- §6 row "agentic" → `agentic_*` tests + arm. ✓
- §6 row "private" (on-device only) → `private_stays_on_device_even_when_firefly_up` + enforced in `resolve_route` (Rust core, not UI). ✓
- §6 row "best" (refuse offline) → `best_*` tests + arm. ✓
- Phase 2 task "tier table + task selector in UI" → Tasks 1, 7. ✓
- Phase 2 task "reachability check with short timeout + brief cache" → Task 2 (`check_reachable`, 1500ms) + Task 5 (`firefly_reachable`, 5s TTL). ✓
- Phase 2 task "on-device endpoint settings per device + fallback" → Tasks 4, 8 + degrade arms. ✓
- Phase 2 task "enforce private in router" → `Private` arm returns on-device unconditionally. ✓
- Phase 2 task "UI badge for served tier/model" → Tasks 3 (persist), 4 (`Routed` event), 7 (render). ✓
- Acceptance (agentic up / degrade down / private never leaves) → Task 9 Step 3. ✓
