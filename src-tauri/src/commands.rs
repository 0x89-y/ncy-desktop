use std::path::PathBuf;
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
    /// Run the LAN sync stack (WS listener, mDNS, outbound peers). Off for fresh
    /// installs — Ncy works standalone, and sync is opt-in.
    ///
    /// The field default is `true`, not `false`: it only fires when a *existing*
    /// `desktop_prefs.json` is read that predates this key, and those users are
    /// already syncing with a phone. A fresh install has no file at all, so it
    /// takes `Default` below and starts off.
    #[serde(rename = "syncServerEnabled", default = "sync_enabled_for_existing_install")]
    pub sync_server_enabled: bool,
}

/// See `DesktopPrefs::sync_server_enabled` — migration default for pre-existing
/// config files, which must keep syncing after an upgrade.
fn sync_enabled_for_existing_install() -> bool {
    true
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

fn save_desktop_prefs_file(settings_path: &std::path::Path, prefs: &DesktopPrefs) -> Result<(), String> {
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
pub async fn pair_reject(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<(), String> {
    let session = {
        let mut reg = state.pairing_registry.lock().await;
        reg.remove(&session_id)
    };
    if let Some(session) = session {
        let _ = session
            .decision_tx
            .send(crate::sync::PairDecision::Reject {
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

    // Keep the pair-endpoint gate in lock-step with the discoverable toggle.
    // Turning discoverability OFF should immediately refuse new pair requests,
    // not "next restart."
    *state.pair_security.discoverable.write().await = settings.discoverable;

    // Reconcile mDNS advertising with the new `discoverable` setting — effective
    // immediately, no restart. When on, re-announce (drop + recreate) so a changed
    // device name is picked up too. Advertise the port the server actually bound
    // (a port change still needs a restart) so we never advertise a dead port.
    // With the sync stack switched off there is nothing to advertise at all.
    let sync_running = state.sync_shutdown.lock().unwrap().is_some();
    if settings.discoverable && sync_running {
        let bound_port = state
            .server_addr
            .read()
            .await
            .as_deref()
            .and_then(|a| a.rsplit(':').next())
            .and_then(|p| p.parse::<u16>().ok())
            .unwrap_or(settings.port);
        crate::announce_mdns(&state, &settings, bound_port).await;
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
        reactions: vec![],
        transcript: None,
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
        reactions: vec![],
        transcript: None,
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
        reactions: vec![],
        transcript: None,
    };
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
        tags: vec![],
        reminder_at: None,
        checklist_items: vec![],
        reactions: vec![],
        transcript: None,
    };
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
        reactions: vec![],
        transcript: None,
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
    let result = crate::storage::whisper::download_model(&paths, move |written, total| {
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

/// Open the whisper install folder in the OS file explorer so the user can
/// drop `whisper-cli.exe` in place manually (release-URL scraping is brittle,
/// so v1 keeps the CLI a manual-install step).
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
        std::process::Command::new(if cfg!(target_os = "macos") { "open" } else { "xdg-open" })
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
