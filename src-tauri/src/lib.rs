mod error;
mod llm;
mod memory;
mod router;
mod secrets;
mod store;
mod sync;

use error::{AppError, Result};
use llm::{ChatMsg, StreamEvent};
use router::{RouteError, RouteInputs, TaskHint};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::sync::Arc;
use std::time::Instant;
use store::{Conversation, Message};
use tauri::ipc::Channel;
use tauri::{Manager, State};
use tokio::sync::Mutex;

const DEFAULT_ENDPOINT: &str = "http://firefly.taild9c345.ts.net:4000";
const DEFAULT_ON_DEVICE_ENDPOINT: &str = "http://localhost:11434";
const DEFAULT_ON_DEVICE_MODEL: &str = "qwen3.6:27b";
const DEFAULT_MODEL_CODE: &str = "code";
const DEFAULT_MODEL_CHAT_HEAVY: &str = "chat-heavy";
const DEFAULT_MODEL_FRONTIER: &str = "frontier";
const DEFAULT_SYNC_ENDPOINT: &str = "http://firefly.taild9c345.ts.net:8788";
const DEFAULT_DEVICE_NAME: &str = "firefly-device";
const DEFAULT_MEMORY_ENABLED: &str = "true";
const REACHABILITY_TTL: std::time::Duration = std::time::Duration::from_secs(5);

#[derive(Default)]
struct ReachabilityCache {
    checked_at: Option<Instant>,
    reachable: bool,
}

struct AppState {
    pool: SqlitePool,
    reachability: Mutex<ReachabilityCache>,
    sync_guard: Arc<Mutex<()>>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Settings {
    firefly_endpoint: String,
    on_device_endpoint: String,
    on_device_model: String,
    model_code: String,
    model_chat_heavy: String,
    model_frontier: String,
    sync_endpoint: String,
    device_name: String,
    memory_enabled: bool,
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
        sync_endpoint: get("sync_endpoint", DEFAULT_SYNC_ENDPOINT).await?,
        device_name: get("device_name", DEFAULT_DEVICE_NAME).await?,
        memory_enabled: get("memory_enabled", DEFAULT_MEMORY_ENABLED)
            .await?
            .parse()
            .unwrap_or(true),
    })
}

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

#[derive(Serialize)]
#[serde(rename_all = "camelCase", tag = "state")]
enum OnDevice {
    Ready { model: String },
    ModelMissing { model: String },
    Unreachable,
}

fn on_device_base(endpoint: &str) -> String {
    router::resolve_endpoint(endpoint)
        .trim_end_matches('/')
        .trim_end_matches("/v1")
        .to_string()
}

#[tauri::command]
async fn check_on_device(state: State<'_, AppState>) -> Result<OnDevice> {
    let s = load_settings(&state.pool).await?;
    let base = on_device_base(&s.on_device_endpoint);
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(1500))
        .build()
    {
        Ok(c) => c,
        Err(_) => return Ok(OnDevice::Unreachable),
    };
    let resp = match client.get(format!("{base}/api/tags")).send().await {
        Ok(r) => r,
        Err(_) => return Ok(OnDevice::Unreachable),
    };
    let tags: serde_json::Value = resp.json().await.unwrap_or_default();
    let present = tags["models"].as_array().is_some_and(|ms| {
        ms.iter()
            .any(|m| m["name"].as_str() == Some(s.on_device_model.as_str()))
    });
    Ok(if present {
        OnDevice::Ready { model: s.on_device_model }
    } else {
        OnDevice::ModelMissing { model: s.on_device_model }
    })
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct PullProgress {
    status: String,
    completed: Option<u64>,
    total: Option<u64>,
}

#[tauri::command]
async fn pull_on_device_model(
    state: State<'_, AppState>,
    on_pull: Channel<PullProgress>,
) -> Result<()> {
    let s = load_settings(&state.pool).await?;
    let base = on_device_base(&s.on_device_endpoint);
    let client = reqwest::Client::new();
    let mut resp = client
        .post(format!("{base}/api/pull"))
        .json(&serde_json::json!({ "model": s.on_device_model, "stream": true }))
        .send()
        .await
        .map_err(|e| AppError::Other(e.to_string()))?;
    // Ollama streams newline-delimited JSON status objects.
    while let Some(chunk) = resp.chunk().await.map_err(|e| AppError::Other(e.to_string()))? {
        for line in chunk.split(|&b| b == b'\n').filter(|l| !l.is_empty()) {
            if let Ok(p) = serde_json::from_slice::<serde_json::Value>(line) {
                on_pull
                    .send(PullProgress {
                        status: p["status"].as_str().unwrap_or_default().to_string(),
                        completed: p["completed"].as_u64(),
                        total: p["total"].as_u64(),
                    })
                    .ok();
            }
        }
    }
    Ok(())
}

#[tauri::command]
async fn list_conversations(state: State<'_, AppState>) -> Result<Vec<Conversation>> {
    store::list_conversations(&state.pool).await
}

#[tauri::command]
async fn get_messages(
    state: State<'_, AppState>,
    conversation_id: String,
) -> Result<Vec<Message>> {
    store::get_messages(&state.pool, &conversation_id).await
}

#[tauri::command]
async fn create_conversation(
    state: State<'_, AppState>,
    title: String,
) -> Result<Conversation> {
    store::create_conversation(&state.pool, &title).await
}

#[tauri::command]
fn set_token(token: String) -> Result<()> {
    secrets::set_token(&token)
}

#[tauri::command]
fn has_token() -> bool {
    secrets::has_token()
}

#[tauri::command]
fn set_sync_token(token: String) -> Result<()> {
    secrets::set_sync_token(&token)
}

#[tauri::command]
fn has_sync_token() -> bool {
    secrets::has_sync_token()
}

#[tauri::command]
async fn get_settings(state: State<'_, AppState>) -> Result<Settings> {
    load_settings(&state.pool).await
}

#[tauri::command]
async fn set_settings(state: State<'_, AppState>, settings: Settings) -> Result<()> {
    let pool = &state.pool;
    store::set_setting(pool, "firefly_endpoint", &settings.firefly_endpoint).await?;
    store::set_setting(pool, "on_device_endpoint", &settings.on_device_endpoint).await?;
    store::set_setting(pool, "on_device_model", &settings.on_device_model).await?;
    store::set_setting(pool, "model_code", &settings.model_code).await?;
    store::set_setting(pool, "model_chat_heavy", &settings.model_chat_heavy).await?;
    store::set_setting(pool, "model_frontier", &settings.model_frontier).await?;
    store::set_setting(pool, "sync_endpoint", &settings.sync_endpoint).await?;
    store::set_setting(pool, "device_name", &settings.device_name).await?;
    store::set_setting(
        pool,
        "memory_enabled",
        if settings.memory_enabled { "true" } else { "false" },
    )
    .await?;
    Ok(())
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct SyncSummary {
    ok: bool,
    pushed: usize,
    pulled: usize,
    cursor: String,
    message: Option<String>,
}

/// Push the outbound queue, pull deltas since the cursor, merge, advance the
/// cursor. Registers the device on first run. Skips quietly when offline, when
/// no sync token is set, or when another sync is already running. Network
/// failures are reported as `ok: false`, never as a hard error, so the UI can
/// show "offline" and retry later.
async fn do_sync(
    pool: &SqlitePool,
    sync_guard: &Mutex<()>,
    reachable: bool,
) -> SyncSummary {
    let off = |message: &str, cursor: String| SyncSummary {
        ok: false,
        pushed: 0,
        pulled: 0,
        cursor,
        message: Some(message.to_string()),
    };

    let _g = match sync_guard.try_lock() {
        Ok(g) => g,
        Err(_) => return off("sync already in progress", String::new()),
    };

    let state = match store::get_sync_state(pool).await {
        Ok(s) => s,
        Err(e) => return off(&e.to_string(), String::new()),
    };
    if !reachable {
        return off("offline", state.cursor);
    }
    let settings = match load_settings(pool).await {
        Ok(s) => s,
        Err(e) => return off(&e.to_string(), state.cursor),
    };
    let token = match secrets::get_sync_token() {
        Ok(t) => t,
        Err(_) => return off("no sync token set", state.cursor),
    };

    // 1. Register on first run.
    let (device_user, registered_now) = if state.device_id.is_empty() {
        match sync::register_device(&settings.sync_endpoint, &token, &settings.device_name, None)
            .await
        {
            Ok(reg) => {
                if let Err(e) = store::set_sync_identity(pool, &reg.device_id, &reg.user_id).await {
                    return off(&e.to_string(), state.cursor);
                }
                (reg.user_id, true)
            }
            Err(e) => return off(&e.to_string(), state.cursor),
        }
    } else {
        (state.user_id.clone(), false)
    };
    let _ = registered_now;

    // 2. Push the outbound queue. Marking rows pushed is not transactional with
    // the network push, so a crash mid-flush can re-send some rows next cycle.
    // That is safe: the sync service push is idempotent (messages are insert-only,
    // conversations/memories are last-write-wins by updated_at) per API-CONTRACT.md §3.3.
    let conv = match store::get_pending_conversations(pool, &device_user).await {
        Ok(c) => c,
        Err(e) => return off(&e.to_string(), state.cursor),
    };
    let msgs = match store::get_pending_messages(pool).await {
        Ok(m) => m,
        Err(e) => return off(&e.to_string(), state.cursor),
    };
    let pushed = conv.len() + msgs.len();
    if pushed > 0 {
        let msg_ids: Vec<String> = msgs.iter().map(|m| m.id.clone()).collect();
        if let Err(e) =
            sync::push(&settings.sync_endpoint, &token, conv.clone(), msgs.clone()).await
        {
            return off(&e.to_string(), state.cursor);
        }
        if let Err(e) = store::mark_conversations_pushed(pool, &conv).await {
            return off(&e.to_string(), state.cursor);
        }
        if let Err(e) = store::mark_messages_pushed(pool, &msg_ids).await {
            return off(&e.to_string(), state.cursor);
        }
    }

    // 3. Pull deltas and merge (conversations before messages for FK order).
    let pull = match sync::pull(&settings.sync_endpoint, &token, &state.cursor, &device_user).await
    {
        Ok(p) => p,
        Err(e) => return off(&e.to_string(), state.cursor),
    };
    let pulled = pull.conversations.len() + pull.messages.len() + pull.memories.len();
    for c in &pull.conversations {
        if let Err(e) = store::upsert_pulled_conversation(pool, c).await {
            return off(&e.to_string(), state.cursor);
        }
    }
    for m in &pull.messages {
        if let Err(e) = store::insert_pulled_message(pool, m).await {
            return off(&e.to_string(), state.cursor);
        }
    }
    for mem in &pull.memories {
        if let Err(e) = store::upsert_pulled_memory(pool, mem).await {
            return off(&e.to_string(), state.cursor);
        }
    }
    if let Err(e) = store::set_sync_cursor(pool, &pull.cursor).await {
        return off(&e.to_string(), pull.cursor);
    }

    SyncSummary {
        ok: true,
        pushed,
        pulled,
        cursor: pull.cursor,
        message: None,
    }
}

#[tauri::command]
async fn sync_now(state: State<'_, AppState>) -> Result<SyncSummary> {
    let settings = load_settings(&state.pool).await?;
    let reachable = firefly_reachable(&state, &settings.firefly_endpoint).await;
    Ok(do_sync(&state.pool, &state.sync_guard, reachable).await)
}

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
    let local_only = matches!(task, TaskHint::Private);
    store::insert_message(&state.pool, &conversation_id, "user", &content, !local_only, local_only).await?;

    let history = store::get_messages(&state.pool, &conversation_id).await?;
    let mut messages: Vec<ChatMsg> = history
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

    let assistant =
        store::insert_message(&state.pool, &conversation_id, "assistant", "", false, local_only).await?;
    store::set_message_routing(&state.pool, &assistant.id, route.tier.as_str(), &route.model).await?;

    if settings.memory_enabled && route.tier == router::Tier::HomeBase {
        if let Ok(sync_token) = secrets::get_sync_token() {
            let user_id = store::get_sync_state(&state.pool)
                .await
                .map(|s| s.user_id)
                .unwrap_or_default();
            match sync::search_memories(&settings.sync_endpoint, &sync_token, &content, &user_id, 8)
                .await
            {
                Ok(mems) => {
                    if let Some(sys) = memory::build_memory_message(&mems) {
                        messages.insert(0, sys);
                    }
                }
                Err(e) => eprintln!("memory search failed, sending without context: {e}"),
            }
        }
    }

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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let dir = app.path().app_data_dir().expect("resolve app data dir");
            std::fs::create_dir_all(&dir).ok();
            let db_path = dir.join("firefly.db");
            let handle = app.handle().clone();
            tauri::async_runtime::block_on(async move {
                let pool = store::init_pool(&db_path).await.expect("init sqlite pool");
                let sync_guard = Arc::new(Mutex::new(()));

                // Background sync loop: every 30s, push/pull when Firefly is reachable.
                let bg_pool = pool.clone();
                let bg_guard = sync_guard.clone();
                tauri::async_runtime::spawn(async move {
                    loop {
                        tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                        let reachable = match load_settings(&bg_pool).await {
                            Ok(s) => router::check_reachable(&s.firefly_endpoint).await,
                            Err(_) => false,
                        };
                        let _ = do_sync(&bg_pool, &bg_guard, reachable).await;
                    }
                });

                handle.manage(AppState {
                    pool,
                    reachability: Mutex::new(ReachabilityCache::default()),
                    sync_guard,
                });
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_conversations,
            get_messages,
            create_conversation,
            set_token,
            has_token,
            set_sync_token,
            has_sync_token,
            get_settings,
            set_settings,
            send_message,
            check_firefly,
            check_on_device,
            pull_on_device_model,
            sync_now,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
