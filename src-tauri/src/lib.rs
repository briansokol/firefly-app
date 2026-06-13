mod error;
mod llm;
mod memory;
mod naming;
mod router;
mod secrets;
mod store;
mod sync;

use error::{AppError, Result};
use llm::{ChatMsg, StreamEvent};
use router::{Profile, RouteError, RouteInputs, TaskHint};
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
const DEFAULT_CONVERSATION_TITLE: &str = "New conversation";
const REACHABILITY_TTL: std::time::Duration = std::time::Duration::from_secs(5);

#[derive(Default)]
struct ReachabilityCache {
    checked_at: Option<Instant>,
    reachable: bool,
}

/// In-flight onboarding state held only in memory between the auth step
/// (signup/login) and the device step (register/claim). Never persisted: the
/// session token is short-lived and only authorizes the device call. If the
/// app closes mid-onboarding nothing is committed and onboarding restarts.
#[derive(Clone)]
struct PendingAuth {
    display_name: String,
    profile: String,
    session_token: String,
}

struct AppState {
    pool: SqlitePool,
    reachability: Mutex<ReachabilityCache>,
    sync_guard: Arc<Mutex<()>>,
    pending_auth: Mutex<Option<PendingAuth>>,
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
fn platform() -> String {
    std::env::consts::OS.to_string()
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
    Ok(probe_on_device(&s.on_device_endpoint, &s.on_device_model).await)
}

/// Probe the on-device model server (Ollama) for readiness. Connection failure or
/// timeout -> `Unreachable`; reachable but the configured model not pulled ->
/// `ModelMissing`; both present -> `Ready`.
async fn probe_on_device(endpoint: &str, model: &str) -> OnDevice {
    let base = on_device_base(endpoint);
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(1500))
        .build()
    {
        Ok(c) => c,
        Err(_) => return OnDevice::Unreachable,
    };
    let resp = match client.get(format!("{base}/api/tags")).send().await {
        Ok(r) => r,
        Err(_) => return OnDevice::Unreachable,
    };
    let tags: serde_json::Value = resp.json().await.unwrap_or_default();
    let present = tags["models"]
        .as_array()
        .is_some_and(|ms| ms.iter().any(|m| m["name"].as_str() == Some(model)));
    if present {
        OnDevice::Ready { model: model.to_string() }
    } else {
        OnDevice::ModelMissing { model: model.to_string() }
    }
}

/// On-device readiness used for routing. iOS has no on-device target, so it is
/// always treated as unreachable there without probing.
async fn on_device_state(settings: &Settings) -> OnDevice {
    if cfg!(target_os = "ios") {
        return OnDevice::Unreachable;
    }
    probe_on_device(&settings.on_device_endpoint, &settings.on_device_model).await
}

/// Actionable message when a route required on-device but the local model server
/// isn't ready. `None` -> use the router's own message (on iOS, where there is no
/// on-device target, or when on-device was actually ready).
fn on_device_hint(state: &OnDevice) -> Option<String> {
    if cfg!(target_os = "ios") {
        return None;
    }
    match state {
        OnDevice::Unreachable => Some(
            "the on-device model server isn't running. Start it (e.g. `ollama serve`) and try again."
                .into(),
        ),
        OnDevice::ModelMissing { model } => Some(format!(
            "the on-device model \"{model}\" isn't downloaded yet. Pull it before using on-device chat."
        )),
        OnDevice::Ready { .. } => None,
    }
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

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ProfileDto {
    user_id: String,
    display_name: String,
    profile: String,
    active: bool,
}

/// Re-derive a user's kid/adult profile from the models its LiteLLM key resolves.
/// Best-effort: leaves the stored profile unchanged if the key or Firefly is
/// unavailable. `adult` iff the key can resolve `code` or `frontier`.
async fn refresh_profile(pool: &SqlitePool, settings: &Settings, user_id: &str) {
    let Ok(key) = secrets::get_litellm_key(user_id) else { return };
    let endpoint = router::resolve_endpoint(&settings.firefly_endpoint);
    if let Ok(models) = llm::list_models(&endpoint, &key).await {
        let adult = models
            .iter()
            .any(|m| m == &settings.model_code || m == &settings.model_frontier);
        let profile = if adult { "adult" } else { "kid" };
        let _ = store::set_user_profile(pool, user_id, profile).await;
    }
}

async fn profiles_dto(pool: &SqlitePool) -> Result<Vec<ProfileDto>> {
    let active = store::get_active_user_id(pool).await?;
    let users = store::list_users(pool).await?;
    Ok(users
        .into_iter()
        .map(|u| ProfileDto {
            active: active.as_deref() == Some(u.user_id.as_str()),
            user_id: u.user_id,
            display_name: u.display_name,
            profile: u.profile,
        })
        .collect())
}

#[tauri::command]
async fn list_profiles(state: State<'_, AppState>) -> Result<Vec<ProfileDto>> {
    profiles_dto(&state.pool).await
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct DeviceDto {
    id: String,
    name: String,
    last_sync: Option<String>,
}

/// Persist freshly-issued device credentials + user row, activate the user,
/// clear the pending auth, and return the refreshed profile list.
async fn commit_device(
    state: &AppState,
    settings: &Settings,
    pending: &PendingAuth,
    cred: sync::DeviceCredentials,
) -> Result<Vec<ProfileDto>> {
    secrets::set_device_token(&cred.user_id, &cred.device_token)?;
    secrets::set_litellm_key(&cred.user_id, &cred.litellm_key)?;
    store::upsert_user(
        &state.pool,
        &cred.user_id,
        &cred.device_id,
        &pending.display_name,
        &pending.profile,
    )
    .await?;
    store::set_active_user_id(&state.pool, &cred.user_id).await?;
    refresh_profile(&state.pool, settings, &cred.user_id).await;
    *state.pending_auth.lock().await = None;
    profiles_dto(&state.pool).await
}

#[tauri::command]
async fn signup(
    state: State<'_, AppState>,
    username: String,
    password: String,
    display_name: String,
) -> Result<()> {
    let settings = load_settings(&state.pool).await?;
    let auth = sync::signup(
        &settings.sync_endpoint,
        username.trim(),
        &password,
        display_name.trim(),
    )
    .await?;
    *state.pending_auth.lock().await = Some(PendingAuth {
        display_name: auth.display_name,
        profile: auth.profile,
        session_token: auth.session_token,
    });
    Ok(())
}

#[tauri::command]
async fn login(
    state: State<'_, AppState>,
    username: String,
    password: String,
) -> Result<Vec<DeviceDto>> {
    let settings = load_settings(&state.pool).await?;
    let result = sync::login(&settings.sync_endpoint, username.trim(), &password).await?;
    let devices = result
        .devices
        .into_iter()
        .map(|d| DeviceDto {
            id: d.id,
            name: d.name,
            last_sync: d.last_sync,
        })
        .collect();
    *state.pending_auth.lock().await = Some(PendingAuth {
        display_name: result.auth.display_name,
        profile: result.auth.profile,
        session_token: result.auth.session_token,
    });
    Ok(devices)
}

#[tauri::command]
async fn register_device(state: State<'_, AppState>, name: String) -> Result<Vec<ProfileDto>> {
    let settings = load_settings(&state.pool).await?;
    // Clone out of the guard and DROP it before the network call / commit_device
    // (tokio Mutex is not reentrant; commit_device re-locks pending_auth).
    let pending = {
        let guard = state.pending_auth.lock().await;
        guard
            .clone()
            .ok_or_else(|| AppError::Other("no pending authentication; restart onboarding".into()))?
    };
    let cred =
        sync::register_device(&settings.sync_endpoint, &pending.session_token, name.trim()).await?;
    commit_device(&state, &settings, &pending, cred).await
}

#[tauri::command]
async fn claim_device(state: State<'_, AppState>, device_id: String) -> Result<Vec<ProfileDto>> {
    let settings = load_settings(&state.pool).await?;
    let pending = {
        let guard = state.pending_auth.lock().await;
        guard
            .clone()
            .ok_or_else(|| AppError::Other("no pending authentication; restart onboarding".into()))?
    };
    let cred =
        sync::claim_device(&settings.sync_endpoint, &pending.session_token, &device_id).await?;
    commit_device(&state, &settings, &pending, cred).await
}

#[tauri::command]
async fn switch_profile(state: State<'_, AppState>, user_id: String) -> Result<Vec<ProfileDto>> {
    if store::get_user(&state.pool, &user_id).await?.is_none() {
        return Err(AppError::Other("unknown profile".into()));
    }
    store::set_active_user_id(&state.pool, &user_id).await?;
    let settings = load_settings(&state.pool).await?;
    refresh_profile(&state.pool, &settings, &user_id).await;
    profiles_dto(&state.pool).await
}

#[tauri::command]
async fn refresh_active_profile(state: State<'_, AppState>) -> Result<Vec<ProfileDto>> {
    if let Some(active) = store::get_active_user_id(&state.pool).await? {
        let settings = load_settings(&state.pool).await?;
        refresh_profile(&state.pool, &settings, &active).await;
    }
    profiles_dto(&state.pool).await
}

/// Clear the active profile's local identity (keychain tokens + user row) and
/// activate a remaining profile, or drop to onboarding if none remain. Local
/// only: no server-side device removal, so it works offline. This is also the
/// migration path off legacy device-self-registration tokens.
#[tauri::command]
async fn sign_out(state: State<'_, AppState>) -> Result<Vec<ProfileDto>> {
    if let Some(user_id) = store::get_active_user_id(&state.pool).await? {
        // Best-effort keychain cleanup: a missing entry is fine.
        let _ = secrets::delete_device_token(&user_id);
        let _ = secrets::delete_litellm_key(&user_id);
        store::delete_user(&state.pool, &user_id).await?;

        match store::list_users(&state.pool).await?.first() {
            Some(next) => store::set_active_user_id(&state.pool, &next.user_id).await?,
            None => store::clear_active_user_id(&state.pool).await?,
        }
    }
    profiles_dto(&state.pool).await
}

#[tauri::command]
async fn list_conversations(state: State<'_, AppState>) -> Result<Vec<Conversation>> {
    match store::get_active_user_id(&state.pool).await? {
        Some(uid) => store::list_conversations(&state.pool, &uid).await,
        None => Ok(Vec::new()),
    }
}

#[tauri::command]
async fn get_messages(
    state: State<'_, AppState>,
    conversation_id: String,
) -> Result<Vec<Message>> {
    let active = store::get_active_user_id(&state.pool)
        .await?
        .ok_or_else(|| AppError::Other("no active profile".into()))?;
    match store::conversation_user_id(&state.pool, &conversation_id).await? {
        Some(owner) if owner == active => store::get_messages(&state.pool, &conversation_id).await,
        _ => Err(AppError::Other("conversation not found for this profile".into())),
    }
}

#[tauri::command]
async fn create_conversation(state: State<'_, AppState>, title: String) -> Result<Conversation> {
    let uid = store::get_active_user_id(&state.pool)
        .await?
        .ok_or_else(|| AppError::Other("no active profile".into()))?;
    store::create_conversation(&state.pool, &title, &uid).await
}

#[tauri::command]
async fn rename_conversation(
    state: State<'_, AppState>,
    conversation_id: String,
    title: String,
) -> Result<Conversation> {
    let active = store::get_active_user_id(&state.pool)
        .await?
        .ok_or_else(|| AppError::Other("no active profile".into()))?;
    match store::conversation_user_id(&state.pool, &conversation_id).await? {
        Some(owner) if owner == active => {}
        _ => return Err(AppError::Other("conversation not found for this profile".into())),
    }
    let title = title.trim();
    if title.is_empty() {
        return Err(AppError::Other("title cannot be empty".into()));
    }
    store::update_conversation_title(&state.pool, &conversation_id, title).await
}

#[tauri::command]
async fn delete_conversation(
    state: State<'_, AppState>,
    conversation_id: String,
) -> Result<()> {
    let active = store::get_active_user_id(&state.pool)
        .await?
        .ok_or_else(|| AppError::Other("no active profile".into()))?;
    match store::conversation_user_id(&state.pool, &conversation_id).await? {
        Some(owner) if owner == active => {}
        _ => return Err(AppError::Other("conversation not found for this profile".into())),
    }
    store::soft_delete_conversation(&state.pool, &conversation_id).await
}

#[tauri::command]
async fn generate_conversation_title(
    state: State<'_, AppState>,
    conversation_id: String,
    first_message: String,
) -> Result<Conversation> {
    let active = store::get_active_user_id(&state.pool)
        .await?
        .ok_or_else(|| AppError::Other("no active profile".into()))?;
    match store::conversation_user_id(&state.pool, &conversation_id).await? {
        Some(owner) if owner == active => {}
        _ => return Err(AppError::Other("conversation not found for this profile".into())),
    }

    let conv = store::get_conversation(&state.pool, &conversation_id)
        .await?
        .ok_or_else(|| AppError::Other("conversation not found".into()))?;
    // Only auto-name a freshly created, still-default conversation; never clobber
    // a title the user (or a prior run) already set.
    if conv.title != DEFAULT_CONVERSATION_TITLE {
        return Ok(conv);
    }

    let settings = load_settings(&state.pool).await?;
    let reachable = firefly_reachable(&state, &settings.firefly_endpoint).await;
    let user = store::get_user(&state.pool, &active)
        .await?
        .ok_or_else(|| AppError::Other("active profile not found".into()))?;
    let profile = if user.profile == "adult" { Profile::Adult } else { Profile::Kid };

    let on_device = on_device_state(&settings).await;
    let inputs = RouteInputs {
        firefly_endpoint: &settings.firefly_endpoint,
        on_device_endpoint: &settings.on_device_endpoint,
        on_device_model: &settings.on_device_model,
        model_code: &settings.model_code,
        model_chat_heavy: &settings.model_chat_heavy,
        model_frontier: &settings.model_frontier,
        firefly_reachable: reachable,
        on_device_available: matches!(on_device, OnDevice::Ready { .. }),
        profile,
    };
    // Quick resolves on-device on desktop; on iOS it has no target. When the local
    // model server isn't ready (offline or model not pulled), naming is skipped.
    let route = match router::resolve_route(TaskHint::Quick, &inputs) {
        Ok(r) => r,
        Err(_) => return Ok(conv),
    };

    let messages = vec![
        ChatMsg { role: "system".into(), content: naming::NAMING_SYSTEM_PROMPT.into() },
        ChatMsg { role: "user".into(), content: first_message },
    ];
    let raw = llm::complete_chat(&route.endpoint, None, &route.model, messages).await?;

    match naming::clean_title(&raw) {
        Some(title) => store::update_conversation_title(&state.pool, &conversation_id, &title).await,
        None => Ok(conv),
    }
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

/// Push the outbound queue and pull deltas for every registered user.
/// Skips quietly when offline or when another sync is already running.
/// Network failures are reported as `ok: false`, never as a hard error,
/// so the UI can show "offline" and retry later.
async fn do_sync(pool: &SqlitePool, sync_guard: &Mutex<()>, reachable: bool) -> SyncSummary {
    let off = |message: &str| SyncSummary {
        ok: false,
        pushed: 0,
        pulled: 0,
        cursor: String::new(),
        message: Some(message.to_string()),
    };

    let _g = match sync_guard.try_lock() {
        Ok(g) => g,
        Err(_) => return off("sync already in progress"),
    };
    if !reachable {
        return off("offline");
    }
    let settings = match load_settings(pool).await {
        Ok(s) => s,
        Err(e) => return off(&e.to_string()),
    };
    let users = match store::list_users(pool).await {
        Ok(u) => u,
        Err(e) => return off(&e.to_string()),
    };
    if users.is_empty() {
        return off("no profiles");
    }

    let mut pushed = 0usize;
    let mut pulled = 0usize;
    let mut errors: Vec<String> = Vec::new();

    // Each user syncs independently with its own device token and cursor; a
    // failure for one user is logged and does not block the others.
    for u in users {
        let token = match secrets::get_device_token(&u.user_id) {
            Ok(t) => t,
            Err(_) => {
                eprintln!("sync: no device token for {}, skipping", u.user_id);
                continue;
            }
        };
        match sync_user(pool, &settings, &token, &u).await {
            Ok((p, q)) => {
                pushed += p;
                pulled += q;
            }
            Err(e) => {
                eprintln!("sync: user {} failed: {e}", u.user_id);
                errors.push(format!("{}: {e}", u.user_id));
            }
        }
    }

    if errors.is_empty() {
        SyncSummary { ok: true, pushed, pulled, cursor: String::new(), message: None }
    } else {
        SyncSummary {
            ok: false,
            pushed,
            pulled,
            cursor: String::new(),
            message: Some(errors.join("; ")),
        }
    }
}

/// Push one user's pending rows and pull/merge deltas since its cursor. Returns
/// (pushed_count, pulled_count). An error aborts only this user's sync.
async fn sync_user(
    pool: &SqlitePool,
    settings: &Settings,
    token: &str,
    u: &store::User,
) -> Result<(usize, usize)> {
    let mut pushed = 0usize;
    let conv = store::get_pending_conversations(pool, &u.user_id).await?;
    let msgs = store::get_pending_messages(pool, &u.user_id).await?;
    if !conv.is_empty() || !msgs.is_empty() {
        let msg_ids: Vec<String> = msgs.iter().map(|m| m.id.clone()).collect();
        sync::push(&settings.sync_endpoint, token, conv.clone(), msgs.clone()).await?;
        store::mark_conversations_pushed(pool, &conv).await?;
        store::mark_messages_pushed(pool, &msg_ids).await?;
        pushed = conv.len() + msgs.len();
    }
    let pull = sync::pull(&settings.sync_endpoint, token, &u.cursor, &u.user_id).await?;
    let pulled = pull.conversations.len() + pull.messages.len() + pull.memories.len();
    for c in &pull.conversations {
        store::upsert_pulled_conversation(pool, c).await?;
    }
    for m in &pull.messages {
        store::insert_pulled_message(pool, m).await?;
    }
    for mem in &pull.memories {
        store::upsert_pulled_memory(pool, mem).await?;
    }
    store::set_user_cursor(pool, &u.user_id, &pull.cursor).await?;
    Ok((pushed, pulled))
}

#[tauri::command]
async fn sync_now(state: State<'_, AppState>) -> Result<SyncSummary> {
    let settings = load_settings(&state.pool).await?;
    let reachable = firefly_reachable(&state, &settings.firefly_endpoint).await;
    Ok(do_sync(&state.pool, &state.sync_guard, reachable).await)
}

/// Persist the user message, resolve the tier/route, stream the assistant reply
/// over `on_token`, persist the served tier+model and final text, and return the
/// assistant message id. Secrets never leave the Rust core, and are only
/// read for home-base/cloud routes.
#[tauri::command]
async fn send_message(
    state: State<'_, AppState>,
    conversation_id: String,
    content: String,
    task: TaskHint,
    on_token: Channel<StreamEvent>,
) -> Result<String> {
    let active = store::get_active_user_id(&state.pool)
        .await?
        .ok_or_else(|| AppError::Other("no active profile".into()))?;
    match store::conversation_user_id(&state.pool, &conversation_id).await? {
        Some(owner) if owner == active => {}
        _ => return Err(AppError::Other("conversation not found for this profile".into())),
    }

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

    let user = store::get_user(&state.pool, &active)
        .await?
        .ok_or_else(|| AppError::Other("active profile not found".into()))?;
    let profile = if user.profile == "adult" { Profile::Adult } else { Profile::Kid };

    let on_device = on_device_state(&settings).await;
    let inputs = RouteInputs {
        firefly_endpoint: &settings.firefly_endpoint,
        on_device_endpoint: &settings.on_device_endpoint,
        on_device_model: &settings.on_device_model,
        model_code: &settings.model_code,
        model_chat_heavy: &settings.model_chat_heavy,
        model_frontier: &settings.model_frontier,
        firefly_reachable: reachable,
        on_device_available: matches!(on_device, OnDevice::Ready { .. }),
        profile,
    };
    let route = match router::resolve_route(task, &inputs) {
        Ok(r) => r,
        Err(RouteError::Refused(m)) => return Err(AppError::Other(m)),
        // A NotConfigured result usually means on-device was the only option and it
        // isn't ready; surface an actionable message instead of a raw network error.
        Err(RouteError::NotConfigured(m)) => {
            return Err(AppError::Other(on_device_hint(&on_device).unwrap_or(m)));
        }
    };

    // LiteLLM/home-base/cloud routes carry the user's per-user virtual key.
    let token = if route.use_token {
        Some(secrets::get_litellm_key(&active)?)
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
        if let Ok(device_token) = secrets::get_device_token(&active) {
            match sync::search_memories(&settings.sync_endpoint, &device_token, &content, &active, 8)
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
        .plugin(tauri_plugin_opener::init())
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
                    pending_auth: Mutex::new(None),
                });
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_conversations,
            get_messages,
            create_conversation,
            rename_conversation,
            delete_conversation,
            generate_conversation_title,
            list_profiles,
            switch_profile,
            refresh_active_profile,
            sign_out,
            signup,
            login,
            register_device,
            claim_device,
            get_settings,
            set_settings,
            send_message,
            check_firefly,
            check_on_device,
            pull_on_device_model,
            sync_now,
            platform,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
