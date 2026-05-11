use std::collections::{BTreeSet, HashMap};

use serde::Serialize;

use crate::model::event::{op_type, ChatEventWire};
use crate::model::{ChatMetadata, Message};

/// Result of materializing all events for one chat. `None` means the chat has been
/// tombstoned or has no metadata yet.
#[derive(Debug, Clone, Serialize)]
pub struct MaterializedChat {
    pub metadata: ChatMetadata,
    pub messages: Vec<Message>,
    #[serde(rename = "lastMessageAt")]
    pub last_message_at: Option<i64>,
    #[serde(rename = "lastMessagePreview")]
    pub last_message_preview: Option<String>,
}

/// Pure function `events → ChatData?` matching protocol §10.4.
pub fn materialize(mut events: Vec<ChatEventWire>) -> Option<MaterializedChat> {
    events.sort_by(|a, b| a.hlc.cmp(&b.hlc));

    let mut metadata: Option<ChatMetadata> = None;
    let mut messages: Vec<(String, Message)> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut tombstoned: BTreeSet<String> = BTreeSet::new();
    let mut chat_tombstoned = false;

    for ev in events {
        match ev.op_type.as_str() {
            op_type::CHAT_TOMBSTONED => {
                chat_tombstoned = true;
            }
            op_type::CHAT_METADATA_CHANGED => {
                if let Ok(m) = serde_json::from_value::<ChatMetadata>(ev.payload) {
                    metadata = Some(m);
                }
            }
            op_type::MESSAGE_TOMBSTONED => {
                if let Some(id) = ev.payload.get("messageId").and_then(|v| v.as_str()) {
                    tombstoned.insert(id.to_string());
                    if let Some(idx) = index.remove(id) {
                        messages.remove(idx);
                        for (_, i) in index.iter_mut() {
                            if *i > idx {
                                *i -= 1;
                            }
                        }
                    }
                }
            }
            op_type::MESSAGE_CREATED | op_type::MESSAGE_EDITED => {
                if let Ok(m) = serde_json::from_value::<Message>(ev.payload) {
                    if tombstoned.contains(&m.id) {
                        continue;
                    }
                    if let Some(&idx) = index.get(&m.id) {
                        messages[idx] = (m.id.clone(), m);
                    } else {
                        index.insert(m.id.clone(), messages.len());
                        messages.push((m.id.clone(), m));
                    }
                }
            }
            _ => { /* unknown op — ignore for forward compat */ }
        }
    }

    if chat_tombstoned {
        return None;
    }
    let metadata = metadata?;

    let mut messages: Vec<Message> = messages.into_iter().map(|(_, m)| m).collect();
    messages.sort_by_key(|m| m.created_at);

    let last_message_at = messages.last().map(|m| m.created_at);
    let last_message_preview = messages.last().map(plain_text_preview);

    Some(MaterializedChat {
        metadata,
        messages,
        last_message_at,
        last_message_preview,
    })
}

fn plain_text_preview(m: &Message) -> String {
    use crate::model::MessageType;
    let s = match m.message_type {
        MessageType::Checklist => {
            let done = m.checklist_items.iter().filter(|i| i.checked).count();
            let total = m.checklist_items.len();
            let first = m
                .checklist_items
                .first()
                .map(|i| i.text.as_str())
                .unwrap_or("");
            format!("{done}/{total} {first}")
        }
        MessageType::Voice => "🎙️ Voice message".to_string(),
        MessageType::Image => "🖼️ Image".to_string(),
        MessageType::Text => m
            .content
            .iter()
            .flat_map(|b| b.spans.iter())
            .map(|s| s.text.as_str())
            .collect::<Vec<_>>()
            .join(""),
    };
    let mut out = String::new();
    for c in s.chars().take(100) {
        out.push(c);
    }
    out
}
