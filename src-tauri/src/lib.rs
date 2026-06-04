mod error;
mod llm;
mod router;
mod secrets;
mod store;
mod sync;

use error::Result;
use llm::{ChatMsg, StreamEvent};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use store::{Conversation, Message};
use tauri::ipc::Channel;
use tauri::{Manager, State};

const DEFAULT_ENDPOINT: &str = "http://firefly.taild9c345.ts.net:4000";
const DEFAULT_MODEL: &str = "fast";

struct AppState {
    pool: SqlitePool,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Settings {
    firefly_endpoint: String,
    logical_model: String,
}

async fn load_settings(pool: &SqlitePool) -> Result<Settings> {
    let firefly_endpoint = store::get_setting(pool, "firefly_endpoint")
        .await?
        .unwrap_or_else(|| DEFAULT_ENDPOINT.to_string());
    let logical_model = store::get_setting(pool, "logical_model")
        .await?
        .unwrap_or_else(|| DEFAULT_MODEL.to_string());
    Ok(Settings {
        firefly_endpoint,
        logical_model,
    })
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
async fn get_settings(state: State<'_, AppState>) -> Result<Settings> {
    load_settings(&state.pool).await
}

#[tauri::command]
async fn set_settings(state: State<'_, AppState>, settings: Settings) -> Result<()> {
    store::set_setting(&state.pool, "firefly_endpoint", &settings.firefly_endpoint).await?;
    store::set_setting(&state.pool, "logical_model", &settings.logical_model).await?;
    Ok(())
}

/// Persist the user message, stream the assistant reply token-by-token over
/// `on_token`, persist the final assistant text, and return its message id.
/// The device token never leaves the Rust core.
#[tauri::command]
async fn send_message(
    state: State<'_, AppState>,
    conversation_id: String,
    content: String,
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
    let token = secrets::get_token()?;
    let endpoint = router::resolve_endpoint(&settings.firefly_endpoint);

    on_token.send(StreamEvent::Started).ok();

    let assistant = store::insert_message(&state.pool, &conversation_id, "assistant", "").await?;
    let full = llm::stream_chat(
        &endpoint,
        &token,
        &settings.logical_model,
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
                handle.manage(AppState { pool });
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_conversations,
            get_messages,
            create_conversation,
            set_token,
            has_token,
            get_settings,
            set_settings,
            send_message,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
