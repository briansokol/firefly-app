//! Pure helper for memory injection (Phase 4): build the system message that
//! prepends distilled memories to a home-base request. The network fetch lives
//! in sync.rs; this module stays pure and unit-tested.

use crate::llm::ChatMsg;
use crate::store::MemRow;

/// Build a system message from distilled memories, or `None` if there is
/// nothing to inject. Memories arrive best-first (similarity order); preserve it.
pub fn build_memory_message(memories: &[MemRow]) -> Option<ChatMsg> {
    let lines: Vec<String> = memories
        .iter()
        .map(|m| m.text.trim())
        .filter(|t| !t.is_empty())
        .map(|t| format!("- {t}"))
        .collect();
    if lines.is_empty() {
        return None;
    }
    let content = format!(
        "Relevant context about the user, retrieved from saved memories. \
         Use it only if helpful and ignore anything irrelevant to the question.\n{}",
        lines.join("\n")
    );
    Some(ChatMsg {
        role: "system".to_string(),
        content,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mem(text: &str) -> MemRow {
        MemRow {
            id: "id".into(),
            user_id: "u".into(),
            text: text.into(),
            source_conversation: None,
            updated_at: "2026-06-08T00:00:00.000Z".into(),
        }
    }

    #[test]
    fn none_when_empty() {
        assert!(build_memory_message(&[]).is_none());
    }

    #[test]
    fn none_when_all_blank() {
        assert!(build_memory_message(&[mem("   "), mem("")]).is_none());
    }

    #[test]
    fn builds_system_message_in_order() {
        let msg = build_memory_message(&[mem("likes tea"), mem("lives in Berlin")]).unwrap();
        assert_eq!(msg.role, "system");
        assert!(msg.content.contains("- likes tea"));
        assert!(msg.content.contains("- lives in Berlin"));
        // order preserved (best-first)
        assert!(msg.content.find("likes tea").unwrap() < msg.content.find("Berlin").unwrap());
    }
}
