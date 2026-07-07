use std::collections::{BTreeSet, HashMap};

use serde::Serialize;
use serde_json::Value;

use crate::model::event::{op_type, ChatEventWire};
use crate::model::{BlockType, ChatMetadata, Message, MessageType, RichTextBlock, RichTextSpan};
use crate::sync::protection;

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
    /// Phase 3: true when a decryption key was supplied for this materialization.
    /// The frontend uses this to decide whether to show the PIN overlay.
    #[serde(rename = "isUnlocked")]
    pub is_unlocked: bool,
    /// Profile sync: which phone profile this chat belongs to (+ display name),
    /// from the chat's highest-HLC stamped event. Empty for pre-profile-sync chats.
    #[serde(rename = "profileId")]
    pub profile_id: String,
    #[serde(rename = "profileName")]
    pub profile_name: String,
}

/// Pure function `events → ChatData?` matching protocol §10.4. Encrypted payloads
/// materialize as `🔒` placeholders (no key available).
pub fn materialize(events: Vec<ChatEventWire>) -> Option<MaterializedChat> {
    materialize_with_key(events, None)
}

/// Phase 3 variant: when `key` is `Some`, encrypted `MessageCreated`/`MessageEdited`
/// payloads are decrypted in-place so the chat shows real content. When `None` (or
/// when decryption fails — wrong key), those events fall back to a locked placeholder.
pub fn materialize_with_key(
    mut events: Vec<ChatEventWire>,
    key: Option<&[u8; 32]>,
) -> Option<MaterializedChat> {
    events.sort_by(|a, b| a.hlc.cmp(&b.hlc));

    // Security invariant: in a protected chat the desktop must never render a message
    // body it didn't decrypt itself. Legacy chats marked protected on the phone before
    // phase-3 salts existed still carry *plaintext* message events that synced over —
    // showing those would defeat the lock. So pre-compute whether the final metadata
    // says protected; if so, only successfully-decrypted (formerly-encrypted) messages
    // become real content, everything else is a placeholder.
    let is_protected = events
        .iter()
        .filter(|e| e.op_type == op_type::CHAT_METADATA_CHANGED)
        .next_back()
        .and_then(|e| e.payload.get("isProtected").and_then(|v| v.as_bool()))
        .unwrap_or(false);

    let mut metadata: Option<ChatMetadata> = None;
    let mut messages: Vec<(String, Message)> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut tombstoned: BTreeSet<String> = BTreeSet::new();
    let mut chat_tombstoned = false;
    // Events are HLC-ascending, so the last non-empty stamp seen is the latest.
    let mut profile_id = String::new();
    let mut profile_name = String::new();

    for ev in events {
        if !ev.profile_id.is_empty() {
            profile_id = ev.profile_id.clone();
            profile_name = ev.profile_name.clone();
        }
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
                let encrypted = ev.payload.get("enc").is_some();
                let parsed: Option<Message> = if encrypted {
                    decrypt_payload(&ev.payload, key)
                        .and_then(|pt| serde_json::from_slice::<Message>(&pt).ok())
                } else if is_protected {
                    // Plaintext message in a protected chat (legacy / pre-salt). Never
                    // render its body on this device — force a placeholder regardless
                    // of unlock state. Real content only comes from decryption.
                    None
                } else {
                    serde_json::from_value::<Message>(ev.payload.clone()).ok()
                };
                let m = match parsed {
                    Some(m) => m,
                    // Encrypted-but-no-key, or plaintext-in-protected → placeholder so
                    // the bubble still shows in the timeline. Plaintext-but-malformed
                    // in an unprotected chat → skip (legacy behavior).
                    None if encrypted || is_protected => locked_placeholder(&ev),
                    None => continue,
                };
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
        is_unlocked: key.is_some(),
        profile_id,
        profile_name,
    })
}

/// Decrypt an `{v,enc,nonce,ct}` payload. `None` when no key is cached or the key
/// is wrong (tag mismatch).
fn decrypt_payload(payload: &Value, key: Option<&[u8; 32]>) -> Option<Vec<u8>> {
    protection::open_envelope(payload, key?)
}

/// Stand-in shown for an encrypted message we can't (yet) decrypt. Keyed by eventId
/// so it dedups idempotently and sorts into the timeline at the event's HLC time —
/// matches `MaterializedView.lockedPlaceholder` on the phone.
fn locked_placeholder(ev: &ChatEventWire) -> Message {
    Message {
        id: format!("locked_{}", ev.event_id),
        chat_id: ev.chat_id.clone(),
        content: vec![RichTextBlock {
            block_type: BlockType::Paragraph,
            spans: vec![RichTextSpan {
                text: "Protected message".to_string(),
                bold: false,
                italic: false,
                underline: false,
                strikethrough: false,
                color: None,
                link: None,
                inline_image: None,
            }],
        }],
        voice_file: None,
        voice_duration_ms: None,
        created_at: ev.hlc.physical_ms,
        edited_at: None,
        message_type: MessageType::Text,
        is_starred: false,
        image_files: vec![],
        tags: vec![],
        reminder_at: None,
        checklist_items: vec![],
        reactions: vec![],
        transcript: None,
    }
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
        // Fall back to the whisper transcript when we have one; keeps the
        // chat-list preview meaningful instead of a bare "Voice message" once
        // transcription has run.
        MessageType::Voice => m
            .transcript
            .as_deref()
            .filter(|t| !t.trim().is_empty())
            .unwrap_or("Voice message")
            .to_string(),
        MessageType::Image => "Image".to_string(),
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
