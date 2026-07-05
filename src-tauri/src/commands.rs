use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::model::event::op_type;
use crate::model::{BlockType, ChatMetadata, Message, MessageType, RichTextBlock, RichTextSpan};
use crate::storage::{
    materialize, now_ms, save_pairings, save_peers, AppSettings, EventStore, MaterializedChat,
    OutboundPeer, Pairing,
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

// ---------- Desktop↔desktop: outbound peers (client role) ----------

/// The pairing payload another device produced (mirrors `PairingPayload` /
/// `PairingPayload.kt`). Only the fields we need to connect out.
#[derive(Debug, Deserialize)]
struct IncomingPairing {
    #[serde(rename = "deviceId")]
    device_id: String,
    #[serde(default)]
    name: String,
    host: String,
    port: u16,
    secret: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PeerView {
    pub id: String,
    pub label: String,
    pub host: String,
    pub port: u16,
    #[serde(rename = "createdAt")]
    pub created_at: i64,
    #[serde(rename = "lastSeenAt")]
    pub last_seen_at: Option<i64>,
}

/// Add another desktop as an outbound sync peer by pasting *its* pairing JSON
/// (produced by `create_pairing` on that device). The supervisor picks it up
/// immediately and starts a reconnecting connection.
#[tauri::command]
pub async fn add_peer(state: State<'_, AppState>, payload_json: String) -> Result<(), String> {
    let p: IncomingPairing = serde_json::from_str(payload_json.trim())
        .map_err(|e| format!("not a valid pairing JSON: {e}"))?;
    if p.device_id == state.identity_device_id {
        return Err("that's this device's own pairing — paste another desktop's".into());
    }
    if p.host.trim().is_empty() || p.secret.trim().is_empty() {
        return Err("pairing JSON is missing host or secret".into());
    }
    let mut guard = state.peers.write().await;
    if guard.peers.iter().any(|e| e.id == p.device_id) {
        return Err("already syncing with that device".into());
    }
    let label = if p.name.trim().is_empty() {
        p.device_id.clone()
    } else {
        p.name.clone()
    };
    guard.peers.push(OutboundPeer {
        id: p.device_id,
        label,
        host: p.host,
        port: p.port,
        secret_b64: p.secret,
        created_at: now_ms(),
        last_seen_at: None,
        last_sent_hlc: None,
        last_received_hlc: None,
    });
    save_peers(&state.paths.peers, &guard).map_err(|e| e.to_string())?;
    drop(guard);
    state.peer_notify.notify_one();
    Ok(())
}

#[tauri::command]
pub async fn list_peers(state: State<'_, AppState>) -> Result<Vec<PeerView>, String> {
    let guard = state.peers.read().await;
    Ok(guard
        .peers
        .iter()
        .map(|p| PeerView {
            id: p.id.clone(),
            label: p.label.clone(),
            host: p.host.clone(),
            port: p.port,
            created_at: p.created_at,
            last_seen_at: p.last_seen_at,
        })
        .collect())
}

#[tauri::command]
pub async fn delete_peer(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let mut guard = state.peers.write().await;
    let before = guard.peers.len();
    guard.peers.retain(|p| p.id != id);
    let changed = guard.peers.len() != before;
    if changed {
        save_peers(&state.paths.peers, &guard).map_err(|e| e.to_string())?;
    }
    drop(guard);
    state.peer_notify.notify_one();
    Ok(())
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
fn seal_envelope(
    sealed: &crate::sync::protection::Sealed,
    supersedes: Option<&str>,
) -> serde_json::Value {
    use base64::{engine::general_purpose::STANDARD as B64, Engine};
    let mut env = serde_json::json!({
        "v": crate::sync::protection::ENVELOPE_VERSION,
        "enc": "AES-256-GCM",
        "nonce": B64.encode(sealed.nonce),
        "ct": B64.encode(&sealed.ciphertext),
    });
    if let Some(s) = supersedes {
        env["supersedes"] = serde_json::Value::String(s.to_string());
    }
    env
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
        seal_envelope(&sealed, None)
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

/// Re-seal every message event in a chat under `new_key`, each as a fresh event
/// that `supersedes` the old one (so peers replace what they hold and our own
/// at-rest log drops the stale copy via the shared scrub). `old_key` is `Some`
/// when existing content is already encrypted (PIN change) — those events are
/// decrypted with it first; `None` when the chat is still plaintext (initial
/// protect). Validation is done up front: nothing is authored until every
/// existing event has been recovered to plaintext, so a wrong old PIN leaves
/// the chat untouched.
fn reseal_all_messages(
    state: &AppState,
    chat_id: &str,
    new_key: &[u8; 32],
    old_key: Option<&[u8; 32]>,
) -> Result<usize, String> {
    let evts = state.events.read_chat(chat_id).map_err(|e| e.to_string())?;
    // Phase 1 — recover plaintext for every message event (no mutation yet).
    let mut work: Vec<(String, String, Vec<u8>)> = Vec::new();
    for w in &evts {
        let is_msg =
            w.op_type == op_type::MESSAGE_CREATED || w.op_type == op_type::MESSAGE_EDITED;
        if !is_msg {
            continue;
        }
        let plaintext = if w.payload.get("enc").is_some() {
            let key = old_key
                .ok_or_else(|| "chat already has encrypted content".to_string())?;
            crate::sync::protection::open_envelope(&w.payload, key)
                .ok_or_else(|| "wrong current PIN".to_string())?
        } else {
            // Plaintext message event — its payload *is* the Message JSON.
            serde_json::to_vec(&w.payload).map_err(|e| e.to_string())?
        };
        work.push((w.event_id.clone(), w.op_type.clone(), plaintext));
    }
    // Phase 2 — author one fresh sealed event per message, superseding the old.
    for (old_id, op, pt) in &work {
        let sealed = crate::sync::protection::seal(new_key, pt).map_err(|e| e.to_string())?;
        let payload = seal_envelope(&sealed, Some(old_id));
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
    }
    // Phase 3 — drop the now-superseded plaintext / old-encrypted events at rest.
    if let Ok(evs) = state.events.read_chat(chat_id) {
        if let Some(scrubbed) = crate::sync::scrub_superseded(&evs) {
            state
                .events
                .replace_chat(chat_id, &scrubbed)
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(work.len())
}

/// Protect an unprotected chat and set its PIN, from the desktop. Generates a
/// salt, derives + caches the key, flips `isProtected`/`protectionSalt` (the
/// metadata stays plaintext — it always does), then re-seals existing messages
/// so neither the at-rest log nor peers keep readable copies. Mirrors the
/// phone's set-protected + first-unlock + `migrateProtectedChat`, in one step.
#[tauri::command]
pub async fn protect_chat(
    state: State<'_, AppState>,
    chat_id: String,
    pin: String,
) -> Result<(), String> {
    if pin.is_empty() {
        return Err("PIN cannot be empty".into());
    }
    let mut meta = current_metadata(&state.events, &chat_id)?;
    if meta.is_protected {
        return Err("chat is already protected".into());
    }
    let salt = crate::sync::protection::generate_salt();
    let key = crate::sync::protection::derive_key(&pin, &salt).map_err(|e| e.to_string())?;

    meta.is_protected = true;
    meta.protection_salt = Some(salt);
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

    state.keystore.unlock(&chat_id, key);
    reseal_all_messages(state.inner(), &chat_id, &key, None)?;
    Ok(())
}

/// Change a protected chat's PIN. Verifies the current PIN against an existing
/// sealed event, then re-seals everything under the new key (superseding the
/// old events) and re-caches the key. The salt is unchanged — only the derived
/// key changes — so no metadata event is needed.
#[tauri::command]
pub async fn change_chat_pin(
    state: State<'_, AppState>,
    chat_id: String,
    old_pin: String,
    new_pin: String,
) -> Result<(), String> {
    if new_pin.is_empty() {
        return Err("new PIN cannot be empty".into());
    }
    let meta = current_metadata(&state.events, &chat_id)?;
    if !meta.is_protected {
        return Err("chat is not protected".into());
    }
    let salt = meta
        .protection_salt
        .ok_or_else(|| "chat has no protection salt".to_string())?;
    let old_key =
        crate::sync::protection::derive_key(&old_pin, &salt).map_err(|e| e.to_string())?;

    // Verify the current PIN against any one sealed event before mutating.
    let evts = state.events.read_chat(&chat_id).map_err(|e| e.to_string())?;
    let sample = evts.iter().find(|e| {
        matches!(
            e.op_type.as_str(),
            op_type::MESSAGE_CREATED | op_type::MESSAGE_EDITED
        ) && e.payload.get("enc").is_some()
    });
    if let Some(ev) = sample {
        if crate::sync::protection::open_envelope(&ev.payload, &old_key).is_none() {
            return Err("wrong current PIN".into());
        }
    }

    let new_key =
        crate::sync::protection::derive_key(&new_pin, &salt).map_err(|e| e.to_string())?;
    reseal_all_messages(state.inner(), &chat_id, &new_key, Some(&old_key))?;
    state.keystore.unlock(&chat_id, new_key);
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

/// Tick / untick one item of a checklist message. Key-aware so it works in an
/// unlocked protected chat (reads the decrypted message, re-seals on author).
#[tauri::command]
pub async fn author_toggle_checklist_item(
    state: State<'_, AppState>,
    chat_id: String,
    message_id: String,
    item_id: String,
) -> Result<(), String> {
    let key = state.keystore.key_for(&chat_id);
    let mut msg = current_message(&state.events, &chat_id, &message_id, key.as_ref())?;
    if !matches!(msg.message_type, MessageType::Checklist) {
        return Err("not a checklist message".into());
    }
    let item = msg
        .checklist_items
        .iter_mut()
        .find(|i| i.id == item_id)
        .ok_or_else(|| "checklist item not found".to_string())?;
    item.checked = !item.checked;
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

/// Create a brand-new chat from the desktop — the primitive that makes the
/// desktop usable with no phone at all. Emits a `ChatMetadataChanged` for a fresh
/// chatId via the normal author pipeline (HLC tick → persist → broadcast → bus),
/// so it shows immediately and syncs to any connected peer. A new chat has no
/// prior events, so it carries an empty profile → the "Default" bucket; a phone
/// that later receives it files it under its active profile.
#[tauri::command]
pub async fn author_create_chat(
    state: State<'_, AppState>,
    name: String,
) -> Result<String, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("name cannot be empty".into());
    }
    let chat_id = uuid::Uuid::new_v4().to_string();
    let meta = ChatMetadata {
        id: chat_id.clone(),
        name: trimmed.to_string(),
        avatar_file: None,
        background_file: None,
        created_at: now_ms(),
        is_pinned: false,
        bubble_color: None,
        bubble_gradient: false,
        is_archived: false,
        tags: vec![],
        is_protected: false,
        hide_name: false,
        protection_salt: None,
    };
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
    Ok(chat_id)
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

// ---------- Standalone backup / restore ----------

#[derive(Debug, Clone, Serialize)]
pub struct ImportSummary {
    pub chats: usize,
    pub events: usize,
    pub blobs: usize,
}

/// Write a portable backup zip: every chat's `events/<id>.ndjson` (the source of
/// truth) plus the content-addressed `blobs/`. Device-local config (identity,
/// pairings, peers, settings) is intentionally excluded — restoring it onto
/// another machine would clone the deviceId and break sync.
#[tauri::command]
pub async fn export_backup(state: State<'_, AppState>, dest_path: String) -> Result<(), String> {
    use std::io::Write;
    let file =
        std::fs::File::create(&dest_path).map_err(|e| format!("create {dest_path}: {e}"))?;
    let mut zip = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    let mut copy_dir = |dir: &std::path::Path, prefix: &str, ndjson_only: bool| -> Result<(), String> {
        let rd = match std::fs::read_dir(dir) {
            Ok(rd) => rd,
            Err(_) => return Ok(()), // dir may not exist yet — nothing to back up
        };
        for entry in rd.flatten() {
            let p = entry.path();
            if !p.is_file() {
                continue;
            }
            let name = match p.file_name().and_then(|s| s.to_str()) {
                Some(n) => n.to_string(),
                None => continue,
            };
            if name.contains(".tmp.") {
                continue; // skip in-progress temp writes
            }
            if ndjson_only && !name.ends_with(".ndjson") {
                continue;
            }
            let bytes = std::fs::read(&p).map_err(|e| e.to_string())?;
            zip.start_file(format!("{prefix}/{name}"), opts)
                .map_err(|e| e.to_string())?;
            zip.write_all(&bytes).map_err(|e| e.to_string())?;
        }
        Ok(())
    };

    copy_dir(&state.paths.events, "events", true)?;
    copy_dir(state.blobs.root(), "blobs", false)?;
    zip.finish().map_err(|e| e.to_string())?;
    Ok(())
}

/// Restore a backup zip by *replaying* its events into the local log — a union,
/// not an overwrite. Dedup by `eventId` makes it idempotent; HLC ordering makes
/// it converge. Blobs are copied if missing (content-addressed, so safe).
#[tauri::command]
pub async fn import_backup(
    state: State<'_, AppState>,
    src_path: String,
) -> Result<ImportSummary, String> {
    use std::io::Read;
    let file = std::fs::File::open(&src_path).map_err(|e| format!("open {src_path}: {e}"))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| format!("not a valid backup zip: {e}"))?;

    let mut chat_files: std::collections::HashSet<String> = Default::default();
    let mut touched: std::collections::HashSet<String> = Default::default();
    let mut events_added = 0usize;
    let mut blobs_added = 0usize;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().to_string();
        if let Some(fname) = name.strip_prefix("events/") {
            if !fname.ends_with(".ndjson") || fname.contains('/') {
                continue;
            }
            chat_files.insert(fname.to_string());
            let mut buf = String::new();
            entry.read_to_string(&mut buf).map_err(|e| e.to_string())?;
            for line in buf.lines() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                if let Ok(wire) =
                    serde_json::from_str::<crate::model::event::ChatEventWire>(line)
                {
                    state.clock.receive(&wire.hlc, now_ms());
                    if state.events.append(&wire).unwrap_or(false) {
                        events_added += 1;
                        touched.insert(wire.chat_id.clone());
                    }
                }
            }
        } else if let Some(fname) = name.strip_prefix("blobs/") {
            // Content-addressed flat names only — reject anything path-ish.
            if fname.is_empty() || fname.contains('/') || fname.contains('\\') || fname.contains("..")
            {
                continue;
            }
            let dest = state.blobs.root().join(fname);
            if dest.exists() {
                continue;
            }
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
            if std::fs::write(&dest, &bytes).is_ok() {
                blobs_added += 1;
            }
        }
    }

    // Protected-chat catch-up parity, and nudge the UI to re-materialize.
    for cid in &touched {
        if let Ok(evs) = state.events.read_chat(cid) {
            if let Some(scrubbed) = crate::sync::scrub_superseded(&evs) {
                let _ = state.events.replace_chat(cid, &scrubbed);
            }
        }
        let _ = state.bus.send(SyncEvent::EventReceived {
            chat_id: cid.clone(),
            event_id: String::new(),
        });
    }

    Ok(ImportSummary {
        chats: chat_files.len(),
        events: events_added,
        blobs: blobs_added,
    })
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
