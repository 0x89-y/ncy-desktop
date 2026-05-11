pub mod commands;
pub mod model;
pub mod network;
pub mod storage;
pub mod sync;

use std::sync::Arc;

use tauri::Manager;
use tokio::sync::{broadcast, RwLock};

use crate::model::HlcTimestamp;
use crate::storage::{
    data_paths, load_pairings, AppSettings, DataPaths, DesktopIdentity, EventStore, PairingFile,
};
use crate::sync::{
    run_server, BlobStore, DesktopClock, MdnsRegistration, OutboundEvent, OutboundTx,
    ServerContext, SyncEvent,
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
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
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
            commands::author_toggle_star,
            commands::author_edit_text,
            commands::author_delete_message,
            commands::read_blob,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
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
