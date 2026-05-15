pub mod commands;
pub mod model;
pub mod network;
pub mod storage;
pub mod sync;

use std::sync::Arc;

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Manager, WindowEvent};
use tokio::sync::{broadcast, RwLock};

use crate::model::HlcTimestamp;
use crate::storage::{
    data_paths, load_pairings, AppSettings, DataPaths, DesktopIdentity, EventStore, PairingFile,
};
use crate::sync::{
    run_server, BlobStore, DesktopClock, MdnsRegistration, OutboundEvent, OutboundTx,
    ProtectionKeyStore, ServerContext, SyncEvent,
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
    /// Holds the mDNS-SD registration alive for the lifetime of the app. Drop unregisters.
    pub _mdns: Option<MdnsRegistration>,
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
        .plugin(tauri_plugin_window_state::Builder::default().build())
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

            // Best-effort mDNS announcement so phones can find this desktop on the LAN
            // without typing its IP. Failures are non-fatal — the manual paste path
            // still works.
            let mdns = MdnsRegistration::announce(
                &settings.device_name,
                &advertised_host,
                settings.port,
                &identity.device_id,
            )
            .map_err(|e| {
                tracing::warn!("mDNS announce failed: {e}");
                e
            })
            .ok();

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
                _mdns: mdns,
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
                });
                let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));
                if let Err(e) = run_server(ctx, addr).await {
                    tracing::error!("sync server fatal: {e}");
                    let _ = bus_clone.send(SyncEvent::ServerError {
                        message: e.to_string(),
                    });
                }
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
            commands::get_settings,
            commands::update_settings,
            commands::list_lan_ips,
            commands::set_advertised_host,
            commands::get_advertised_host,
            commands::get_device_identity,
            commands::author_send_text,
            commands::author_send_message,
            commands::author_toggle_star,
            commands::author_edit_text,
            commands::author_delete_message,
            commands::author_send_image,
            commands::author_send_checklist,
            commands::read_blob,
            commands::unlock_chat,
            commands::lock_chat,
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
