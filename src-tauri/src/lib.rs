pub mod commands;

// The core modules now live in the shared `ncy-core` crate. Re-export them under
// this crate's root so the existing `crate::model::…` / `crate::storage::…` /
// `crate::sync::…` / `crate::network::…` paths throughout `commands.rs` keep
// resolving unchanged.
pub use ncy_core::{model, network, storage, sync};

use std::sync::Arc;

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Manager, WindowEvent};
use tokio::sync::{broadcast, Notify, RwLock};

use crate::model::HlcTimestamp;
use crate::storage::{
    data_paths, load_pairings, load_peers, AppSettings, DataPaths, DesktopIdentity, EventStore,
    PairingFile, PeerFile,
};
use crate::sync::{
    run_peer_supervisor, run_server, BlobStore, DesktopClock, MdnsRegistration, OutboundEvent,
    OutboundTx, PeerSyncDeps, ProtectionKeyStore, ServerContext, SyncEvent,
};

/// Frontend-visible app state. Held by Tauri as a managed singleton.
pub struct AppState {
    pub paths: DataPaths,
    pub identity_device_id: String,
    pub events: Arc<EventStore>,
    pub pairings: Arc<RwLock<PairingFile>>,
    pub settings: Arc<RwLock<AppSettings>>,
    pub bus: broadcast::Sender<SyncEvent>,
    pub advertised_host: Arc<RwLock<String>>,
    pub clock: Arc<DesktopClock>,
    pub outbound: OutboundTx,
    pub blobs: Arc<BlobStore>,
    /// Phase 3: in-memory per-chat decryption keys, derived from the PIN on unlock.
    pub keystore: Arc<ProtectionKeyStore>,
    /// Queryable WS server status — closes the startup race where the webview
    /// attaches its `sync-event` listener after `ServerStarted` already fired.
    pub server_status: Arc<RwLock<&'static str>>,
    pub server_addr: Arc<RwLock<Option<String>>>,
    /// Holds the live mDNS-SD registration (or `None` when discovery is off).
    /// Dropping the inner value unregisters the service. Behind a `std::sync::Mutex`
    /// so the `discoverable` setting can start/stop advertising at runtime without
    /// a restart.
    pub mdns: std::sync::Mutex<Option<MdnsRegistration>>,
    /// Outbound peers this desktop connects to (the client role — e.g. another
    /// desktop). The supervisor task reconciles this list with live connections.
    pub peers: Arc<RwLock<PeerFile>>,
    /// Signalled after `peers` changes so the supervisor reconciles immediately.
    pub peer_notify: Arc<Notify>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    tauri::Builder::default()
        // Track size/position/maximized/fullscreen but NOT decorations — we
        // want `tauri.conf.json`'s `decorations: false` (frameless) to win, not
        // a stale "decorated" value the plugin saved from an earlier build.
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(
                    tauri_plugin_window_state::StateFlags::all()
                        & !tauri_plugin_window_state::StateFlags::DECORATIONS,
                )
                .build(),
        )
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        // Hide-to-tray on close: keeps the WebSocket server alive so the phone can
        // keep syncing while the user has no visible window. Right-click the tray
        // icon → Quit to actually exit.
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .setup(|app| {
            // Resolve a writable data dir. Tauri 2's path API gives one per-bundle.
            let data_root = app
                .path()
                .app_data_dir()
                .expect("resolve app data dir")
                .join("ncy");
            std::fs::create_dir_all(&data_root)?;
            let paths = data_paths(data_root);

            let settings = storage::settings::load(&paths.settings).unwrap_or_default();
            let identity = DesktopIdentity::load_or_create(&paths.identity, &settings.device_name)?;
            let pairings = load_pairings(&paths.pairings).unwrap_or_default();
            let peers = Arc::new(RwLock::new(load_peers(&paths.peers).unwrap_or_default()));
            let peer_notify = Arc::new(Notify::new());
            let events = Arc::new(EventStore::open(&paths.events)?);
            // Trim oversized event logs once per launch — silent no-op for small chats.
            if let Err(e) = storage::compaction::run_if_needed(&events) {
                tracing::warn!("compaction error: {e}");
            }
            let blobs = Arc::new(BlobStore::open(&paths.blobs)?);
            let (bus, _initial_rx) = broadcast::channel::<SyncEvent>(256);
            let (outbound, _initial_outbound_rx) = broadcast::channel::<OutboundEvent>(256);
            let advertised_host = network::best_lan_ip();

            // Seed the desktop's HLC from whatever's already on disk so we never
            // produce a clock that's behind a stored event.
            let initial_max_hlc = max_stored_hlc(&events);
            let clock = Arc::new(DesktopClock::new(
                identity.device_id.clone(),
                initial_max_hlc,
            ));

            // Opt-in mDNS announcement so phones can find this desktop on the LAN
            // without typing its IP. Off by default — on shared/large networks the
            // user may not want to broadcast the desktop's presence. The manual
            // QR/paste pairing path works regardless. Toggling `discoverable` in
            // settings starts/stops this at runtime (see `update_settings`).
            let mdns = if settings.discoverable {
                MdnsRegistration::announce(
                    &settings.device_name,
                    &advertised_host,
                    settings.port,
                    &identity.device_id,
                )
                .map_err(|e| {
                    tracing::warn!("mDNS announce failed: {e}");
                    e
                })
                .ok()
            } else {
                tracing::info!("mDNS advertising disabled (discoverable = false)");
                None
            };

            let server_status = Arc::new(RwLock::new("starting"));
            let server_addr = Arc::new(RwLock::new(None::<String>));

            let state = AppState {
                paths,
                identity_device_id: identity.device_id.clone(),
                events: events.clone(),
                pairings: Arc::new(RwLock::new(pairings)),
                settings: Arc::new(RwLock::new(settings.clone())),
                bus: bus.clone(),
                advertised_host: Arc::new(RwLock::new(advertised_host)),
                clock: clock.clone(),
                outbound: outbound.clone(),
                blobs: blobs.clone(),
                keystore: Arc::new(ProtectionKeyStore::new()),
                server_status: server_status.clone(),
                server_addr: server_addr.clone(),
                mdns: std::sync::Mutex::new(mdns),
                peers: peers.clone(),
                peer_notify: peer_notify.clone(),
            };

            let port = settings.port;
            let identity_name = settings.device_name.clone();
            let pairings_clone = state.pairings.clone();
            let events_clone = state.events.clone();
            let pairings_path = state.paths.pairings.clone();
            let bus_clone = bus.clone();
            let clock_for_server = clock.clone();
            let outbound_for_server = outbound.clone();
            let blobs_for_server = blobs.clone();
            let status_for_server = server_status.clone();
            let addr_for_server = server_addr.clone();
            let status_for_err = server_status.clone();

            // Spawn the WS server on the Tauri-managed Tokio runtime.
            tauri::async_runtime::spawn(async move {
                let ctx = Arc::new(ServerContext {
                    identity_device_id: identity.device_id,
                    identity_name,
                    pairings: pairings_clone,
                    pairings_path,
                    events: events_clone,
                    bus: bus_clone.clone(),
                    clock: clock_for_server,
                    outbound: outbound_for_server,
                    blobs: blobs_for_server,
                    server_status: status_for_server,
                    server_addr: addr_for_server,
                });
                let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));
                if let Err(e) = run_server(ctx, addr).await {
                    tracing::error!("sync server fatal: {e}");
                    *status_for_err.write().await = "error";
                    let _ = bus_clone.send(SyncEvent::ServerError {
                        message: e.to_string(),
                    });
                }
            });

            // Spawn the outbound-peer supervisor (desktop↔desktop client role).
            // It reconciles `peers.json` with live connections; `peer_notify`
            // wakes it when a peer is added/removed.
            let peer_deps = Arc::new(PeerSyncDeps {
                identity_device_id: state.identity_device_id.clone(),
                events: events.clone(),
                bus: bus.clone(),
                clock: clock.clone(),
                outbound: outbound.clone(),
                blobs: blobs.clone(),
                peers: peers.clone(),
                peers_path: state.paths.peers.clone(),
            });
            let peer_notify_for_sup = peer_notify.clone();
            tauri::async_runtime::spawn(async move {
                run_peer_supervisor(peer_deps, peer_notify_for_sup).await;
            });

            // Forward sync events to the webview.
            let app_handle = app.handle().clone();
            let rx = bus.subscribe();
            commands::spawn_sync_event_forwarder(app_handle, rx);

            app.manage(state);

            // System tray. Left-click toggles the main window; right-click opens a
            // small menu (Show / Quit). The menu is the only way to actually exit
            // — closing the window just hides it (see on_window_event above).
            let show_item = MenuItem::with_id(app, "tray_show", "Show Ncy", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "tray_quit", "Quit", true, None::<&str>)?;
            let tray_menu = Menu::with_items(app, &[&show_item, &quit_item])?;
            let icon = app
                .default_window_icon()
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("no window icon configured"))?;
            TrayIconBuilder::with_id("main")
                .tooltip("Ncy")
                .icon(icon)
                .menu(&tray_menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "tray_show" => show_main_window(app),
                    "tray_quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        toggle_main_window(tray.app_handle());
                    }
                })
                .build(app)?;

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_chats,
            commands::get_chat,
            commands::list_pairings,
            commands::create_pairing,
            commands::delete_pairing,
            commands::add_peer,
            commands::list_peers,
            commands::delete_peer,
            commands::get_settings,
            commands::update_settings,
            commands::list_lan_ips,
            commands::set_advertised_host,
            commands::get_advertised_host,
            commands::get_device_identity,
            commands::get_server_status,
            commands::author_send_text,
            commands::author_send_message,
            commands::author_toggle_star,
            commands::author_toggle_reaction,
            commands::author_edit_text,
            commands::author_delete_message,
            commands::author_send_image,
            commands::author_send_voice,
            commands::author_send_checklist,
            commands::author_set_reminder,
            commands::author_set_chat_avatar,
            commands::author_set_chat_background,
            commands::author_clear_chat_avatar,
            commands::author_clear_chat_background,
            commands::author_toggle_checklist_item,
            commands::author_create_chat,
            commands::author_update_chat_meta,
            commands::author_delete_chat,
            commands::read_blob,
            commands::export_backup,
            commands::import_backup,
            commands::unlock_chat,
            commands::lock_chat,
            commands::protect_chat,
            commands::change_chat_pin,
            commands::author_transcribe_voice,
            commands::transcription_status,
            commands::transcription_download_model,
            commands::transcription_delete_model,
            commands::transcription_open_folder,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Show + unminimize + focus the main window. Used by the tray menu and by the
/// left-click handler when the window is currently hidden.
fn show_main_window(app: &tauri::AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
    }
}

/// Left-click handler: if the window is visible and focused, hide it; otherwise
/// bring it to the front. Standard tray-app convention.
fn toggle_main_window(app: &tauri::AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let visible = win.is_visible().unwrap_or(false);
        let focused = win.is_focused().unwrap_or(false);
        if visible && focused {
            let _ = win.hide();
        } else {
            let _ = win.show();
            let _ = win.unminimize();
            let _ = win.set_focus();
        }
    }
}

/// Largest HLC across every chat's event log on disk. Used to seed the desktop clock
/// at startup so we never tick to a value that's already been used.
fn max_stored_hlc(events: &EventStore) -> Option<HlcTimestamp> {
    let chat_ids = match events.list_chat_ids() {
        Ok(ids) => ids,
        Err(_) => return None,
    };
    let mut best: Option<HlcTimestamp> = None;
    for cid in chat_ids {
        if let Ok(Some(h)) = events.max_hlc(&cid) {
            best = Some(match best {
                Some(cur) if cur >= h => cur,
                _ => h,
            });
        }
    }
    best
}
