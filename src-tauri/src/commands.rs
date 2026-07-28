use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::model::event::op_type;
use crate::model::{
    BlockType, ChatMetadata, FileAttachment, Message, MessageType, RichTextBlock, RichTextSpan,
};
use crate::storage::{
    materialize, now_ms, reset_sync_watermarks, save_pairings, save_peers, AppSettings, EventStore,
    MaterializedChat, OutboundPeer, Pairing,
};
use crate::sync::pear::valid_public_key;
use crate::sync::{author_event, author_event_with_profile, SyncEvent};
use crate::AppState;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DesktopPrefs {
    #[serde(rename = "startMinimized")]
    pub start_minimized: bool,
    /// Closing the window hides to the tray instead of quitting. Off for fresh
    /// installs — the first close asks instead (see `ask_close_to_tray`).
    #[serde(rename = "closeToTray")]
    pub close_to_tray: bool,
    /// Offer "keep Ncy in the tray?" when closing while `close_to_tray` is off.
    /// Cleared by the prompt's "Don't ask again".
    #[serde(rename = "askCloseToTray")]
    pub ask_close_to_tray: bool,
    #[serde(rename = "launchOnStartup")]
    pub launch_on_startup: bool,
    #[serde(rename = "askBeforeDeleting")]
    pub ask_before_deleting: bool,
    #[serde(rename = "autoCheckUpdates")]
    pub auto_check_updates: bool,
    #[serde(rename = "autoBackupEnabled", default)]
    pub auto_backup_enabled: bool,
    #[serde(
        rename = "autoBackupIntervalHours",
        default = "default_backup_interval"
    )]
    pub auto_backup_interval_hours: u64,
    #[serde(rename = "autoBackupFolder", default)]
    pub auto_backup_folder: Option<String>,
    #[serde(rename = "autoBackupRetention", default = "default_backup_retention")]
    pub auto_backup_retention: Option<u32>,
    #[serde(rename = "lastAutoBackupAt", default)]
    pub last_auto_backup_at: Option<i64>,
    #[serde(rename = "lastAutoBackupError", default)]
    pub last_auto_backup_error: Option<String>,
    /// Run the LAN sync stack (WS listener, mDNS, outbound peers). Off for fresh
    /// installs — Ncy works standalone, and sync is opt-in.
    ///
    /// The field default is `true`, not `false`: it only fires when a *existing*
    /// `desktop_prefs.json` is read that predates this key, and those users are
    /// already syncing with a phone. A fresh install has no file at all, so it
    /// takes `Default` below and starts off.
    #[serde(
        rename = "syncServerEnabled",
        default = "sync_enabled_for_existing_install"
    )]
    pub sync_server_enabled: bool,
}

/// See `DesktopPrefs::sync_server_enabled` — migration default for pre-existing
/// config files, which must keep syncing after an upgrade.
fn sync_enabled_for_existing_install() -> bool {
    true
}
fn default_backup_interval() -> u64 {
    24
}
fn default_backup_retention() -> Option<u32> {
    Some(10)
}

impl Default for DesktopPrefs {
    fn default() -> Self {
        Self {
            start_minimized: false,
            close_to_tray: false,
            ask_close_to_tray: true,
            launch_on_startup: false,
            ask_before_deleting: true,
            auto_check_updates: true,
            auto_backup_enabled: false,
            auto_backup_interval_hours: default_backup_interval(),
            auto_backup_folder: None,
            auto_backup_retention: default_backup_retention(),
            last_auto_backup_at: None,
            last_auto_backup_error: None,
            sync_server_enabled: false,
        }
    }
}

pub fn desktop_prefs_path(settings_path: &std::path::Path) -> PathBuf {
    settings_path.with_file_name("desktop_prefs.json")
}

pub fn load_desktop_prefs(settings_path: &std::path::Path) -> DesktopPrefs {
    let path = desktop_prefs_path(settings_path);
    let mut prefs = std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str::<DesktopPrefs>(&s).ok())
        .unwrap_or_default();
    prefs.launch_on_startup = startup_enabled().unwrap_or(prefs.launch_on_startup);
    prefs
}

fn save_desktop_prefs_file(
    settings_path: &std::path::Path,
    prefs: &DesktopPrefs,
) -> Result<(), String> {
    let path = desktop_prefs_path(settings_path);
    let bytes = serde_json::to_vec_pretty(prefs).map_err(|e| e.to_string())?;
    std::fs::write(path, bytes).map_err(|e| e.to_string())
}

/// `CREATE_NO_WINDOW` — `reg.exe` is a console program, and Ncy is a GUI binary
/// with no console to inherit, so without this every call flashes a black cmd
/// box over the UI (on launch, and on any desktop-pref change).
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// A `reg.exe` invocation that stays invisible.
#[cfg(windows)]
fn reg_command() -> std::process::Command {
    use std::os::windows::process::CommandExt;
    let mut cmd = std::process::Command::new("reg");
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

#[cfg(windows)]
fn startup_enabled() -> Result<bool, String> {
    let output = reg_command()
        .args([
            "query",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
            "/v",
            "Ncy",
        ])
        .output()
        .map_err(|e| e.to_string())?;
    Ok(output.status.success())
}

#[cfg(not(windows))]
fn startup_enabled() -> Result<bool, String> {
    Ok(false)
}

#[cfg(windows)]
fn set_startup_enabled(enabled: bool) -> Result<(), String> {
    let key = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
    let output = if enabled {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let value = format!("\"{}\"", exe.display());
        reg_command()
            .args(["add", key, "/v", "Ncy", "/t", "REG_SZ", "/d", &value, "/f"])
            .output()
            .map_err(|e| e.to_string())?
    } else {
        reg_command()
            .args(["delete", key, "/v", "Ncy", "/f"])
            .output()
            .map_err(|e| e.to_string())?
    };
    if output.status.success() || !enabled {
        Ok(())
    } else {
        let msg = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(if msg.is_empty() {
            "could not update Windows startup setting".into()
        } else {
            msg
        })
    }
}

#[cfg(not(windows))]
fn set_startup_enabled(enabled: bool) -> Result<(), String> {
    if enabled {
        Err("launch on startup is only implemented on Windows right now".into())
    } else {
        Ok(())
    }
}

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
    #[serde(rename = "pearPublicKey", skip_serializing_if = "Option::is_none")]
    pub pear_public_key: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PairingView {
    pub id: String,
    pub label: String,
    #[serde(rename = "phoneId")]
    pub phone_id: Option<String>,
    #[serde(rename = "phoneName")]
    pub phone_name: Option<String>,
    #[serde(rename = "createdAt")]
    pub created_at: i64,
    #[serde(rename = "lastSeenAt")]
    pub last_seen_at: Option<i64>,
}

impl From<&Pairing> for PairingView {
    fn from(pairing: &Pairing) -> Self {
        Self {
            id: pairing.id.clone(),
            label: pairing.label.clone(),
            phone_id: pairing.phone_id.clone(),
            phone_name: pairing.phone_name.clone(),
            created_at: pairing.created_at,
            last_seen_at: pairing.last_seen_at,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PairingDescriptor {
    pub pairing: PairingView,
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
            .then(
                b.last_message_at
                    .unwrap_or(0)
                    .cmp(&a.last_message_at.unwrap_or(0)),
            )
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
    let has_unused_invitation = state
        .pairings
        .read()
        .await
        .pairings
        .iter()
        .any(|pairing| pairing.phone_id.is_none());
    let pear_public_key = if has_unused_invitation {
        let mode = state.settings.read().await.sync_transport_mode;
        let key = state.pear_public_key.read().await.clone();
        if mode.uses_pear() && key.is_none() {
            return Err("Pear is not ready to display this pairing invitation".to_string());
        }
        key
    } else {
        None
    };
    let pairings = state.pairings.read().await;
    let settings = state.settings.read().await.clone();
    let identity_device_id = state.identity_device_id.clone();
    let identity_name = settings.device_name.clone();
    let host = state.advertised_host.read().await.clone();
    let mut out = Vec::new();
    for pairing in pairings.pairings.iter() {
        let (payload_json, qr_svg) = if pairing.phone_id.is_none() {
            let payload = PairingPayload {
                v: 1,
                device_id: identity_device_id.clone(),
                name: identity_name.clone(),
                host: host.clone(),
                port: settings.port,
                secret: pairing.secret_b64.clone(),
                pear_public_key: pear_public_key.clone(),
            };
            let payload_json = serde_json::to_string(&payload).unwrap_or_default();
            let qr_svg = make_qr_svg(&payload_json);
            (payload_json, qr_svg)
        } else {
            (String::new(), String::new())
        };
        out.push(PairingDescriptor {
            pairing: PairingView::from(pairing),
            payload_json,
            qr_svg,
        });
    }
    Ok(out)
}
/// A Pear-capable pairing must never silently degrade to a LAN-only payload.
/// Retry startup here so a transient sidecar failure at application launch can
/// recover when the user actually opens or creates a pairing invitation.
async fn pairing_pear_public_key(state: &AppState) -> Result<Option<String>, String> {
    let mode = state.settings.read().await.sync_transport_mode;
    if !mode.uses_pear() {
        return Ok(None);
    }
    if state.sync_shutdown.lock().unwrap().is_none() {
        return Err("Pear pairing requires device sync to be enabled".to_string());
    }
    if let Some(public_key) = state.pear_public_key.read().await.clone() {
        return Ok(Some(public_key));
    }
    crate::apply_pear_mode(state, mode)
        .await
        .map_err(|error| format!("Pear is not ready: {error}"))?;
    state
        .pear_public_key
        .read()
        .await
        .clone()
        .map(Some)
        .ok_or_else(|| "Pear started without providing a public key".to_string())
}

#[tauri::command]
pub async fn create_pairing(
    state: State<'_, AppState>,
    label: String,
) -> Result<PairingDescriptor, String> {
    let pear_public_key = pairing_pear_public_key(&state).await?;
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
        pear_public_key,
    };
    let payload_json = serde_json::to_string(&payload).unwrap_or_default();
    let qr_svg = make_qr_svg(&payload_json);
    Ok(PairingDescriptor {
        pairing: PairingView::from(&pairing),
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
pub async fn replace_pairing(
    state: State<'_, AppState>,
    id: String,
) -> Result<PairingDescriptor, String> {
    let pear_public_key = pairing_pear_public_key(&state).await?;
    let pairing = {
        let mut guard = state.pairings.write().await;
        let pairing = guard
            .pairings
            .iter_mut()
            .find(|pairing| pairing.id == id)
            .ok_or_else(|| "pairing not found".to_string())?;
        if pairing.phone_id.is_some() {
            return Err("paired devices cannot have their credentials replaced; remove and pair the device again".to_string());
        }
        pairing.rotate_secret();
        let rotated = pairing.clone();
        save_pairings(&state.paths.pairings, &guard).map_err(|e| e.to_string())?;
        rotated
    };
    let settings = state.settings.read().await.clone();
    let host = state.advertised_host.read().await.clone();
    let payload = PairingPayload {
        v: 1,
        device_id: state.identity_device_id.clone(),
        name: settings.device_name,
        host,
        port: settings.port,
        secret: pairing.secret_b64.clone(),
        pear_public_key,
    };
    let payload_json = serde_json::to_string(&payload).unwrap_or_default();
    Ok(PairingDescriptor {
        pairing: PairingView::from(&pairing),
        qr_svg: make_qr_svg(&payload_json),
        payload_json,
    })
}

// ---------- Discovery-initiated pairing (2026-07-07) ----------

/// The frontend has confirmed a pending pair request. Generate a fresh
/// pairing (same shape `create_pairing` returns), persist it, and hand the
/// plaintext JSON to the server's pending-request handler — the server
/// AES-256-GCM-seals it with the ECDH-derived key before it goes on the wire.
/// The phone will then reconnect through the normal HMAC-authenticated
/// `Hello` flow to actually sync.
#[tauri::command]
pub async fn pair_accept(
    state: State<'_, AppState>,
    session_id: String,
    label: String,
) -> Result<(), String> {
    let pear_public_key = pairing_pear_public_key(&state).await?;
    let session = {
        let mut reg = state.pairing_registry.lock().await;
        reg.remove(&session_id)
    };
    let session = session.ok_or_else(|| "no pending pair request (expired?)".to_string())?;

    // Same pairing-generation flow `create_pairing` uses so anything you can
    // pair via QR you can pair via mDNS.
    let effective_label = if label.trim().is_empty() {
        "Phone".to_string()
    } else {
        label
    };
    let pairing = Pairing::new_with_label(effective_label);
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
        pear_public_key,
    };
    let pairing_json = serde_json::to_string(&payload).map_err(|e| e.to_string())?;

    session
        .decision_tx
        .send(crate::sync::PairDecision::Accept { pairing_json })
        .map_err(|_| "server no longer waiting on this session".to_string())?;
    Ok(())
}

/// Frontend rejected the pair request. Signal the waiting server handler.
#[tauri::command]
pub async fn pair_reject(state: State<'_, AppState>, session_id: String) -> Result<(), String> {
    let session = {
        let mut reg = state.pairing_registry.lock().await;
        reg.remove(&session_id)
    };
    if let Some(session) = session {
        let _ = session.decision_tx.send(crate::sync::PairDecision::Reject {
            reason: "rejected".to_string(),
        });
    }
    Ok(())
}

/// Recent pair-endpoint attempts (accepted, rejected, timed out, rate-limited,
/// discovery-off, bad-pubkey). Backs the "Recent requests" list in Settings →
/// Sync so the user can spot unwanted activity. Newest last.
#[tauri::command]
pub async fn list_pair_attempts(
    state: State<'_, AppState>,
) -> Result<Vec<crate::sync::PairAttempt>, String> {
    let q = state.pair_security.attempts.read().await;
    Ok(q.iter().cloned().collect())
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
    #[serde(default, rename = "pearPublicKey")]
    pear_public_key: Option<String>,
    #[serde(default)]
    availability: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PeerView {
    pub id: String,
    pub label: String,
    #[serde(rename = "authenticatedName")]
    pub authenticated_name: Option<String>,
    pub host: String,
    pub port: u16,
    #[serde(rename = "pearPublicKey")]
    pub pear_public_key: Option<String>,
    #[serde(rename = "foregroundOnly")]
    pub foreground_only: bool,
    pub enabled: bool,
    #[serde(rename = "createdAt")]
    pub created_at: i64,
    #[serde(rename = "lastSeenAt")]
    pub last_seen_at: Option<i64>,
    #[serde(rename = "lastDiscoveredAt")]
    pub last_discovered_at: Option<i64>,
    #[serde(rename = "lastAuthenticatedAt")]
    pub last_authenticated_at: Option<i64>,
    #[serde(rename = "lastAuthenticatedHost")]
    pub last_authenticated_host: Option<String>,
    #[serde(rename = "lastAuthenticatedPort")]
    pub last_authenticated_port: Option<u16>,
    #[serde(rename = "lastError")]
    pub last_error: Option<String>,
    #[serde(rename = "nextRetryAt")]
    pub next_retry_at: Option<i64>,
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
    if p.secret.trim().is_empty() {
        return Err("pairing JSON is missing its secret".into());
    }
    if p.pear_public_key
        .as_deref()
        .is_some_and(|key| !valid_public_key(key))
    {
        return Err("pairing JSON contains an invalid Pear public key".into());
    }
    if p.host.trim().is_empty() && p.pear_public_key.is_none() {
        return Err("pairing JSON has neither a LAN endpoint nor a Pear public key".into());
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
        authenticated_name: (!p.name.trim().is_empty()).then_some(p.name),
        host: p.host,
        port: p.port,
        pear_public_key: p.pear_public_key,
        availability: p.availability,
        disabled: false,
        secret_b64: p.secret,
        created_at: now_ms(),
        last_seen_at: None,
        last_discovered_at: None,
        last_authenticated_at: None,
        last_authenticated_host: None,
        last_authenticated_port: None,
        last_error: None,
        next_retry_at: None,
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
            authenticated_name: p.authenticated_name.clone(),
            host: p.host.clone(),
            port: p.port,
            pear_public_key: p.pear_public_key.clone(),
            foreground_only: p.availability.as_deref() == Some("foreground"),
            enabled: !p.disabled,
            created_at: p.created_at,
            last_seen_at: p.last_seen_at,
            last_discovered_at: p.last_discovered_at,
            last_authenticated_at: p.last_authenticated_at,
            last_authenticated_host: p.last_authenticated_host.clone(),
            last_authenticated_port: p.last_authenticated_port,
            last_error: p.last_error.clone(),
            next_retry_at: p.next_retry_at,
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
pub async fn set_peer_enabled(
    state: State<'_, AppState>,
    id: String,
    enabled: bool,
) -> Result<(), String> {
    let mut guard = state.peers.write().await;
    let peer = guard
        .peers
        .iter_mut()
        .find(|peer| peer.id == id)
        .ok_or_else(|| "peer not found".to_string())?;
    peer.disabled = !enabled;
    save_peers(&state.paths.peers, &guard).map_err(|e| e.to_string())?;
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
    // Persist the user's routing policy before starting its runtime. A
    // transient Pear startup failure must not silently reset the selector to
    // Local only; retaining the preference also lets the next launch/pairing
    // attempt retry startup.
    crate::storage::settings::save(&state.paths.settings, &settings).map_err(|e| e.to_string())?;
    *state.settings.write().await = settings.clone();
    state.automatic_file_download_limit_bytes.store(
        settings.automatic_file_download_limit_bytes,
        std::sync::atomic::Ordering::Relaxed,
    );

    let sync_running = state.sync_shutdown.lock().unwrap().is_some();
    if sync_running {
        crate::apply_pear_mode(&state, settings.sync_transport_mode)
            .await
            .map_err(|error| format!("could not enable Pear: {error}"))?;
    } else {
        *state.transport_mode.write().await = settings.sync_transport_mode;
        crate::apply_pear_mode(&state, crate::sync::SyncTransportMode::LocalOnly)
            .await
            .map_err(|error| error.to_string())?;
        *state.transport_mode.write().await = settings.sync_transport_mode;
    }

    // Keep the pair-endpoint gate in lock-step with the discoverable toggle.
    // Turning discoverability OFF should immediately refuse new pair requests,
    // not "next restart."
    *state.pair_security.discoverable.write().await = settings.discoverable;

    // Reconcile mDNS advertising with the new `discoverable` setting — effective
    // immediately, no restart. When on, re-announce (drop + recreate) using the
    // privacy-preserving stable-ID label. Advertise the port the server actually bound
    // (a port change still needs a restart) so we never advertise a dead port.
    // With the sync stack switched off there is nothing to advertise at all.
    if settings.discoverable && sync_running {
        let bound_port = state
            .server_addr
            .read()
            .await
            .as_deref()
            .and_then(|a| a.rsplit(':').next())
            .and_then(|p| p.parse::<u16>().ok())
            .unwrap_or(settings.port);
        crate::announce_mdns(&state, bound_port).await;
    } else {
        // Drop the registration → mdns-sd unregisters the service.
        *state.mdns.lock().unwrap() = None;
    }

    Ok(settings)
}

#[tauri::command]
pub async fn get_desktop_prefs(state: State<'_, AppState>) -> Result<DesktopPrefs, String> {
    Ok(state.desktop_prefs.read().unwrap().clone())
}

#[tauri::command]
pub async fn update_desktop_prefs(
    state: State<'_, AppState>,
    prefs: DesktopPrefs,
) -> Result<DesktopPrefs, String> {
    // Only touch the registry when the startup pref actually changed. Every other
    // pref (tray, sync, …) rides the same command, and shelling out to `reg.exe`
    // for each of them is pure waste.
    let startup_changed = {
        let current = state.desktop_prefs.read().unwrap();
        current.launch_on_startup != prefs.launch_on_startup
    };
    if startup_changed {
        set_startup_enabled(prefs.launch_on_startup)?;
    }
    save_desktop_prefs_file(&state.paths.settings, &prefs)?;
    *state.desktop_prefs.write().unwrap() = prefs.clone();
    // Start or stop the LAN sync stack to match — effective immediately.
    crate::apply_sync_enabled(&state, prefs.sync_server_enabled).await;
    Ok(prefs)
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
fn author_message(state: &AppState, chat_id: &str, op: &str, msg: &Message) -> Result<(), String> {
    let meta = current_metadata(&state.events, chat_id)?;
    let payload = if meta.is_protected {
        let key = state
            .keystore
            .key_for(chat_id)
            .ok_or_else(|| "chat is locked — unlock it before sending".to_string())?;
        let plaintext = serde_json::to_vec(msg).map_err(|e| e.to_string())?;
        let sealed = crate::sync::protection::seal(&key, &plaintext).map_err(|e| e.to_string())?;
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
        let is_msg = w.op_type == op_type::MESSAGE_CREATED || w.op_type == op_type::MESSAGE_EDITED;
        if !is_msg {
            continue;
        }
        let plaintext = if w.payload.get("enc").is_some() {
            let key = old_key.ok_or_else(|| "chat already has encrypted content".to_string())?;
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
    let evts = state
        .events
        .read_chat(&chat_id)
        .map_err(|e| e.to_string())?;
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

/// Freeze a reply quote onto an outgoing message. The snapshot is taken here in the
/// backend rather than passed in by the UI: only this side can read the target inside
/// an unlocked protected chat, and only this side is authoritative about what the
/// target actually said.
fn attach_reply(
    state: &AppState,
    chat_id: &str,
    msg: Message,
    reply_to_id: Option<String>,
) -> Message {
    let target_id = match reply_to_id.filter(|s| !s.trim().is_empty()) {
        Some(id) => id,
        None => return msg,
    };
    let key = state.keystore.key_for(chat_id);
    match current_message(&state.events, chat_id, &target_id, key.as_ref()) {
        Ok(target) => msg.replying_to(&target),
        // Target deleted between opening the reply bar and sending. Keep the pointer
        // — it is still true — but there is no longer any text to freeze.
        Err(_) => Message {
            reply_to_id: Some(target_id),
            ..msg
        },
    }
}

#[tauri::command]
pub async fn author_send_text(
    state: State<'_, AppState>,
    chat_id: String,
    text: String,
    reply_to_id: Option<String>,
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
        file_attachments: vec![],
        tags: vec![],
        reminder_at: None,
        checklist_items: vec![],
        reactions: vec![],
        transcript: None,
        reply_to_id: None,
        reply_to_preview: None,
        reply_to_type: None,
        reply_to_created_at: None,
        author_device_id: Some(state.identity_device_id.clone()),
    };
    let msg = attach_reply(state.inner(), &chat_id, msg, reply_to_id);
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

/// Toggle a single emoji reaction on a message. Adds if absent, removes if
/// present. Key-aware so it works in an unlocked protected chat (reads the
/// decrypted message, re-seals the reaction change alongside the body).
#[tauri::command]
pub async fn author_toggle_reaction(
    state: State<'_, AppState>,
    chat_id: String,
    message_id: String,
    emoji: String,
) -> Result<(), String> {
    let trimmed = emoji.trim().to_string();
    if trimmed.is_empty() {
        return Err("emoji is empty".into());
    }
    let key = state.keystore.key_for(&chat_id);
    let mut msg = current_message(&state.events, &chat_id, &message_id, key.as_ref())?;
    if let Some(pos) = msg.reactions.iter().position(|e| e == &trimmed) {
        msg.reactions.remove(pos);
    } else {
        msg.reactions.push(trimmed);
    }
    msg.edited_at = Some(now_ms());
    author_message(state.inner(), &chat_id, op_type::MESSAGE_EDITED, &msg)
}

/// Set or clear a message's reminder time. `reminder_at` is Unix ms; `None`
/// clears it. Key-aware so it works in an unlocked protected chat (reads the
/// decrypted message, re-seals on author).
#[tauri::command]
pub async fn author_set_reminder(
    state: State<'_, AppState>,
    chat_id: String,
    message_id: String,
    reminder_at: Option<i64>,
) -> Result<(), String> {
    let key = state.keystore.key_for(&chat_id);
    let mut msg = current_message(&state.events, &chat_id, &message_id, key.as_ref())?;
    msg.reminder_at = reminder_at;
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

/// Toggle a Markdown task marker inside an ordinary TEXT message. The entire
/// message is emitted through the normal MessageEdited path; legacy CHECKLIST
/// messages continue to use `author_toggle_checklist_item` above.
#[tauri::command]
pub async fn author_toggle_text_checklist_block(
    state: State<'_, AppState>,
    chat_id: String,
    message_id: String,
    block_index: usize,
) -> Result<(), String> {
    let key = state.keystore.key_for(&chat_id);
    let mut msg = current_message(&state.events, &chat_id, &message_id, key.as_ref())?;
    if !matches!(msg.message_type, MessageType::Text) {
        return Err("not a text message".into());
    }
    let block = msg
        .content
        .get_mut(block_index)
        .ok_or_else(|| "checklist block not found".to_string())?;
    let span = block
        .spans
        .first_mut()
        .ok_or_else(|| "checklist marker not found".to_string())?;
    if span.text.starts_with("[ ] ") {
        span.text.replace_range(..3, "[x]");
    } else if span.text.starts_with("[x] ") || span.text.starts_with("[X] ") {
        span.text.replace_range(..3, "[ ]");
    } else {
        return Err("checklist marker not found".into());
    }
    msg.edited_at = Some(now_ms());
    author_message(state.inner(), &chat_id, op_type::MESSAGE_EDITED, &msg)
}

#[tauri::command]
pub async fn author_edit_text(
    state: State<'_, AppState>,
    chat_id: String,
    message_id: String,
    blocks: Vec<RichTextBlock>,
) -> Result<(), String> {
    let has_text = blocks
        .iter()
        .any(|block| block.spans.iter().any(|span| !span.text.trim().is_empty()));
    if !has_text {
        return Err("edit cannot be empty".to_string());
    }
    let key = state.keystore.key_for(&chat_id);
    let mut msg = current_message(&state.events, &chat_id, &message_id, key.as_ref())?;
    if !matches!(msg.message_type, MessageType::Text) {
        return Err("only TEXT messages support plaintext edit from desktop".to_string());
    }
    msg.content = blocks;
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
    reply_to_id: Option<String>,
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
        file_attachments: vec![],
        tags: vec![],
        reminder_at: None,
        checklist_items: cleaned,
        reactions: vec![],
        transcript: None,
        reply_to_id: None,
        reply_to_preview: None,
        reply_to_type: None,
        reply_to_created_at: None,
        author_device_id: Some(state.identity_device_id.clone()),
    };
    let msg = attach_reply(state.inner(), &chat_id, msg, reply_to_id);
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
/// so it shows immediately and syncs to any connected peer. The caller supplies
/// either an existing/new profile stamp or empty strings for the Default profile.
#[tauri::command]
pub async fn author_create_chat(
    state: State<'_, AppState>,
    name: String,
    profile_id: String,
    profile_name: String,
) -> Result<String, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("name cannot be empty".into());
    }
    let profile_id = profile_id.trim();
    let profile_name = profile_name.trim();
    if profile_id.is_empty() != profile_name.is_empty() {
        return Err("profile ID and name must either both be empty or both be set".into());
    }
    if !profile_id.is_empty()
        && (profile_id.len() > 128
            || !profile_id
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_'))
    {
        return Err("invalid profile ID".into());
    }
    if profile_name.len() > 120 {
        return Err("profile name is too long".into());
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
        remote_messages_on_left: false,
        retention_days: None,
    };
    let payload = serde_json::to_value(&meta).map_err(|e| e.to_string())?;
    author_event_with_profile(
        &state.clock,
        &state.events,
        &state.outbound,
        &state.bus,
        &state.identity_device_id,
        &chat_id,
        op_type::CHAT_METADATA_CHANGED,
        payload,
        profile_id,
        profile_name,
    )
    .map_err(|e| e.to_string())?;
    Ok(chat_id)
}

/// Patch chat-level metadata from the desktop and sync it back. Only the provided
/// fields change; everything else (avatar, background, protection salt, isProtected,
/// createdAt, bubble style, hideName) is preserved — important because
/// `ChatMetadataChanged` is whole-record LWW, so dropping a field would wipe it on
/// the phone.
// One optional parameter per patchable field is the shape Tauri commands take; a
// struct would have to be mirrored in TypeScript for no gain.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn author_update_chat_meta(
    state: State<'_, AppState>,
    chat_id: String,
    name: Option<String>,
    is_pinned: Option<bool>,
    is_archived: Option<bool>,
    tags: Option<Vec<String>>,
    remote_messages_on_left: Option<bool>,
    // Absent leaves the rule alone; `0` clears it; anything else sets the window.
    // A nested `Option<Option<_>>` would be the natural shape but serde cannot tell
    // an absent field from an explicit `null`, so both would arrive as `None`.
    retention_days: Option<u32>,
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
    if let Some(enabled) = remote_messages_on_left {
        meta.remote_messages_on_left = enabled;
    }
    if let Some(days) = retention_days {
        // Bounded: beyond a decade this is indistinguishable from "keep forever" and
        // only invites overflow in the cutoff arithmetic.
        meta.retention_days = Some(days).filter(|d| *d > 0 && *d <= 3650);
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

/// Set (or replace) a chat's avatar from a local file. Hashes the bytes,
/// persists the blob content-addressed, then authors a `ChatMetadataChanged`
/// pointing at `avatar/<hash>.jpg` — same on-the-wire shape the phone uses in
/// its `setChatAvatar`, so paired peers pick it up via the existing blob
/// transfer path.
#[tauri::command]
pub async fn author_set_chat_avatar(
    state: State<'_, AppState>,
    chat_id: String,
    source_path: String,
) -> Result<(), String> {
    let mut meta = current_metadata(&state.events, &chat_id)?;
    let bytes = std::fs::read(&source_path).map_err(|e| format!("read source: {e}"))?;
    if bytes.is_empty() {
        return Err("empty image file".into());
    }
    let hash = crate::sync::blob::sha256_hex(&bytes);
    state
        .blobs
        .write_verified(&hash, "image", &bytes)
        .map_err(|e| format!("persist blob: {e}"))?;
    meta.avatar_file = Some(format!("avatar/{}.jpg", hash));
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

/// Same as [`author_set_chat_avatar`] but for the chat background image.
#[tauri::command]
pub async fn author_set_chat_background(
    state: State<'_, AppState>,
    chat_id: String,
    source_path: String,
) -> Result<(), String> {
    let mut meta = current_metadata(&state.events, &chat_id)?;
    let bytes = std::fs::read(&source_path).map_err(|e| format!("read source: {e}"))?;
    if bytes.is_empty() {
        return Err("empty image file".into());
    }
    let hash = crate::sync::blob::sha256_hex(&bytes);
    state
        .blobs
        .write_verified(&hash, "image", &bytes)
        .map_err(|e| format!("persist blob: {e}"))?;
    meta.background_file = Some(format!("background/{}.jpg", hash));
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

/// Clear the chat's avatar (sets `avatar_file: null`).
#[tauri::command]
pub async fn author_clear_chat_avatar(
    state: State<'_, AppState>,
    chat_id: String,
) -> Result<(), String> {
    let mut meta = current_metadata(&state.events, &chat_id)?;
    meta.avatar_file = None;
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

/// Clear the chat's background image (sets `background_file: null`).
#[tauri::command]
pub async fn author_clear_chat_background(
    state: State<'_, AppState>,
    chat_id: String,
) -> Result<(), String> {
    let mut meta = current_metadata(&state.events, &chat_id)?;
    meta.background_file = None;
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
pub async fn author_delete_chat(state: State<'_, AppState>, chat_id: String) -> Result<(), String> {
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
    schedule_blob_gc(&state);
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
    schedule_blob_gc(&state);
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
    reply_to_id: Option<String>,
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
        file_attachments: vec![],
        tags: vec![],
        reminder_at: None,
        checklist_items: vec![],
        reactions: vec![],
        transcript: None,
        reply_to_id: None,
        reply_to_preview: None,
        reply_to_type: None,
        reply_to_created_at: None,
        author_device_id: Some(state.identity_device_id.clone()),
    };
    let msg = attach_reply(state.inner(), &chat_id, msg, reply_to_id);
    author_message(state.inner(), &chat_id, op_type::MESSAGE_CREATED, &msg)
}

/// Author a voice message. The frontend records via `MediaRecorder`, base64s the
/// bytes and calls this. The blob is stored content-addressed (mediaType=voice);
/// the reference format matches the phone's `voice/<hash>.m4a`. Duration is what
/// the recorder measured (ms) so peers show a length even before they download.
#[tauri::command]
pub async fn author_send_voice(
    state: State<'_, AppState>,
    chat_id: String,
    bytes_b64: String,
    duration_ms: Option<i64>,
    reply_to_id: Option<String>,
) -> Result<String, String> {
    use base64::{engine::general_purpose::STANDARD as B64, Engine};
    let bytes = B64
        .decode(bytes_b64.as_bytes())
        .map_err(|e| format!("bad base64: {e}"))?;
    if bytes.is_empty() {
        return Err("empty voice payload".into());
    }
    let hash = crate::sync::blob::sha256_hex(&bytes);
    state
        .blobs
        .write_verified(&hash, "voice", &bytes)
        .map_err(|e| format!("persist blob: {e}"))?;

    let reference = format!("voice/{}.m4a", hash);
    let msg = Message {
        id: uuid::Uuid::new_v4().to_string(),
        chat_id: chat_id.clone(),
        content: vec![],
        voice_file: Some(reference),
        voice_duration_ms: duration_ms,
        created_at: now_ms(),
        edited_at: None,
        message_type: MessageType::Voice,
        is_starred: false,
        image_files: vec![],
        file_attachments: vec![],
        tags: vec![],
        reminder_at: None,
        checklist_items: vec![],
        reactions: vec![],
        transcript: None,
        reply_to_id: None,
        reply_to_preview: None,
        reply_to_type: None,
        reply_to_created_at: None,
        author_device_id: Some(state.identity_device_id.clone()),
    };
    let msg = attach_reply(state.inner(), &chat_id, msg, reply_to_id);
    let id = msg.id.clone();
    author_message(state.inner(), &chat_id, op_type::MESSAGE_CREATED, &msg)?;
    // Return the id so the frontend can kick off `author_transcribe_voice`
    // in the background when auto-transcription is enabled.
    Ok(id)
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
    reply_to_id: Option<String>,
) -> Result<(), String> {
    let bytes = std::fs::read(&source_path).map_err(|e| format!("read source: {e}"))?;
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
        reply_to_id: None,
        reply_to_preview: None,
        reply_to_type: None,
        reply_to_created_at: None,
        author_device_id: Some(state.identity_device_id.clone()),
        image_files: vec![],
        file_attachments: vec![],
        tags: vec![],
        reminder_at: None,
        checklist_items: vec![],
        reactions: vec![],
        transcript: None,
    };
    let msg = attach_reply(state.inner(), &chat_id, msg, reply_to_id);
    author_message(state.inner(), &chat_id, op_type::MESSAGE_CREATED, &msg)
}

/// Attach an arbitrary local file as an opaque, content-addressed blob.
#[tauri::command]
pub async fn author_send_file(
    state: State<'_, AppState>,
    chat_id: String,
    source_path: String,
    display_name: String,
    reply_to_id: Option<String>,
) -> Result<(), String> {
    let source = std::path::Path::new(&source_path);
    let (hash, size) = state
        .blobs
        .import_file(source, "file")
        .map_err(|e| format!("import file: {e}"))?;
    let fallback_name = source
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("File");
    let name = if display_name.trim().is_empty() {
        fallback_name.to_string()
    } else {
        display_name.trim().to_string()
    };
    let msg = Message {
        id: uuid::Uuid::new_v4().to_string(),
        chat_id: chat_id.clone(),
        content: vec![],
        voice_file: None,
        voice_duration_ms: None,
        created_at: now_ms(),
        edited_at: None,
        message_type: MessageType::File,
        is_starred: false,
        image_files: vec![],
        file_attachments: vec![FileAttachment {
            file: format!("files/{hash}.bin"),
            name,
            mime_type: None,
            size_bytes: i64::try_from(size).ok(),
        }],
        tags: vec![],
        reminder_at: None,
        checklist_items: vec![],
        reactions: vec![],
        transcript: None,
        reply_to_id: None,
        reply_to_preview: None,
        reply_to_type: None,
        reply_to_created_at: None,
        author_device_id: Some(state.identity_device_id.clone()),
    };
    let msg = attach_reply(state.inner(), &chat_id, msg, reply_to_id);
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

fn validate_blob_ref(hash: &str, media_type: &str) -> Result<(), String> {
    if hash.len() != 64
        || !hash
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    {
        return Err("invalid blob hash".into());
    }
    if !matches!(media_type, "voice" | "image" | "file") {
        return Err("invalid blob media type".into());
    }
    Ok(())
}

#[tauri::command]
pub async fn blob_exists(
    state: State<'_, AppState>,
    hash: String,
    media_type: String,
) -> Result<bool, String> {
    validate_blob_ref(&hash, &media_type)?;
    Ok(state.blobs.exists(&hash, &media_type))
}

#[tauri::command]
pub async fn download_blob_anyway(
    state: State<'_, AppState>,
    hash: String,
    media_type: String,
) -> Result<(), String> {
    validate_blob_ref(&hash, &media_type)?;
    // Voice and image blobs can also fail to arrive — a dropped Pear transfer, or
    // an automatic-download limit that skipped them — and the user is then stuck
    // looking at "not on this device" with no recourse. The request path below is
    // media-type agnostic, so there is no reason to restrict this to attachments.
    if state.blobs.exists(&hash, &media_type) {
        return Ok(());
    }
    // Asking for it explicitly overrides an earlier eviction, otherwise the fetch
    // paths would keep skipping it.
    state.blobs.clear_eviction(&hash, &media_type);
    state
        .blob_requests
        .send(crate::sync::ManualBlobRequest {
            hash,
            media_type,
            cancel: false,
        })
        .map_err(|_| "no connected device is available to provide this file".to_string())?;
    Ok(())
}

/// True when this device dropped the blob on purpose.
///
/// The UI needs this to tell "removed on purpose" from "still arriving" — both are
/// simply "not on disk", and showing an evicted file as perpetually downloading is
/// worse than showing nothing.
#[tauri::command]
pub async fn blob_evicted(
    state: State<'_, AppState>,
    hash: String,
    media_type: String,
) -> Result<bool, String> {
    validate_blob_ref(&hash, &media_type)?;
    Ok(state.blobs.is_evicted(&hash, &media_type))
}

/// Drop this device's copy of a blob while keeping the message.
///
/// Local, deliberate, and sticky: the message keeps its attachment, but this device
/// stops holding the bytes and will not re-fetch them automatically. Lets an
/// always-on archive peer keep everything while a laptop or phone holds a subset.
/// "Download anyway" reverses it.
#[tauri::command]
pub async fn evict_blob(
    state: State<'_, AppState>,
    hash: String,
    media_type: String,
) -> Result<u64, String> {
    validate_blob_ref(&hash, &media_type)?;
    // Deliberately no `BlobFailed` on the bus: the frontend refreshes its own view,
    // and a "transfer failed" banner would misreport a choice the user just made.
    state
        .blobs
        .evict(&hash, &media_type)
        .map_err(|e| e.to_string())
}

/// Remove the content of already-deleted messages from the event logs.
///
/// Deleting a message used to only write a tombstone: the `MessageCreated` holding
/// the text stayed in the log, so anything ever deleted was still readable on disk.
/// Deletion now scrubs as it goes; this cleans up what was deleted before that.
///
/// Idempotent and cheap on repeat runs, so it is safe on every launch.
#[tauri::command]
pub async fn scrub_deleted_message_content(state: State<'_, AppState>) -> Result<usize, String> {
    let events = state.events.clone();
    let dropped = tokio::task::spawn_blocking(move || events.scrub_tombstoned_messages())
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    if dropped > 0 {
        tracing::info!("scrubbed {dropped} event(s) belonging to deleted messages");
    }
    Ok(dropped)
}

/// Forget how much of the log each device has been sent, forcing a full replay.
///
/// The outbound watermark is advanced optimistically as events are written to the
/// connection and persisted when the peer acks. A connection that dies partway
/// through — or, historically, one whose frames were being silently truncated —
/// can leave it standing past what the peer actually applied. Everything below it
/// is then unreachable: catch-up skips it forever, and a manual sync honestly
/// reports zero, because by its own bookkeeping there is nothing to send.
///
/// Deletions are the visible casualty. A message whose `MessageCreated` arrived
/// but whose `MessageTombstoned` fell into that gap stays on the peer for good.
///
/// Clearing the watermarks costs one full replay per device and nothing else:
/// event IDs are idempotent, so re-delivered events are absorbed, and pairing
/// trust is untouched. Returns how many watermarks were cleared.
/// Sync must be restarted for this to take effect: a live session clones the
/// watermark when it connects, so rewriting the file underneath it changes nothing
/// until it reconnects. The restore path already does this dance.
#[tauri::command]
pub async fn force_full_resync(state: State<'_, AppState>) -> Result<usize, String> {
    let restart_sync = state.sync_shutdown.lock().unwrap().is_some();
    if restart_sync {
        crate::apply_sync_enabled(state.inner(), false).await;
    }
    let result = {
        let mut pairings = state.pairings.write().await;
        let mut peers = state.peers.write().await;
        let cleared = reset_sync_watermarks(&mut pairings, &mut peers);
        if cleared == 0 {
            Ok(0usize)
        } else {
            save_pairings(&state.paths.pairings, &pairings)
                .map_err(|e| e.to_string())
                .and_then(|_| {
                    save_peers(&state.paths.peers, &peers).map_err(|e| e.to_string())
                })
                .map(|_| cleared)
        }
    };
    if restart_sync {
        crate::apply_sync_enabled(state.inner(), true).await;
    }
    let cleared = result?;
    if cleared > 0 {
        tracing::info!("cleared {cleared} sync watermark(s); peers replay on reconnect");
    }
    Ok(cleared)
}

/// Delete messages past their chat's retention window, everywhere.
///
/// Ordinary tombstones, so the deletion syncs exactly like a manual one and every
/// paired device converges. Any device may run this; a peer that already tombstoned
/// the same message absorbs the duplicate. Returns how many were removed.
///
/// Starred messages and ones with a future reminder are exempt — see
/// `storage::retention::expired_message_ids`.
#[tauri::command]
pub async fn apply_chat_retention(state: State<'_, AppState>) -> Result<usize, String> {
    let now = now_ms();
    let chat_ids = state.events.list_chat_ids().map_err(|e| e.to_string())?;
    let mut deleted = 0usize;

    for chat_id in chat_ids {
        let Ok(meta) = current_metadata(&state.events, &chat_id) else {
            continue;
        };
        if meta.retention_days.is_none_or(|days| days == 0) {
            continue;
        }
        // Materialize rather than reading raw events: a protected chat's bodies are
        // sealed, and `materialize` is what applies the key when one is cached. A
        // locked chat yields placeholders whose `is_starred` we cannot trust, so it
        // is skipped rather than swept blind.
        let Ok(evts) = state.events.read_chat(&chat_id) else {
            continue;
        };
        let key = state.keystore.key_for(&chat_id);
        let Some(chat) = crate::storage::materialize::materialize_with_key(evts, key.as_ref())
        else {
            continue;
        };
        // `is_unlocked` is literally "a key is cached", so it is false for every
        // ordinary chat too — only a *protected* chat without one is unreadable.
        // Skipping on `!is_unlocked` alone would skip every unprotected chat, which
        // is nearly all of them. A locked chat yields placeholder bodies whose
        // `is_starred` cannot be trusted, so it waits until unlocked.
        if meta.is_protected && !chat.is_unlocked {
            continue;
        }
        for message_id in crate::storage::retention::expired_message_ids(&meta, &chat.messages, now)
        {
            let payload = serde_json::json!({ "messageId": message_id });
            if author_event(
                &state.clock,
                &state.events,
                &state.outbound,
                &state.bus,
                &state.identity_device_id,
                &chat_id,
                op_type::MESSAGE_TOMBSTONED,
                payload,
            )
            .is_ok()
            {
                deleted += 1;
            }
        }
    }

    if deleted > 0 {
        tracing::info!("retention: deleted {deleted} expired message(s)");
        // Their attachments are now unreferenced.
        schedule_blob_gc(&state);
    }
    Ok(deleted)
}

/// Quiet period after the last deletion before the automatic sweep runs.
const BLOB_GC_DEBOUNCE: std::time::Duration = std::time::Duration::from_secs(30);

/// Queue a background sweep after a deletion.
///
/// A blob cannot simply be deleted alongside its message: the store is
/// content-addressed, so the same photo sent to three chats is *one* file, and one
/// of those messages going away says nothing about the other two. Deciding whether
/// the bytes are still wanted means the same whole-log scan the sweep already does.
/// So the delete path just marks the store dirty and the scan runs once, shortly
/// after the user stops deleting things — rather than once per deleted message.
pub fn schedule_blob_gc(state: &AppState) {
    // A sweep is already pending; it will pick up this deletion too.
    if state
        .blob_gc_pending
        .swap(true, std::sync::atomic::Ordering::SeqCst)
    {
        return;
    }
    let events = state.events.clone();
    let blobs = state.blobs.clone();
    let keystore = state.keystore.clone();
    let pending = state.blob_gc_pending.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(BLOB_GC_DEBOUNCE).await;
        pending.store(false, std::sync::atomic::Ordering::SeqCst);
        let report = tokio::task::spawn_blocking(move || {
            crate::sync::collect_unreferenced_blobs(&events, &blobs, &keystore, false)
        })
        .await;
        match report {
            Ok(Ok(report)) if report.deleted > 0 => tracing::info!(
                "blob gc: reclaimed {} bytes across {} files",
                report.reclaimed_bytes,
                report.deleted
            ),
            Ok(Ok(report)) if !report.locked_chats.is_empty() => tracing::debug!(
                "blob gc: skipped, {} protected chat(s) locked",
                report.locked_chats.len()
            ),
            Ok(Err(e)) => tracing::warn!("blob gc failed: {e}"),
            _ => {}
        }
    });
}

/// Measure the media store, or sweep blobs no stored event refers to any more.
///
/// Nothing has ever removed a blob: the store is content-addressed and append-only,
/// so deleting a message leaves its attachment on disk indefinitely. That is both
/// unbounded growth and a privacy surprise — re-sending a "deleted" file completes
/// instantly because the bytes never left.
///
/// `dry_run` measures without touching anything. If any protected chat is locked its
/// media references are still sealed, so the sweep reports those chats and deletes
/// nothing rather than guessing.
#[tauri::command]
pub async fn collect_unused_blobs(
    state: State<'_, AppState>,
    dry_run: bool,
) -> Result<crate::sync::BlobGcReport, String> {
    let events = state.events.clone();
    let blobs = state.blobs.clone();
    let keystore = state.keystore.clone();
    // Walks every chat log and stats the whole store — keep it off the async runtime.
    tokio::task::spawn_blocking(move || {
        crate::sync::collect_unreferenced_blobs(&events, &blobs, &keystore, dry_run)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Abandon an in-flight download. The partial file is discarded and the automatic
/// reconcile stops asking for this blob until the user requests it again — without
/// that it would simply restart on the next sweep.
#[tauri::command]
pub async fn cancel_blob_download(
    state: State<'_, AppState>,
    hash: String,
    media_type: String,
) -> Result<(), String> {
    validate_blob_ref(&hash, &media_type)?;
    // A cancel with nothing listening is a no-op, not an error: the transfer is
    // already not running, which is exactly what the user asked for.
    let _ = state.blob_requests.send(crate::sync::ManualBlobRequest {
        hash,
        media_type,
        cancel: true,
    });
    Ok(())
}

/** Stream/copy an opaque blob to a path chosen by the user without base64 IPC. */
#[tauri::command]
pub async fn save_blob(
    state: State<'_, AppState>,
    hash: String,
    media_type: String,
    dest: String,
) -> Result<(), String> {
    validate_blob_ref(&hash, &media_type)?;
    let source = state.blobs.path_for(&hash, &media_type);
    if !source.is_file() {
        return Err("file not on this device yet".into());
    }
    std::fs::copy(source, dest).map_err(|e| e.to_string())?;
    Ok(())
}

// ---------- Image viewer: view / save / copy ----------

/// An image blob, ready to render — plus the format we had to sniff for it.
/// No dimensions: the webview reads `naturalWidth`/`naturalHeight` off the
/// `<img>`, so reporting them here would mean decoding the whole image twice.
#[derive(Debug, Clone, Serialize)]
pub struct ImageView {
    /// Base64 of the raw stored bytes, for a `data:` URL.
    #[serde(rename = "dataB64")]
    pub data_b64: String,
    /// The real MIME type, so the `data:` URL doesn't lie about the payload.
    pub mime: String,
    /// The real extension, used as the save dialog's default.
    pub ext: String,
}

/// Read an image blob and sniff what it actually is.
///
/// Nothing records an image's format: `author_send_image` stores whatever bytes
/// the user picked and labels the reference `images/<hash>.jpg` regardless. So a
/// PNG is on disk under a `.jpg` name. Sniff the magic bytes rather than trust
/// that label, or we'd save a PNG as `photo.jpg`.
fn load_image(state: &AppState, hash: &str) -> Result<(Vec<u8>, image::ImageFormat), String> {
    if !state.blobs.exists(hash, "image") {
        return Err("image not on this device yet".into());
    }
    let bytes = state.blobs.read(hash, "image").map_err(|e| e.to_string())?;
    let format = image::guess_format(&bytes).map_err(|e| format!("unrecognized image: {e}"))?;
    Ok((bytes, format))
}

#[tauri::command]
pub async fn image_view(state: State<'_, AppState>, hash: String) -> Result<ImageView, String> {
    use base64::{engine::general_purpose::STANDARD as B64, Engine};
    let (bytes, format) = load_image(state.inner(), &hash)?;
    Ok(ImageView {
        data_b64: B64.encode(&bytes),
        mime: format.to_mime_type().to_string(),
        ext: format
            .extensions_str()
            .first()
            .copied()
            .unwrap_or("bin")
            .to_string(),
    })
}

/// Copy the stored bytes to `dest` verbatim — no re-encode, so the saved file is
/// bit-identical to what the sender shared.
#[tauri::command]
pub async fn save_image(
    state: State<'_, AppState>,
    hash: String,
    dest: String,
) -> Result<(), String> {
    let (bytes, _) = load_image(state.inner(), &hash)?;
    std::fs::write(&dest, bytes).map_err(|e| format!("write {dest}: {e}"))
}

/// Put the image's pixels on the clipboard, so it pastes into Paint, Word, a
/// chat box — anywhere expecting an image rather than a file.
///
/// The clipboard takes raw RGBA, not JPEG, hence the decode. Done here rather
/// than via a canvas round-trip in the webview so the bytes never leave Rust.
#[tauri::command]
pub async fn copy_image(
    app: AppHandle,
    state: State<'_, AppState>,
    hash: String,
) -> Result<(), String> {
    use tauri_plugin_clipboard_manager::ClipboardExt;

    let (bytes, format) = load_image(state.inner(), &hash)?;
    let decoded = image::load_from_memory_with_format(&bytes, format)
        .map_err(|e| format!("decode image: {e}"))?
        .to_rgba8();
    let (width, height) = (decoded.width(), decoded.height());
    let image = tauri::image::Image::new(decoded.as_raw(), width, height);
    app.clipboard()
        .write_image(&image)
        .map_err(|e| format!("clipboard: {e}"))
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
fn verified_import_blob_name(name: &str, bytes: &[u8]) -> Option<String> {
    if name.len() != 64 || !name.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let normalized = name.to_ascii_lowercase();
    (crate::sync::blob::sha256_hex(bytes) == normalized).then_some(normalized)
}

#[tauri::command]
pub async fn export_backup(state: State<'_, AppState>, dest_path: String) -> Result<(), String> {
    export_backup_to(state.inner(), &dest_path)
}

fn export_backup_to(state: &AppState, dest_path: &str) -> Result<(), String> {
    use std::io::Write;
    let file = std::fs::File::create(dest_path).map_err(|e| format!("create {dest_path}: {e}"))?;
    let mut zip = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    let mut copy_dir =
        |dir: &std::path::Path, prefix: &str, ndjson_only: bool| -> Result<(), String> {
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
    #[allow(clippy::drop_non_drop)] // end the closure borrow of zip before writing the graveyard
    drop(copy_dir);
    let graveyard = state.paths.events.join("chat-tombstones.graveyard");
    if graveyard.is_file() {
        zip.start_file("events/chat-tombstones.graveyard", opts)
            .map_err(|e| e.to_string())?;
        zip.write_all(&std::fs::read(graveyard).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    }
    let message_tombstones = state
        .events
        .read_all_message_graveyard()
        .map_err(|e| e.to_string())?;
    if !message_tombstones.is_empty() {
        zip.start_file("events/message-tombstones.ndjson", opts)
            .map_err(|e| e.to_string())?;
        for event in message_tombstones {
            writeln!(
                zip,
                "{}",
                serde_json::to_string(&event).map_err(|e| e.to_string())?
            )
            .map_err(|e| e.to_string())?;
        }
    }
    zip.finish().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn run_automatic_backup(state: State<'_, AppState>) -> Result<String, String> {
    let prefs = state.desktop_prefs.read().unwrap().clone();
    let folder = prefs
        .auto_backup_folder
        .ok_or("Choose an automatic backup folder")?;
    std::fs::create_dir_all(&folder).map_err(|e| format!("create backup folder: {e}"))?;
    let stamp = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S");
    let path = std::path::Path::new(&folder).join(format!("ncy-auto-backup-{stamp}.zip"));
    export_backup_to(state.inner(), path.to_string_lossy().as_ref())?;
    if let Some(keep) = prefs.auto_backup_retention {
        let mut files: Vec<_> = std::fs::read_dir(&folder)
            .map_err(|e| e.to_string())?
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("ncy-auto-backup-") && n.ends_with(".zip"))
            })
            .collect();
        files.sort();
        let remove_count = files.len().saturating_sub(keep.max(1) as usize);
        for old in files.into_iter().take(remove_count) {
            std::fs::remove_file(old).map_err(|e| e.to_string())?;
        }
    }
    Ok(path.to_string_lossy().into_owned())
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

    // Validate every event clock before stopping sync or mutating local state.
    // Re-open entries by index during the import pass below.
    let import_now = now_ms();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let name = entry.name().to_string();
        let Some(fname) = name.strip_prefix("events/") else {
            continue;
        };
        if entry.is_dir()
            || (!fname.ends_with(".ndjson") && fname != "chat-tombstones.graveyard")
            || fname.contains('/')
        {
            continue;
        }
        let mut buf = String::new();
        entry.read_to_string(&mut buf).map_err(|e| e.to_string())?;
        for line in buf.lines().map(str::trim).filter(|line| !line.is_empty()) {
            if let Ok(wire) = serde_json::from_str::<crate::model::event::ChatEventWire>(line) {
                if !wire.hlc.is_acceptable_remote(import_now) {
                    return Err(
                        "backup contains an invalid or future-skewed event clock".to_string()
                    );
                }
            }
        }
    }

    // Restoring events invalidates every local statement of sync progress. Stop
    // live sessions, persist cleared watermarks before touching event state, and
    // restart in all success/error paths. A crash during import therefore also
    // restarts safely from zero on the next application launch.
    let restart_sync = state.sync_shutdown.lock().unwrap().is_some();
    if restart_sync {
        crate::apply_sync_enabled(state.inner(), false).await;
    }
    let reset_result: Result<(), String> = {
        let mut pairings = state.pairings.write().await;
        let mut peers = state.peers.write().await;
        let cleared = reset_sync_watermarks(&mut pairings, &mut peers);
        if cleared == 0 {
            Ok(())
        } else {
            save_pairings(&state.paths.pairings, &pairings)
                .map_err(|e| e.to_string())
                .and_then(|_| save_peers(&state.paths.peers, &peers).map_err(|e| e.to_string()))
        }
    };
    if let Err(error) = reset_result {
        if restart_sync {
            crate::apply_sync_enabled(state.inner(), true).await;
        }
        return Err(error);
    }

    let result = (|| -> Result<ImportSummary, String> {
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
                if (!fname.ends_with(".ndjson") && fname != "chat-tombstones.graveyard")
                    || fname.contains('/')
                {
                    continue;
                }
                if fname != "chat-tombstones.graveyard" && fname != "message-tombstones.ndjson" {
                    chat_files.insert(fname.to_string());
                }
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
                        let added = if fname == "chat-tombstones.graveyard" {
                            state.events.archive_chat_tombstone(&wire).unwrap_or(false)
                        } else {
                            state.events.append(&wire).unwrap_or(false)
                        };
                        if added {
                            events_added += 1;
                            touched.insert(wire.chat_id.clone());
                        }
                    }
                }
            } else if let Some(fname) = name.strip_prefix("blobs/") {
                // Content-addressed flat names only — reject anything path-ish.
                if fname.is_empty()
                    || fname.contains('/')
                    || fname.contains('\\')
                    || fname.contains("..")
                {
                    continue;
                }
                let mut bytes = Vec::new();
                entry.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
                let Some(verified_name) = verified_import_blob_name(fname, &bytes) else {
                    continue;
                };
                let dest = state.blobs.root().join(verified_name);
                if dest.exists() {
                    continue;
                }
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
    })();

    if restart_sync {
        crate::apply_sync_enabled(state.inner(), true).await;
    }
    result
}

#[tauri::command]
pub async fn purge_chat_tombstones(state: State<'_, AppState>) -> Result<usize, String> {
    state.events.purge_tombstones().map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Voice transcription (whisper.cpp via subprocess). See `storage/whisper.rs`
// for the engine details.
// ---------------------------------------------------------------------------

fn whisper_paths(state: &AppState) -> crate::storage::whisper::WhisperPaths {
    crate::storage::whisper::WhisperPaths::from_data_root(&state.paths.root)
}

fn extract_voice_hash(reference: &str) -> Option<String> {
    // Reference format is `voice/<64-hex>.m4a` — same shape phone-authored
    // messages carry, mirrored by desktop's own `author_send_voice`.
    let stem = reference
        .strip_prefix("voice/")
        .and_then(|s| s.strip_suffix(".m4a"))?;
    if stem.len() == 64 && stem.chars().all(|c| c.is_ascii_hexdigit()) {
        Some(stem.to_string())
    } else {
        None
    }
}

/// Local install probe — the Settings UI polls this to render the CLI /
/// model status card.
#[tauri::command]
pub async fn transcription_status(
    state: State<'_, AppState>,
) -> Result<crate::storage::whisper::WhisperStatus, String> {
    let paths = whisper_paths(state.inner());
    Ok(crate::storage::whisper::WhisperStatus::probe(&paths))
}

/// Streaming download of `ggml-tiny.bin` into the app's whisper dir. Emits
/// `transcription-event` payloads on the tauri event bus so the Settings UI
/// can render a progress bar. Blocks until finished (or errored).
#[tauri::command]
pub async fn transcription_download_model(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let paths = whisper_paths(state.inner());
    let app_for_progress = app.clone();
    let model_id = paths.model_id();
    let result =
        crate::storage::whisper::download_model(&paths, &model_id, move |written, total| {
            let _ = app_for_progress.emit(
                "transcription-event",
                serde_json::json!({
                    "kind": "download-progress",
                    "written": written,
                    "total": total,
                }),
            );
        })
        .await;
    match result {
        Ok(()) => {
            let _ = app.emit(
                "transcription-event",
                serde_json::json!({ "kind": "download-complete" }),
            );
            Ok(())
        }
        Err(e) => {
            let msg = e.to_string();
            let _ = app.emit(
                "transcription-event",
                serde_json::json!({ "kind": "download-error", "message": msg.clone() }),
            );
            Err(msg)
        }
    }
}

/// One-click local transcription setup. Installs the tested whisper.cpp
/// runtime first and then the model, skipping either component when it is
/// already present so interrupted setups are naturally resumable.
#[tauri::command]
pub async fn transcription_setup(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let paths = whisper_paths(state.inner());
    let result: anyhow::Result<()> = async {
        if !paths.cli_present() {
            let progress_app = app.clone();
            crate::storage::whisper::download_runtime(&paths, move |written, total| {
                let _ = progress_app.emit(
                    "transcription-event",
                    serde_json::json!({
                        "kind": "download-progress",
                        "stage": "runtime",
                        "written": written,
                        "total": total,
                    }),
                );
            })
            .await?;
        }
        if !paths.model_present() {
            let progress_app = app.clone();
            let model_id = paths.model_id();
            crate::storage::whisper::download_model(&paths, &model_id, move |written, total| {
                let _ = progress_app.emit(
                    "transcription-event",
                    serde_json::json!({
                        "kind": "download-progress",
                        "stage": "model",
                        "written": written,
                        "total": total,
                    }),
                );
            })
            .await?;
        }
        Ok(())
    }
    .await;

    match result {
        Ok(()) => {
            let _ = app.emit(
                "transcription-event",
                serde_json::json!({ "kind": "download-complete" }),
            );
            Ok(())
        }
        Err(error) => {
            let message = error.to_string();
            let _ = app.emit(
                "transcription-event",
                serde_json::json!({ "kind": "download-error", "message": message.clone() }),
            );
            Err(message)
        }
    }
}

#[tauri::command]
pub async fn transcription_select_model(
    app: AppHandle,
    state: State<'_, AppState>,
    model_id: String,
) -> Result<(), String> {
    let paths = whisper_paths(state.inner());
    if paths.model_cached(&model_id) {
        paths
            .activate_cached_model(&model_id)
            .map_err(|error| error.to_string())?;
        let _ = app.emit(
            "transcription-event",
            serde_json::json!({ "kind": "download-complete" }),
        );
        return Ok(());
    }
    let progress_app = app.clone();
    let result =
        crate::storage::whisper::download_model(&paths, &model_id, move |written, total| {
            let _ = progress_app.emit("transcription-event", serde_json::json!({
            "kind": "download-progress", "stage": "model", "written": written, "total": total,
        }));
        })
        .await;
    match result {
        Ok(()) => {
            let _ = app.emit(
                "transcription-event",
                serde_json::json!({ "kind": "download-complete" }),
            );
            Ok(())
        }
        Err(error) => {
            let message = error.to_string();
            let _ = app.emit(
                "transcription-event",
                serde_json::json!({ "kind": "download-error", "message": message.clone() }),
            );
            Err(message)
        }
    }
}

#[tauri::command]
pub async fn transcription_remove_unused_models(state: State<'_, AppState>) -> Result<u64, String> {
    whisper_paths(state.inner())
        .remove_unused_models()
        .map_err(|error| error.to_string())
}

/// Open the whisper install folder in the OS file explorer for diagnostics or
/// manual installation on platforms where automatic setup is not yet offered.
#[tauri::command]
pub async fn transcription_open_folder(state: State<'_, AppState>) -> Result<(), String> {
    let paths = whisper_paths(state.inner());
    paths.ensure_dir().map_err(|e| e.to_string())?;
    #[cfg(windows)]
    {
        std::process::Command::new("explorer")
            .arg(&paths.dir)
            .spawn()
            .map_err(|e| format!("open folder: {e}"))?;
    }
    #[cfg(not(windows))]
    {
        std::process::Command::new(if cfg!(target_os = "macos") {
            "open"
        } else {
            "xdg-open"
        })
        .arg(&paths.dir)
        .spawn()
        .map_err(|e| format!("open folder: {e}"))?;
    }
    Ok(())
}

/// Delete the model file. UI calls this from the "Delete model" button in
/// Settings to reclaim ~74 MB.
#[tauri::command]
pub async fn transcription_delete_model(state: State<'_, AppState>) -> Result<(), String> {
    let paths = whisper_paths(state.inner());
    if paths.model.is_file() {
        std::fs::remove_file(&paths.model).map_err(|e| format!("delete model: {e}"))?;
    }
    let _ = std::fs::remove_file(paths.dir.join("model-id.txt"));
    Ok(())
}

/// Transcribe a voice message, patching the message's `transcript` field via a
/// `MessageEdited` event that syncs to every paired peer. Key-aware (handles
/// unlocked protected chats). Blocks until whisper-cli finishes — the frontend
/// awaits this and shows a "Transcribing…" indicator meanwhile.
#[tauri::command]
pub async fn author_transcribe_voice(
    state: State<'_, AppState>,
    chat_id: String,
    message_id: String,
) -> Result<(), String> {
    let key = state.keystore.key_for(&chat_id);
    let msg_ref = {
        let msg = current_message(&state.events, &chat_id, &message_id, key.as_ref())?;
        if !matches!(msg.message_type, MessageType::Voice) {
            return Err("not a voice message".into());
        }
        msg.voice_file
            .clone()
            .ok_or_else(|| "voice message has no blob reference".to_string())?
    };
    let hash = extract_voice_hash(&msg_ref)
        .ok_or_else(|| format!("cannot parse voice reference {msg_ref}"))?;
    if !state.blobs.exists(&hash, "voice") {
        return Err("voice blob not yet on this device — wait for it to sync".into());
    }
    let bytes = state
        .blobs
        .read(&hash, "voice")
        .map_err(|e| format!("read blob: {e}"))?;

    let paths = whisper_paths(state.inner());
    // Whisper is blocking (subprocess) — move it off the async runtime so we
    // don't stall the tauri worker while it grinds.
    let transcript = tokio::task::spawn_blocking(move || {
        crate::storage::whisper::transcribe_bytes(&paths, bytes)
    })
    .await
    .map_err(|e| format!("transcription join: {e}"))?
    .map_err(|e| format!("transcription: {e}"))?;

    // Re-read the message (it may have changed while whisper was running) and
    // patch just the transcript + edited_at, mirroring how star / reminder /
    // reactions all re-emit `MessageEdited`.
    let mut msg = current_message(&state.events, &chat_id, &message_id, key.as_ref())?;
    msg.transcript = Some(transcript);
    msg.edited_at = Some(now_ms());
    author_message(state.inner(), &chat_id, op_type::MESSAGE_EDITED, &msg)
}

/// Forward sync server events into a Tauri event channel so the frontend can react
/// to them (peer connected, new messages, etc.) without polling.
pub fn spawn_sync_event_forwarder(
    app: AppHandle,
    mut rx: tokio::sync::broadcast::Receiver<SyncEvent>,
) {
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
        } => {
            serde_json::json!({"kind":"peer-connected","peer":peer,"pairingId":pairing_id,"phoneId":phone_id})
        }
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
        SyncEvent::BlobProgress {
            hash,
            received,
            total,
        } => {
            serde_json::json!({"kind":"blob-progress","hash":hash,"received":received,"total":total})
        }
        SyncEvent::BlobFailed { hash, reason } => {
            serde_json::json!({"kind":"blob-failed","hash":hash,"reason":reason})
        }
        SyncEvent::PeerDisconnected { peer } => {
            serde_json::json!({"kind":"peer-disconnected","peer":peer})
        }
        SyncEvent::PeerError { peer, message } => {
            serde_json::json!({"kind":"peer-error","peer":peer,"message":message})
        }
        SyncEvent::ServerStarted { addr } => {
            serde_json::json!({"kind":"server-started","addr":addr})
        }
        SyncEvent::ServerStopped => serde_json::json!({"kind":"server-stopped"}),
        SyncEvent::ServerError { message } => {
            serde_json::json!({"kind":"server-error","message":message})
        }
        SyncEvent::PairRequested {
            session_id,
            peer,
            phone_id,
            phone_name,
            safety_code,
        } => serde_json::json!({
            "kind":"pair-requested",
            "sessionId": session_id,
            "peer": peer,
            "phoneId": phone_id,
            "phoneName": phone_name,
            "safetyCode": safety_code,
        }),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pairing_view_never_serializes_the_pairing_secret() {
        let pairing = Pairing::new_with_label("Temporary invitation");
        let secret = pairing.secret_b64.clone();
        let json = serde_json::to_string(&PairingView::from(&pairing)).expect("serialize view");

        assert!(!json.contains("secret"));
        assert!(!json.contains(&secret));
    }

    /// A 1x1 opaque red PNG. Real bytes, so it decodes as well as sniffs.
    const RED_PNG: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90,
        0x77, 0x53, 0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, 0x08, 0xD7, 0x63, 0xF8,
        0xCF, 0xC0, 0x00, 0x00, 0x03, 0x01, 0x01, 0x00, 0x18, 0xDD, 0x8D, 0xB0, 0x00, 0x00, 0x00,
        0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];

    /// A stored image is labelled `.jpg` whatever it is, so the viewer sniffs. The
    /// save dialog shows whatever `extensions_str().first()` returns — pin the
    /// exact strings, since "jpeg" vs "jpg" is user-visible in the filename.
    #[test]
    fn sniffed_format_drives_extension_and_mime() {
        let png = image::guess_format(RED_PNG).expect("sniff png");
        assert_eq!(png, image::ImageFormat::Png);
        assert_eq!(png.extensions_str().first().copied(), Some("png"));
        assert_eq!(png.to_mime_type(), "image/png");

        // JPEG's SOI + APP0 marker is enough for guess_format.
        let jpeg_magic = [0xFF, 0xD8, 0xFF, 0xE0];
        let jpeg = image::guess_format(&jpeg_magic).expect("sniff jpeg");
        assert_eq!(jpeg, image::ImageFormat::Jpeg);
        assert_eq!(jpeg.extensions_str().first().copied(), Some("jpg"));
        assert_eq!(jpeg.to_mime_type(), "image/jpeg");
    }

    /// A PNG mislabelled `.jpg` must not be reported as JPEG — that's the whole
    /// reason `image_view` sniffs rather than reading the reference's extension.
    #[test]
    fn png_stored_under_a_jpg_reference_is_still_png() {
        let format = image::guess_format(RED_PNG).expect("sniff");
        assert_eq!(format.to_mime_type(), "image/png");
    }

    /// `copy_image` decodes to RGBA before handing pixels to the clipboard, which
    /// cannot take JPEG. Check the decode path the command relies on.
    #[test]
    fn images_decode_to_rgba_for_the_clipboard() {
        let format = image::guess_format(RED_PNG).unwrap();
        let rgba = image::load_from_memory_with_format(RED_PNG, format)
            .expect("decode")
            .to_rgba8();
        assert_eq!((rgba.width(), rgba.height()), (1, 1));
        // 4 bytes per pixel is what tauri::image::Image::new expects.
        assert_eq!(rgba.as_raw().len(), 4);
        assert_eq!(rgba.as_raw()[3], 255, "alpha channel present");
    }

    /// Garbage must surface as an error, not a panic or a bogus ".bin" save.
    #[test]
    fn unrecognized_bytes_are_rejected() {
        assert!(image::guess_format(b"not an image at all").is_err());
    }

    #[test]
    fn imported_blob_name_must_match_its_content() {
        let bytes = b"verified blob";
        let hash = crate::sync::blob::sha256_hex(bytes);
        assert_eq!(
            verified_import_blob_name(&hash.to_uppercase(), bytes),
            Some(hash)
        );
        assert_eq!(verified_import_blob_name(&"0".repeat(64), bytes), None);
        assert_eq!(verified_import_blob_name("../blob", bytes), None);
    }

    /// A fresh install has no `desktop_prefs.json` at all, so it takes `Default`:
    /// sync off, and the close button asks rather than silently hiding.
    #[test]
    fn fresh_install_defaults_to_sync_off_and_asking() {
        let prefs = DesktopPrefs::default();
        assert!(!prefs.sync_server_enabled);
        assert!(!prefs.close_to_tray);
        assert!(prefs.ask_close_to_tray);
    }

    /// An upgrade reads a file written before either key existed. Sync must stay
    /// on — those users are already paired with a phone — and the recorded
    /// `closeToTray` must survive the change of `Default`.
    #[test]
    fn existing_config_keeps_syncing_after_upgrade() {
        let legacy = r#"{
            "startMinimized": false,
            "closeToTray": true,
            "launchOnStartup": false,
            "askBeforeDeleting": true,
            "autoCheckUpdates": true
        }"#;
        let prefs: DesktopPrefs = serde_json::from_str(legacy).expect("parse legacy prefs");
        assert!(prefs.sync_server_enabled, "upgrade must not stop sync");
        assert!(prefs.close_to_tray, "recorded closeToTray must survive");
        assert!(prefs.ask_close_to_tray);
    }

    /// Turning the toggle off has to stick across restarts — i.e. an explicit
    /// `false` must not be overwritten by the migration default.
    #[test]
    fn explicit_sync_off_is_preserved() {
        let prefs: DesktopPrefs =
            serde_json::from_str(r#"{"syncServerEnabled": false}"#).expect("parse");
        assert!(!prefs.sync_server_enabled);
    }

    /// Unknown/partial files must not wipe the remaining prefs (the container
    /// `#[serde(default)]` fills the gaps).
    #[test]
    fn partial_config_fills_gaps_without_erroring() {
        let prefs: DesktopPrefs =
            serde_json::from_str(r#"{"startMinimized": true}"#).expect("parse");
        assert!(prefs.start_minimized);
        assert!(prefs.ask_before_deleting);
    }
}
