use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::model::event::op_type;
use crate::model::{BlockType, ChatMetadata, Message, MessageType, RichTextBlock, RichTextSpan};
use crate::storage::{
    materialize, now_ms, save_pairings, AppSettings, EventStore, MaterializedChat, Pairing,
};
use crate::sync::{author_event, SyncEvent};
use crate::AppState;

#[derive(Debug, Clone, Serialize)]
pub struct ChatSummary {
    pub id: String,
    pub name: String,
    #[serde(rename = "avatarFile")]
    pub avatar_file: Option<String>,
    #[serde(rename = "createdAt")]
    pub created_at: i64,
    #[serde(rename = "isPinned")]
    pub is_pinned: bool,
    #[serde(rename = "isArchived")]
    pub is_archived: bool,
    #[serde(rename = "isProtected")]
    pub is_protected: bool,
    pub tags: Vec<String>,
    #[serde(rename = "lastMessageAt")]
    pub last_message_at: Option<i64>,
    #[serde(rename = "lastMessagePreview")]
    pub last_message_preview: Option<String>,
    #[serde(rename = "messageCount")]
    pub message_count: usize,
    #[serde(rename = "bubbleColor")]
    pub bubble_color: Option<String>,
    #[serde(rename = "bubbleGradient")]
    pub bubble_gradient: bool,
    #[serde(rename = "hideName")]
    pub hide_name: bool,
    #[serde(rename = "profileId")]
    pub profile_id: String,
    #[serde(rename = "profileName")]
    pub profile_name: String,
}

impl ChatSummary {
    fn from_materialized(m: &MaterializedChat) -> Self {
        Self {
            id: m.metadata.id.clone(),
            name: m.metadata.name.clone(),
            avatar_file: m.metadata.avatar_file.clone(),
            created_at: m.metadata.created_at,
            is_pinned: m.metadata.is_pinned,
            is_archived: m.metadata.is_archived,
            is_protected: m.metadata.is_protected,
            tags: m.metadata.tags.clone(),
            last_message_at: m.last_message_at,
            last_message_preview: m.last_message_preview.clone(),
            message_count: m.messages.len(),
            bubble_color: m.metadata.bubble_color.clone(),
            bubble_gradient: m.metadata.bubble_gradient,
            hide_name: m.metadata.hide_name,
            profile_id: m.profile_id.clone(),
            profile_name: m.profile_name.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PairingPayload {
    pub v: u32,
    #[serde(rename = "deviceId")]
    pub device_id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub secret: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PairingDescriptor {
    pub pairing: Pairing,
    #[serde(rename = "payloadJson")]
    pub payload_json: String,
    #[serde(rename = "qrSvg")]
    pub qr_svg: String,
}

#[tauri::command]
pub async fn list_chats(state: State<'_, AppState>) -> Result<Vec<ChatSummary>, String> {
    list_chats_impl(state.events.clone()).map_err(|e| e.to_string())
}

fn list_chats_impl(events: Arc<EventStore>) -> anyhow::Result<Vec<ChatSummary>> {
    let mut out = Vec::new();
    for chat_id in events.list_chat_ids()? {
        let evts = events.read_chat(&chat_id)?;
        if let Some(m) = materialize(evts) {
            out.push(ChatSummary::from_materialized(&m));
        }
    }
    out.sort_by(|a, b| {
        b.is_pinned
            .cmp(&a.is_pinned)
            .then(b.last_message_at.unwrap_or(0).cmp(&a.last_message_at.unwrap_or(0)))
            .then(b.created_at.cmp(&a.created_at))
    });
    Ok(out)
}

#[tauri::command]
pub async fn get_chat(
    state: State<'_, AppState>,
    chat_id: String,
) -> Result<Option<MaterializedChat>, String> {
    let events = state.events.clone();
    let evts = events.read_chat(&chat_id).map_err(|e| e.to_string())?;
    // Phase 3: decrypt protected content if this chat is currently unlocked.
    let key = state.keystore.key_for(&chat_id);
    Ok(crate::storage::materialize::materialize_with_key(
        evts,
        key.as_ref(),
    ))
}

/// Phase 3: derive the per-chat key from `pin` + the chat's `protectionSalt` and
/// cache it. Verifies correctness by trying to decrypt the first encrypted event in
/// the chat — returns `false` on a wrong PIN. If the chat has no encrypted events
/// yet (just marked protected, nothing authored remotely), the key is cached
/// optimistically and `true` is returned.
#[tauri::command]
pub async fn unlock_chat(
    state: State<'_, AppState>,
    chat_id: String,
    pin: String,
) -> Result<bool, String> {
    let evts = state
        .events
        .read_chat(&chat_id)
        .map_err(|e| e.to_string())?;

    // Find the chat's protectionSalt from the latest ChatMetadataChanged.
    let salt = evts
        .iter()
        .filter(|e| e.op_type == op_type::CHAT_METADATA_CHANGED)
        .max_by(|a, b| a.hlc.cmp(&b.hlc))
        .and_then(|e| {
            e.payload
                .get("protectionSalt")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .ok_or_else(|| "chat has no protection salt".to_string())?;

    let key = crate::sync::protection::derive_key(&pin, &salt).map_err(|e| e.to_string())?;

    // Verify against the first encrypted message event, if any exists.
    let sample = evts.iter().find(|e| {
        matches!(
            e.op_type.as_str(),
            op_type::MESSAGE_CREATED | op_type::MESSAGE_EDITED
        ) && e.payload.get("enc").is_some()
    });
    let verified = match sample {
        Some(ev) => crate::sync::protection::open_envelope(&ev.payload, &key).is_some(),
        None => true, // nothing to check against — trust optimistically
    };
    if !verified {
        return Ok(false);
    }
    state.keystore.unlock(&chat_id, key);
    Ok(true)
}

#[tauri::command]
pub async fn lock_chat(state: State<'_, AppState>, chat_id: String) -> Result<(), String> {
    state.keystore.lock(&chat_id);
    Ok(())
}

#[tauri::command]
pub async fn list_pairings(state: State<'_, AppState>) -> Result<Vec<PairingDescriptor>, String> {
    let pairings = state.pairings.read().await;
    let settings = state.settings.read().await.clone();
    let identity_device_id = state.identity_device_id.clone();
    let identity_name = settings.device_name.clone();
    let host = state.advertised_host.read().await.clone();
    let mut out = Vec::new();
    for p in pairings.pairings.iter() {
        let payload = PairingPayload {
            v: 1,
            device_id: identity_device_id.clone(),
            name: identity_name.clone(),
            host: host.clone(),
            port: settings.port,
            secret: p.secret_b64.clone(),
        };
        let payload_json = serde_json::to_string(&payload).unwrap_or_default();
        let qr_svg = make_qr_svg(&payload_json);
        out.push(PairingDescriptor {
            pairing: p.clone(),
            payload_json,
            qr_svg,
        });
    }
    Ok(out)
}

#[tauri::command]
pub async fn create_pairing(
    state: State<'_, AppState>,
    label: String,
) -> Result<PairingDescriptor, String> {
    let pairing = Pairing::new_with_label(label);
    {
        let mut guard = state.pairings.write().await;
        guard.pairings.push(pairing.clone());
        save_pairings(&state.paths.pairings, &guard).map_err(|e| e.to_string())?;
    }
    let settings = state.settings.read().await.clone();
    let host = state.advertised_host.read().await.clone();
    let payload = PairingPayload {
        v: 1,
        device_id: state.identity_device_id.clone(),
        name: settings.device_name.clone(),
        host,
        port: settings.port,
        secret: pairing.secret_b64.clone(),
    };
    let payload_json = serde_json::to_string(&payload).unwrap_or_default();
    let qr_svg = make_qr_svg(&payload_json);
    Ok(PairingDescriptor {
        pairing,
        payload_json,
        qr_svg,
    })
}

#[tauri::command]
pub async fn delete_pairing(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let mut guard = state.pairings.write().await;
    guard.pairings.retain(|p| p.id != id);
    save_pairings(&state.paths.pairings, &guard).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<AppSettings, String> {
    Ok(state.settings.read().await.clone())
}

#[tauri::command]
pub async fn update_settings(
    state: State<'_, AppState>,
    settings: AppSettings,
) -> Result<AppSettings, String> {
    crate::storage::settings::save(&state.paths.settings, &settings).map_err(|e| e.to_string())?;
    *state.settings.write().await = settings.clone();

    // Reconcile mDNS advertising with the new `discoverable` setting — effective
    // immediately, no restart. When on, re-announce (drop + recreate) so a changed
    // device name is picked up too. Advertise the port the server actually bound
    // (a port change still needs a restart) so we never advertise a dead port.
    let host = state.advertised_host.read().await.clone();
    let bound_port = state
        .server_addr
        .read()
        .await
        .as_deref()
        .and_then(|a| a.rsplit(':').next())
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(settings.port);
    {
        let mut guard = state.mdns.lock().unwrap();
        if settings.discoverable {
            match crate::sync::MdnsRegistration::announce(
                &settings.device_name,
                &host,
                bound_port,
                &state.identity_device_id,
            ) {
                Ok(reg) => *guard = Some(reg),
                Err(e) => {
                    tracing::warn!("mDNS re-announce failed: {e}");
                    *guard = None;
                }
            }
        } else {
            // Drop the registration → mdns-sd unregisters the service.
            *guard = None;
        }
    }

    Ok(settings)
}

#[tauri::command]
pub async fn list_lan_ips() -> Result<Vec<String>, String> {
    Ok(crate::network::detect_lan_ips())
}

#[tauri::command]
pub async fn set_advertised_host(state: State<'_, AppState>, host: String) -> Result<(), String> {
    *state.advertised_host.write().await = host;
    Ok(())
}

#[tauri::command]
pub async fn get_advertised_host(state: State<'_, AppState>) -> Result<String, String> {
    Ok(state.advertised_host.read().await.clone())
}

#[tauri::command]
pub async fn get_device_identity(state: State<'_, AppState>) -> Result<DeviceIdentityView, String> {
    let settings = state.settings.read().await.clone();
    Ok(DeviceIdentityView {
        device_id: state.identity_device_id.clone(),
        name: settings.device_name,
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct DeviceIdentityView {
    #[serde(rename = "deviceId")]
    pub device_id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ServerStatusView {
    pub status: String,
    pub addr: Option<String>,
}

/// Read the current WS server status directly (not via the one-shot bus event).
/// The frontend calls this on bootstrap so a listener attached after
/// `ServerStarted` already fired still shows the right state.
#[tauri::command]
pub async fn get_server_status(state: State<'_, AppState>) -> Result<ServerStatusView, String> {
    Ok(ServerStatusView {
        status: state.server_status.read().await.to_string(),
        addr: state.server_addr.read().await.clone(),
    })
}

// ---------- Phase 2a.2 authoring commands ----------

/// The message as it currently materializes. `key` must be the chat's unlocked key
/// for a protected chat — otherwise a protected message resolves to the 🔒
/// placeholder and an edit/star would clobber it. `None` for unprotected chats.
fn current_message(
    events: &EventStore,
    chat_id: &str,
    message_id: &str,
    key: Option<&[u8; 32]>,
) -> Result<Message, String> {
    let evts = events.read_chat(chat_id).map_err(|e| e.to_string())?;
    let mat = crate::storage::materialize::materialize_with_key(evts, key)
        .ok_or_else(|| "chat not found".to_string())?;
    mat.messages
        .into_iter()
        .find(|m| m.id == message_id)
        .ok_or_else(|| "message not found".to_string())
}

/// Serialize the `{v,enc,nonce,ct}` event payload for a sealed message — the exact
/// shape `ChatEventCodec.payloadOf` produces on the phone, so a desktop-authored
/// encrypted message round-trips and decodes there as an `EncryptedMessage`.
fn seal_envelope(sealed: &crate::sync::protection::Sealed) -> serde_json::Value {
    use base64::{engine::general_purpose::STANDARD as B64, Engine};
    serde_json::json!({
        "v": crate::sync::protection::ENVELOPE_VERSION,
        "enc": "AES-256-GCM",
        "nonce": B64.encode(sealed.nonce),
        "ct": B64.encode(&sealed.ciphertext),
    })
}

/// Author a `MessageCreated`/`MessageEdited` event. In a protected chat the message
/// body is AES-GCM-sealed before it leaves this process, so neither the local event
/// log nor the wire ever carries plaintext. Refuses if the chat is protected but
/// locked — the desktop must hold the PIN-derived key (chat unlocked) to author
/// into it, otherwise we'd either leak plaintext or write a body the phone hides.
fn author_message(
    state: &AppState,
    chat_id: &str,
    op: &str,
    msg: &Message,
) -> Result<(), String> {
    let meta = current_metadata(&state.events, chat_id)?;
    let payload = if meta.is_protected {
        let key = state
            .keystore
            .key_for(chat_id)
            .ok_or_else(|| "chat is locked — unlock it before sending".to_string())?;
        let plaintext = serde_json::to_vec(msg).map_err(|e| e.to_string())?;
        let sealed =
            crate::sync::protection::seal(&key, &plaintext).map_err(|e| e.to_string())?;
        seal_envelope(&sealed)
    } else {
        serde_json::to_value(msg).map_err(|e| e.to_string())?
    };
    author_event(
        &state.clock,
        &state.events,
        &state.outbound,
        &state.bus,
        &state.identity_device_id,
        chat_id,
        op,
        payload,
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn author_send_text(
    state: State<'_, AppState>,
    chat_id: String,
    text: String,
) -> Result<(), String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err("empty message".to_string());
    }
    let msg = Message {
        id: uuid::Uuid::new_v4().to_string(),
        chat_id: chat_id.clone(),
        content: vec![RichTextBlock {
            block_type: BlockType::Paragraph,
            spans: vec![RichTextSpan {
                text: trimmed.to_string(),
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
        created_at: now_ms(),
        edited_at: None,
        message_type: MessageType::Text,
        is_starred: false,
        image_files: vec![],
        tags: vec![],
        reminder_at: None,
        checklist_items: vec![],
    };
    author_message(state.inner(), &chat_id, op_type::MESSAGE_CREATED, &msg)
}

#[tauri::command]
pub async fn author_toggle_star(
    state: State<'_, AppState>,
    chat_id: String,
    message_id: String,
) -> Result<(), String> {
    let key = state.keystore.key_for(&chat_id);
    let mut msg = current_message(&state.events, &chat_id, &message_id, key.as_ref())?;
    msg.is_starred = !msg.is_starred;
    msg.edited_at = Some(now_ms());
    author_message(state.inner(), &chat_id, op_type::MESSAGE_EDITED, &msg)
}

#[tauri::command]
pub async fn author_edit_text(
    state: State<'_, AppState>,
    chat_id: String,
    message_id: String,
    text: String,
) -> Result<(), String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err("edit cannot be empty".to_string());
    }
    let key = state.keystore.key_for(&chat_id);
    let mut msg = current_message(&state.events, &chat_id, &message_id, key.as_ref())?;
    if !matches!(msg.message_type, MessageType::Text) {
        return Err("only TEXT messages support plaintext edit from desktop".to_string());
    }
    msg.content = vec![RichTextBlock {
        block_type: BlockType::Paragraph,
        spans: vec![RichTextSpan {
            text: trimmed.to_string(),
            bold: false,
            italic: false,
            underline: false,
            strikethrough: false,
            color: None,
            link: None,
            inline_image: None,
        }],
    }];
    msg.edited_at = Some(now_ms());
    author_message(state.inner(), &chat_id, op_type::MESSAGE_EDITED, &msg)
}

/// Author a checklist message. Items arrive as plain strings (one per checklist row);
/// the command wraps each in a `ChecklistItem` with a fresh UUID and `checked = false`,
/// then emits a `MessageCreated` event with type=CHECKLIST. Phone uses the same shape.
#[tauri::command]
pub async fn author_send_checklist(
    state: State<'_, AppState>,
    chat_id: String,
    items: Vec<String>,
) -> Result<(), String> {
    let cleaned: Vec<crate::model::ChecklistItem> = items
        .into_iter()
        .filter_map(|raw| {
            let t = raw.trim();
            if t.is_empty() {
                None
            } else {
                Some(crate::model::ChecklistItem {
                    id: uuid::Uuid::new_v4().to_string(),
                    text: t.to_string(),
                    checked: false,
                })
            }
        })
        .collect();
    if cleaned.is_empty() {
        return Err("checklist has no items".into());
    }
    let msg = Message {
        id: uuid::Uuid::new_v4().to_string(),
        chat_id: chat_id.clone(),
        content: vec![],
        voice_file: None,
        voice_duration_ms: None,
        created_at: now_ms(),
        edited_at: None,
        message_type: MessageType::Checklist,
        is_starred: false,
        image_files: vec![],
        tags: vec![],
        reminder_at: None,
        checklist_items: cleaned,
    };
    author_message(state.inner(), &chat_id, op_type::MESSAGE_CREATED, &msg)
}

/// Current chat metadata (the latest `ChatMetadataChanged`, via the materializer).
/// Errors if the chat is gone/tombstoned so callers don't resurrect it.
fn current_metadata(events: &EventStore, chat_id: &str) -> Result<ChatMetadata, String> {
    let evts = events.read_chat(chat_id).map_err(|e| e.to_string())?;
    materialize(evts)
        .map(|m| m.metadata)
        .ok_or_else(|| "chat not found".to_string())
}

/// Patch chat-level metadata from the desktop and sync it back. Only the provided
/// fields change; everything else (avatar, background, protection salt, isProtected,
/// createdAt, bubble style, hideName) is preserved — important because
/// `ChatMetadataChanged` is whole-record LWW, so dropping a field would wipe it on
/// the phone.
#[tauri::command]
pub async fn author_update_chat_meta(
    state: State<'_, AppState>,
    chat_id: String,
    name: Option<String>,
    is_pinned: Option<bool>,
    is_archived: Option<bool>,
    tags: Option<Vec<String>>,
) -> Result<(), String> {
    let mut meta = current_metadata(&state.events, &chat_id)?;
    if let Some(n) = name {
        let n = n.trim();
        if n.is_empty() {
            return Err("name cannot be empty".into());
        }
        meta.name = n.to_string();
    }
    if let Some(p) = is_pinned {
        meta.is_pinned = p;
    }
    if let Some(a) = is_archived {
        meta.is_archived = a;
    }
    if let Some(t) = tags {
        meta.tags = t
            .into_iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
    }
    let payload = serde_json::to_value(&meta).map_err(|e| e.to_string())?;
    author_event(
        &state.clock,
        &state.events,
        &state.outbound,
        &state.bus,
        &state.identity_device_id,
        &chat_id,
        op_type::CHAT_METADATA_CHANGED,
        payload,
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Delete a chat from the desktop. Emits a `ChatTombstoned` (delete-wins, syncs to
/// the phone) then collapses this chat's local log to just that tombstone — same
/// shape as the phone's `deleteChat`, so there's nothing left to read on disk but
/// the deletion still propagates to offline peers via catchup.
#[tauri::command]
pub async fn author_delete_chat(
    state: State<'_, AppState>,
    chat_id: String,
) -> Result<(), String> {
    // Refuse if already gone, so we don't append a tombstone to a dead chat.
    current_metadata(&state.events, &chat_id)?;
    let payload = serde_json::json!({ "chatId": chat_id });
    let wire = author_event(
        &state.clock,
        &state.events,
        &state.outbound,
        &state.bus,
        &state.identity_device_id,
        &chat_id,
        op_type::CHAT_TOMBSTONED,
        payload,
    )
    .map_err(|e| e.to_string())?;
    state
        .events
        .replace_chat(&chat_id, std::slice::from_ref(&wire))
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn author_delete_message(
    state: State<'_, AppState>,
    chat_id: String,
    message_id: String,
) -> Result<(), String> {
    let payload = serde_json::json!({ "messageId": message_id });
    author_event(
        &state.clock,
        &state.events,
        &state.outbound,
        &state.bus,
        &state.identity_device_id,
        &chat_id,
        op_type::MESSAGE_TOMBSTONED,
        payload,
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Author a fully-formatted message — the frontend builds the `RichTextBlock` array
/// (typically by running its own light Markdown parser on the compose box) and the
/// Rust side just wraps it in a `Message` and emits a `MessageCreated` event.
///
/// Coexists with [author_send_text] which remains for callers that only need
/// plaintext and don't want to build the block array themselves.
#[tauri::command]
pub async fn author_send_message(
    state: State<'_, AppState>,
    chat_id: String,
    content: Vec<RichTextBlock>,
) -> Result<(), String> {
    // Reject if the entire payload would render as empty text — same guard as
    // author_send_text but applied to the assembled rich content.
    let any_text = content
        .iter()
        .flat_map(|b| b.spans.iter())
        .any(|s| !s.text.trim().is_empty() || s.inline_image.is_some());
    if !any_text {
        return Err("empty message".to_string());
    }
    let msg = Message {
        id: uuid::Uuid::new_v4().to_string(),
        chat_id: chat_id.clone(),
        content,
        voice_file: None,
        voice_duration_ms: None,
        created_at: now_ms(),
        edited_at: None,
        message_type: MessageType::Text,
        is_starred: false,
        image_files: vec![],
        tags: vec![],
        reminder_at: None,
        checklist_items: vec![],
    };
    author_message(state.inner(), &chat_id, op_type::MESSAGE_CREATED, &msg)
}

/// Attach an image from a local file path. Reads the bytes, hashes them, persists
/// the blob into the desktop's BlobStore (so the phone can fetch it via the existing
/// `BlobRequest` flow), then emits a `MessageCreated` event with the same on-the-wire
/// shape the phone authors for image messages (type=IMAGE, content with a single
/// span carrying `inlineImage = "images/<hash>.jpg"`).
#[tauri::command]
pub async fn author_send_image(
    state: State<'_, AppState>,
    chat_id: String,
    source_path: String,
) -> Result<(), String> {
    let bytes = std::fs::read(&source_path)
        .map_err(|e| format!("read source: {e}"))?;
    if bytes.is_empty() {
        return Err("empty image file".into());
    }
    let hash = crate::sync::blob::sha256_hex(&bytes);
    state
        .blobs
        .write_verified(&hash, "image", &bytes)
        .map_err(|e| format!("persist blob: {e}"))?;

    let reference = format!("images/{}.jpg", hash);
    let msg = Message {
        id: uuid::Uuid::new_v4().to_string(),
        chat_id: chat_id.clone(),
        content: vec![RichTextBlock {
            block_type: BlockType::Paragraph,
            spans: vec![RichTextSpan {
                text: String::new(),
                bold: false,
                italic: false,
                underline: false,
                strikethrough: false,
                color: None,
                link: None,
                inline_image: Some(reference),
            }],
        }],
        voice_file: None,
        voice_duration_ms: None,
        created_at: now_ms(),
        edited_at: None,
        message_type: MessageType::Image,
        is_starred: false,
        image_files: vec![],
        tags: vec![],
        reminder_at: None,
        checklist_items: vec![],
    };
    author_message(state.inner(), &chat_id, op_type::MESSAGE_CREATED, &msg)
}

/// Phase 2b — return a content-addressed media blob to the frontend as base64.
/// Frontend uses it as the `src` of an `<audio>` or `<img>` via a `data:` URL.
/// Returns `None` if the blob hasn't been transferred yet (still pending or peer
/// reported missing) so the UI can fall back to a placeholder.
#[tauri::command]
pub async fn read_blob(
    state: State<'_, AppState>,
    hash: String,
    media_type: String,
) -> Result<Option<String>, String> {
    use base64::{engine::general_purpose::STANDARD as B64, Engine};
    if !state.blobs.exists(&hash, &media_type) {
        return Ok(None);
    }
    let bytes = state
        .blobs
        .read(&hash, &media_type)
        .map_err(|e| e.to_string())?;
    Ok(Some(B64.encode(bytes)))
}

/// Forward sync server events into a Tauri event channel so the frontend can react
/// to them (peer connected, new messages, etc.) without polling.
pub fn spawn_sync_event_forwarder(app: AppHandle, mut rx: tokio::sync::broadcast::Receiver<SyncEvent>) {
    tauri::async_runtime::spawn(async move {
        while let Ok(ev) = rx.recv().await {
            let _ = app.emit("sync-event", ev_to_json(&ev));
        }
    });
}

fn ev_to_json(ev: &SyncEvent) -> serde_json::Value {
    match ev {
        SyncEvent::PeerConnected {
            peer,
            pairing_id,
            phone_id,
        } => serde_json::json!({"kind":"peer-connected","peer":peer,"pairingId":pairing_id,"phoneId":phone_id}),
        SyncEvent::PeerAuthFailed { peer } => {
            serde_json::json!({"kind":"peer-auth-failed","peer":peer})
        }
        SyncEvent::EventReceived { chat_id, event_id } => {
            serde_json::json!({"kind":"event-received","chatId":chat_id,"eventId":event_id})
        }
        SyncEvent::EventAuthored { chat_id, event_id } => {
            serde_json::json!({"kind":"event-authored","chatId":chat_id,"eventId":event_id})
        }
        SyncEvent::BlobArrived { hash } => {
            serde_json::json!({"kind":"blob-arrived","hash":hash})
        }
        SyncEvent::PeerDisconnected { peer } => {
            serde_json::json!({"kind":"peer-disconnected","peer":peer})
        }
        SyncEvent::ServerStarted { addr } => {
            serde_json::json!({"kind":"server-started","addr":addr})
        }
        SyncEvent::ServerStopped => serde_json::json!({"kind":"server-stopped"}),
        SyncEvent::ServerError { message } => {
            serde_json::json!({"kind":"server-error","message":message})
        }
    }
}

fn make_qr_svg(text: &str) -> String {
    use qrcode::render::svg;
    use qrcode::QrCode;
    match QrCode::new(text.as_bytes()) {
        Ok(code) => code
            .render::<svg::Color>()
            .min_dimensions(256, 256)
            .quiet_zone(true)
            .build(),
        Err(_) => String::new(),
    }
}
