use crate::error::{AppError, Result};
use eventsource_stream::Eventsource;
use futures_util::StreamExt;
use serde::Serialize;
use serde_json::Value;
use tauri::ipc::Channel;

#[derive(Clone, Serialize)]
pub struct ChatMsg {
    pub role: String,
    pub content: String,
}

#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum StreamEvent {
    Started,
    // Reasoning ("thinking") tokens from reasoning models; shown but not persisted.
    Reasoning { text: String },
    Token { text: String },
    Done,
    Error { message: String },
}

/// Stream an OpenAI-compatible chat completion, emitting each token on `channel`.
/// Returns the fully accumulated assistant text on success.
pub async fn stream_chat(
    endpoint: &str,
    token: &str,
    model: &str,
    messages: Vec<ChatMsg>,
    channel: &Channel<StreamEvent>,
) -> Result<String> {
    let url = format!("{}/v1/chat/completions", endpoint.trim_end_matches('/'));
    let body = serde_json::json!({
        "model": model,
        "messages": messages,
        "stream": true,
    });

    let resp = reqwest::Client::new()
        .post(&url)
        .bearer_auth(token)
        .header("Accept", "text/event-stream")
        .json(&body)
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let detail = resp.text().await.unwrap_or_default();
        let message = format!("HTTP {status}: {detail}");
        let _ = channel.send(StreamEvent::Error {
            message: message.clone(),
        });
        return Err(AppError::Other(message));
    }

    let mut stream = resp.bytes_stream().eventsource();
    let mut accumulated = String::new();

    while let Some(event) = stream.next().await {
        let event = match event {
            Ok(e) => e,
            Err(e) => {
                let message = e.to_string();
                let _ = channel.send(StreamEvent::Error {
                    message: message.clone(),
                });
                return Err(AppError::Other(message));
            }
        };

        if event.data == "[DONE]" {
            break;
        }

        let json: Value = match serde_json::from_str(&event.data) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let delta = &json["choices"][0]["delta"];

        if let Some(text) = delta["reasoning_content"].as_str() {
            if !text.is_empty() {
                let _ = channel.send(StreamEvent::Reasoning {
                    text: text.to_string(),
                });
            }
        }

        if let Some(text) = delta["content"].as_str() {
            if !text.is_empty() {
                accumulated.push_str(text);
                let _ = channel.send(StreamEvent::Token {
                    text: text.to_string(),
                });
            }
        }
    }

    let _ = channel.send(StreamEvent::Done);
    Ok(accumulated)
}
