use crate::error::{AppError, Result};
use eventsource_stream::Eventsource;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::ipc::Channel;

#[derive(Deserialize)]
struct ModelsResponse {
    #[serde(default)]
    data: Vec<ModelEntry>,
}

#[derive(Deserialize)]
struct ModelEntry {
    id: String,
}

/// Pure: extract logical model ids from a `GET /v1/models` body. Returns empty on
/// any parse failure (caller treats that as "could not determine").
pub fn parse_model_ids(json: &str) -> Vec<String> {
    serde_json::from_str::<ModelsResponse>(json)
        .map(|r| r.data.into_iter().map(|m| m.id).collect())
        .unwrap_or_default()
}

/// List the logical model names a LiteLLM key is allowed to resolve. Used to
/// derive whether the active profile is `kid` or `adult`. See API-CONTRACT.md §2.
pub async fn list_models(endpoint: &str, key: &str) -> Result<Vec<String>> {
    let url = format!("{}/v1/models", endpoint.trim_end_matches('/'));
    let resp = reqwest::Client::new().get(&url).bearer_auth(key).send().await?;
    if !resp.status().is_success() {
        return Err(AppError::Other(format!("models HTTP {}", resp.status())));
    }
    let text = resp.text().await?;
    Ok(parse_model_ids(&text))
}

#[derive(Clone, Serialize)]
pub struct ChatMsg {
    pub role: String,
    pub content: String,
}

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

/// Stream an OpenAI-compatible chat completion, emitting each token on `channel`.
/// Returns the fully accumulated assistant text on success.
pub async fn stream_chat(
    endpoint: &str,
    token: Option<&str>,
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

    let mut req = reqwest::Client::new()
        .post(&url)
        .header("Accept", "text/event-stream")
        .json(&body);
    if let Some(t) = token {
        req = req.bearer_auth(t);
    }
    let resp = req.send().await?;

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

/// Non-streaming OpenAI-compatible chat completion. Returns the assistant text.
/// Used for short, side-channel generations (e.g. conversation titles) where we
/// do not want to stream tokens to the webview.
pub async fn complete_chat(
    endpoint: &str,
    token: Option<&str>,
    model: &str,
    messages: Vec<ChatMsg>,
) -> Result<String> {
    let url = format!("{}/v1/chat/completions", endpoint.trim_end_matches('/'));
    let body = serde_json::json!({
        "model": model,
        "messages": messages,
        "stream": false,
    });

    let mut req = reqwest::Client::new().post(&url).json(&body);
    if let Some(t) = token {
        req = req.bearer_auth(t);
    }
    let resp = req.send().await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let detail = resp.text().await.unwrap_or_default();
        return Err(AppError::Other(format!("HTTP {status}: {detail}")));
    }

    let json: Value = resp.json().await?;
    let content = json["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    Ok(content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_model_ids_reads_data_ids() {
        let json = r#"{"object":"list","data":[{"id":"fast"},{"id":"chat-heavy"},{"id":"code"}]}"#;
        assert_eq!(parse_model_ids(json), vec!["fast", "chat-heavy", "code"]);
    }

    #[test]
    fn parse_model_ids_is_empty_on_garbage() {
        assert!(parse_model_ids("not json").is_empty());
    }
}
