pub mod cli;
pub mod commands;

// The core modules now live in the shared `ncy-core` crate. Re-export them under
// this crate's root so the existing `crate::model::…` / `crate::storage::…` /
// `crate::sync::…` / `crate::network::…` paths throughout `commands.rs` keep
// resolving unchanged.
pub use ncy_core::{model, network, storage, sync};

use std::path::PathBuf;
use std::sync::{Arc, RwLock as StdRwLock};

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Emitter, Manager, RunEvent, WindowEvent};
use tokio::sync::{broadcast, Notify, RwLock};

use crate::model::HlcTimestamp;
use crate::storage::{
    data_paths, load_pairings, load_peers, AppSettings, DataPaths, DesktopIdentity, EventStore,
    PairingFile, PeerFile,
};
use crate::sync::{
    run_peer_supervisor_with_shutdown, run_server_with_shutdown, BlobStore, DesktopClock,
    MdnsRegistration, OutboundEvent, OutboundTx, PairSecurityState, PairingRegistry, PearGateway,
    PearSidecar, PearSidecarStatus, PeerSyncDeps, ProtectionKeyStore, ServerContext, SyncEvent,
    SyncTransportMode,
};
use commands::DesktopPrefs;

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
    pub automatic_file_download_limit_bytes: Arc<std::sync::atomic::AtomicU64>,
    pub blob_requests: broadcast::Sender<crate::sync::ManualBlobRequest>,
    /// True while a debounced media sweep is queued — see `commands::schedule_blob_gc`.
    pub blob_gc_pending: Arc<std::sync::atomic::AtomicBool>,
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
    /// Pending discovery-initiated pair requests. Shared with the sync
    /// `ServerContext` so the connection handler can register a session and
    /// the `pair_accept` / `pair_reject` tauri commands can look it up.
    pub pairing_registry: crate::sync::PairingRegistry,
    /// Rate-limit state + audit log for pair attempts. Shared with the sync
    /// server; the command layer reads it for `list_pair_attempts`.
    pub pair_security: Arc<PairSecurityState>,
    /// Desktop-only UI/runtime preferences (tray behavior, startup behavior).
    pub desktop_prefs: Arc<StdRwLock<DesktopPrefs>>,
    /// Everything the WS listener needs, retained so the `syncServerEnabled`
    /// toggle can respawn it after a stop without rebuilding the world.
    pub server_ctx: Arc<ServerContext>,
    /// Ditto for the outbound peer supervisor.
    pub peer_deps: Arc<PeerSyncDeps>,
    pub transport_mode: Arc<RwLock<SyncTransportMode>>,
    pub pear_gateway: Arc<RwLock<Option<Arc<PearGateway>>>>,
    pub pear_sidecar: Arc<RwLock<Option<Arc<PearSidecar>>>>,
    pub pear_public_key: Arc<RwLock<Option<String>>>,
    pub pear_runtime_path: PathBuf,
    pub pear_bundle_path: PathBuf,
    pub pear_seed_path: PathBuf,
    /// `Some` exactly while the sync stack is running. Doubles as the "is sync
    /// up?" flag — see [`apply_sync_enabled`].
    pub sync_shutdown: std::sync::Mutex<Option<SyncHandle>>,
}

/// Stop button for a running sync stack, plus the tasks to wait on afterwards.
/// This is the sole owner of the shutdown sender, so dropping it stops the stack
/// just as firing it does.
pub struct SyncHandle {
    shutdown: broadcast::Sender<()>,
    server: tauri::async_runtime::JoinHandle<()>,
    peers: tauri::async_runtime::JoinHandle<()>,
}

/// Spawn the WS listener and the outbound peer supervisor, returning the handle
/// that stops both. Callers park it in [`AppState::sync_shutdown`].
pub fn spawn_sync(state: &AppState, port: u16) -> SyncHandle {
    let (shutdown, _) = broadcast::channel(1);

    let ctx = state.server_ctx.clone();
    let status_for_err = state.server_status.clone();
    let bus_for_err = state.bus.clone();
    let server_shutdown = shutdown.subscribe();
    let server = tauri::async_runtime::spawn(async move {
        let advertised_host = network::best_lan_ip();
        let addr = network::wildcard_bind_addr(&advertised_host, port);
        if let Err(e) = run_server_with_shutdown(ctx, addr, server_shutdown).await {
            tracing::error!("sync server fatal: {e}");
            *status_for_err.write().await = "error";
            let _ = bus_for_err.send(SyncEvent::ServerError {
                message: e.to_string(),
            });
        }
    });

    let deps = state.peer_deps.clone();
    let notify = state.peer_notify.clone();
    let peer_shutdown = shutdown.subscribe();
    let peers = tauri::async_runtime::spawn(async move {
        run_peer_supervisor_with_shutdown(deps, notify, peer_shutdown).await;
    });

    SyncHandle {
        shutdown,
        server,
        peers,
    }
}

/// Reconcile the whole sync stack — listener, live connections, outbound peers,
/// and mDNS advertising — with `enabled`. Idempotent, and effective immediately
/// rather than "next restart".
pub async fn apply_sync_enabled(state: &AppState, enabled: bool) {
    // Scoped so the std Mutex guard never crosses an await.
    let running = state.sync_shutdown.lock().unwrap().is_some();
    if running == enabled {
        return;
    }

    if enabled {
        let settings = state.settings.read().await.clone();
        if let Err(error) = apply_pear_mode(state, settings.sync_transport_mode).await {
            tracing::error!(target: "ncy_pear", "start Pear transport: {error}");
        }
        *state.server_status.write().await = "starting";
        let handle = spawn_sync(state, settings.port);
        *state.sync_shutdown.lock().unwrap() = Some(handle);

        // The listener hasn't bound yet, so advertise the configured port.
        if settings.discoverable {
            announce_mdns(state, settings.port).await;
        }
    } else {
        stop_pear(state).await;
        let handle = state.sync_shutdown.lock().unwrap().take();
        if let Some(handle) = handle {
            let _ = handle.shutdown.send(());
            // Wait for the listener to actually drop its socket. Without this, a
            // quick off→on toggle would race the old bind and fail with
            // "address already in use".
            let _ = handle.server.await;
            let _ = handle.peers.await;
        }
        // Nothing to reach: stop advertising too. `run_with_shutdown` publishes
        // the "stopped" status and the ServerStopped event on its way out.
        *state.mdns.lock().unwrap() = None;
    }
}

fn load_or_create_pear_seed(path: &std::path::Path) -> anyhow::Result<String> {
    if let Ok(seed) = std::fs::read_to_string(path) {
        let seed = seed.trim();
        if !seed.is_empty() {
            return Ok(seed.to_owned());
        }
    }
    let seed = storage::generate_secret_b64();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, &seed)?;
    Ok(seed)
}

async fn stop_pear(state: &AppState) {
    *state.pear_gateway.write().await = None;
    *state.pear_public_key.write().await = None;
    if let Some(sidecar) = state.pear_sidecar.write().await.take() {
        sidecar.shutdown().await;
    }
}

/// Apply the routing policy immediately. Local-only is a hard privacy boundary:
/// no Pear process remains alive after this function returns.
pub async fn apply_pear_mode(state: &AppState, mode: SyncTransportMode) -> anyhow::Result<()> {
    *state.transport_mode.write().await = mode;
    if !mode.uses_pear() {
        stop_pear(state).await;
        state.peer_notify.notify_waiters();
        return Ok(());
    }
    if state.pear_sidecar.read().await.is_some() {
        state.peer_notify.notify_waiters();
        return Ok(());
    }

    let seed = load_or_create_pear_seed(&state.pear_seed_path)?;
    let sidecar =
        PearSidecar::spawn(&state.pear_runtime_path, &state.pear_bundle_path, seed).await?;
    let mut status = sidecar.status();
    let ready = tokio::time::timeout(std::time::Duration::from_secs(20), async {
        loop {
            let current = status.borrow().clone();
            match current {
                PearSidecarStatus::Ready { public_key } => break Ok(public_key),
                PearSidecarStatus::Failed { message } => anyhow::bail!(message),
                PearSidecarStatus::Stopped => anyhow::bail!("Pear worker stopped during startup"),
                PearSidecarStatus::Starting => status.changed().await?,
            }
        }
    })
    .await
    .map_err(|_| anyhow::anyhow!("Pear worker startup timed out"))
    .and_then(|result| result);
    let public_key = match ready {
        Ok(public_key) => public_key,
        Err(error) => {
            sidecar.shutdown().await;
            return Err(error);
        }
    };
    let port = state.settings.read().await.port;
    let gateway = PearGateway::start(sidecar.clone(), format!("ws://127.0.0.1:{port}/sync"));
    *state.pear_public_key.write().await = Some(public_key);
    *state.pear_gateway.write().await = Some(gateway);
    *state.pear_sidecar.write().await = Some(sidecar);
    state.peer_notify.notify_waiters();
    Ok(())
}

/// (Re-)announce this desktop over mDNS-SD using a privacy label derived from its
/// random installation ID, never its human device name. Failures are logged, not fatal — manual
/// QR/paste pairing works without discovery.
pub async fn announce_mdns(state: &AppState, port: u16) {
    let host = state.advertised_host.read().await.clone();
    let mut guard = state.mdns.lock().unwrap();
    match MdnsRegistration::announce(&host, port, &state.identity_device_id, "request") {
        Ok(reg) => *guard = Some(reg),
        Err(e) => {
            tracing::warn!("mDNS announce failed: {e}");
            *guard = None;
        }
    }
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
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        // Close (✕) behavior, in precedence order:
        //   close_to_tray     → hide; sync keeps running with no visible window
        //   ask_close_to_tray → hold the close and let the webview ask (see
        //                       CloseToTrayPrompt.svelte), which then hides or quits
        //   neither           → let it through, quitting the app
        // Right-click the tray icon → Quit always exits, whatever the prefs say.
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    let prefs = window.try_state::<AppState>().and_then(|state| {
                        state
                            .desktop_prefs
                            .read()
                            .ok()
                            .map(|p| (p.close_to_tray, p.ask_close_to_tray))
                    });
                    // No state yet (close during startup): ask rather than guess.
                    let (close_to_tray, ask_close_to_tray) = prefs.unwrap_or((false, true));
                    if close_to_tray {
                        api.prevent_close();
                        let _ = window.hide();
                    } else if ask_close_to_tray {
                        api.prevent_close();
                        let _ = window.emit("close-requested", ());
                    }
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
            let pear_seed_path = paths.identity.with_file_name("pear-seed.txt");
            let dev_sidecar =
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../ncy-core/pear-sidecar");
            let resource_sidecar = app.path().resource_dir()?.join("pear-sidecar");
            // Development must use the source sidecar directly. After moving to
            // a workspace, `target/debug/pear-sidecar` can exist as a copied
            // resource and Tauri may return it with a Windows verbatim path that
            // Bare cannot convert to a file URL. Packaged builds use resources.
            let sidecar_root = if cfg!(debug_assertions) && dev_sidecar.exists() {
                dev_sidecar
            } else if resource_sidecar.exists() {
                resource_sidecar
            } else {
                dev_sidecar
            };
            let pear_runtime_path =
                sidecar_root.join("node_modules/bare-runtime-win32-x64/bin/bare.exe");
            let pear_bundle_path = sidecar_root.join("out/ncy-pear-sidecar.bundle");

            let settings = storage::settings::load(&paths.settings).unwrap_or_default();
            let desktop_prefs = commands::load_desktop_prefs(&paths.settings);
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
            let (blob_requests, _) = broadcast::channel::<crate::sync::ManualBlobRequest>(64);
            let automatic_file_download_limit_bytes = Arc::new(std::sync::atomic::AtomicU64::new(
                settings.automatic_file_download_limit_bytes,
            ));
            let advertised_host = network::best_lan_ip();

            // Seed the desktop's HLC from whatever's already on disk so we never
            // produce a clock that's behind a stored event.
            let initial_max_hlc = max_stored_hlc(&events);
            let clock = Arc::new(DesktopClock::new(
                identity.device_id.clone(),
                initial_max_hlc,
            ));

            // The whole LAN sync stack is opt-in (`syncServerEnabled`). When it's
            // off nothing binds, advertises, or dials out — Ncy runs standalone.
            let sync_enabled = desktop_prefs.sync_server_enabled;

            // Opt-in mDNS announcement so phones can find this desktop on the LAN
            // without typing its IP. Off by default — on shared/large networks the
            // user may not want to broadcast the desktop's presence. The manual
            // QR/paste pairing path works regardless. Toggling `discoverable` in
            // settings starts/stops this at runtime (see `update_settings`).
            let mdns = if sync_enabled && settings.discoverable {
                MdnsRegistration::announce(
                    &advertised_host,
                    settings.port,
                    &identity.device_id,
                    "request",
                )
                .map_err(|e| {
                    tracing::warn!("mDNS announce failed: {e}");
                    e
                })
                .ok()
            } else {
                tracing::info!("mDNS advertising disabled (discoverable = false, or sync off)");
                None
            };

            // A disabled stack never reports "starting" — it goes straight to the
            // terminal state the frontend renders as a dormant status dot.
            let server_status = Arc::new(RwLock::new(if sync_enabled {
                "starting"
            } else {
                "stopped"
            }));
            let server_addr = Arc::new(RwLock::new(None::<String>));
            let pairing_registry: PairingRegistry =
                Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new()));
            let pair_security = PairSecurityState::new(settings.discoverable);
            let pairings = Arc::new(RwLock::new(pairings));
            let transport_mode = Arc::new(RwLock::new(settings.sync_transport_mode));
            let pear_gateway = Arc::new(RwLock::new(None));
            let pear_sidecar = Arc::new(RwLock::new(None));
            let pear_public_key = Arc::new(RwLock::new(None));

            // Built once and retained in `AppState`, so flipping `syncServerEnabled`
            // back on can respawn the tasks without reopening stores or rebinding
            // identity.
            let server_ctx = Arc::new(ServerContext {
                identity_device_id: identity.device_id.clone(),
                identity_name: settings.device_name.clone(),
                pairings: pairings.clone(),
                pairings_path: paths.pairings.clone(),
                events: events.clone(),
                bus: bus.clone(),
                clock: clock.clone(),
                outbound: outbound.clone(),
                blobs: blobs.clone(),
                automatic_file_download_limit_bytes: automatic_file_download_limit_bytes.clone(),
                blob_requests: blob_requests.clone(),
                server_status: server_status.clone(),
                server_addr: server_addr.clone(),
                pairing_registry: pairing_registry.clone(),
                pair_security: pair_security.clone(),
                active_peers: Default::default(),
            });

            // The outbound-peer supervisor's deps (desktop↔desktop client role).
            // It reconciles `peers.json` with live connections; `peer_notify`
            // wakes it when a peer is added/removed.
            let peer_deps = Arc::new(PeerSyncDeps {
                identity_device_id: identity.device_id.clone(),
                identity_name: settings.device_name.clone(),
                events: events.clone(),
                bus: bus.clone(),
                clock: clock.clone(),
                outbound: outbound.clone(),
                blobs: blobs.clone(),
                automatic_file_download_limit_bytes: automatic_file_download_limit_bytes.clone(),
                blob_requests: blob_requests.clone(),
                peers: peers.clone(),
                peers_path: paths.peers.clone(),
                transport_mode: transport_mode.clone(),
                pear_gateway: pear_gateway.clone(),
            });

            let port = settings.port;
            let state = AppState {
                paths,
                identity_device_id: identity.device_id.clone(),
                events: events.clone(),
                pairings,
                settings: Arc::new(RwLock::new(settings.clone())),
                bus: bus.clone(),
                advertised_host: Arc::new(RwLock::new(advertised_host)),
                clock: clock.clone(),
                outbound: outbound.clone(),
                blobs: blobs.clone(),
                automatic_file_download_limit_bytes,
                blob_requests,
                blob_gc_pending: Arc::new(std::sync::atomic::AtomicBool::new(false)),
                keystore: Arc::new(ProtectionKeyStore::new()),
                server_status: server_status.clone(),
                server_addr: server_addr.clone(),
                mdns: std::sync::Mutex::new(mdns),
                peers: peers.clone(),
                peer_notify: peer_notify.clone(),
                pairing_registry: pairing_registry.clone(),
                pair_security: pair_security.clone(),
                desktop_prefs: Arc::new(StdRwLock::new(desktop_prefs.clone())),
                server_ctx,
                peer_deps,
                transport_mode,
                pear_gateway,
                pear_sidecar,
                pear_public_key,
                pear_runtime_path,
                pear_bundle_path,
                pear_seed_path,
                sync_shutdown: std::sync::Mutex::new(None),
            };

            // Spawn the WS server + peer supervisor on the Tauri-managed Tokio
            // runtime. `update_desktop_prefs` can stop and restart them later.
            if sync_enabled {
                if let Err(error) = tauri::async_runtime::block_on(apply_pear_mode(
                    &state,
                    settings.sync_transport_mode,
                )) {
                    tracing::error!(target: "ncy_pear", "start Pear transport: {error}");
                }
                let handle = spawn_sync(&state, port);
                *state.sync_shutdown.lock().unwrap() = Some(handle);
            } else {
                tracing::info!("sync stack disabled (syncServerEnabled = false)");
            }

            // Forward sync events to the webview.
            let app_handle = app.handle().clone();
            let rx = bus.subscribe();
            commands::spawn_sync_event_forwarder(app_handle, rx);

            app.manage(state);

            // System tray. Left-click toggles the main window; right-click opens a
            // small menu (Show / Quit). Quit always exits, even while the close
            // button is set to hide to the tray (see on_window_event above).
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

            if desktop_prefs.start_minimized {
                if let Some(win) = app.get_webview_window("main") {
                    let _ = win.hide();
                }
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_chats,
            commands::get_chat,
            commands::list_pairings,
            commands::create_pairing,
            commands::delete_pairing,
            commands::replace_pairing,
            commands::add_peer,
            commands::list_peers,
            commands::set_peer_enabled,
            commands::delete_peer,
            commands::get_settings,
            commands::update_settings,
            commands::get_desktop_prefs,
            commands::update_desktop_prefs,
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
            commands::author_send_file,
            commands::author_send_voice,
            commands::author_send_checklist,
            commands::author_set_reminder,
            commands::author_set_chat_avatar,
            commands::author_set_chat_background,
            commands::author_clear_chat_avatar,
            commands::author_clear_chat_background,
            commands::author_toggle_checklist_item,
            commands::author_toggle_text_checklist_block,
            commands::author_create_chat,
            commands::author_update_chat_meta,
            commands::author_delete_chat,
            commands::read_blob,
            commands::blob_exists,
            commands::download_blob_anyway,
            commands::cancel_blob_download,
            commands::collect_unused_blobs,
            commands::evict_blob,
            commands::blob_evicted,
            commands::apply_chat_retention,
            commands::force_full_resync,
            commands::scrub_deleted_message_content,
            commands::save_blob,
            commands::image_view,
            commands::save_image,
            commands::copy_image,
            commands::export_backup,
            commands::run_automatic_backup,
            commands::import_backup,
            commands::purge_chat_tombstones,
            commands::unlock_chat,
            commands::lock_chat,
            commands::protect_chat,
            commands::change_chat_pin,
            commands::author_transcribe_voice,
            commands::transcription_status,
            commands::transcription_setup,
            commands::transcription_select_model,
            commands::transcription_remove_unused_models,
            commands::transcription_download_model,
            commands::transcription_delete_model,
            commands::transcription_open_folder,
            commands::pair_accept,
            commands::pair_reject,
            commands::list_pair_attempts,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if let RunEvent::ExitRequested { .. } = event {
                if let Some(state) = app.try_state::<AppState>() {
                    // Window close (when not kept in the tray), tray Quit, and
                    // programmatic exits all converge here. The Bare reader
                    // tasks retain an Arc<PearSidecar>, so relying on
                    // Child::kill_on_drop alone can orphan bare.exe.
                    tauri::async_runtime::block_on(async {
                        apply_sync_enabled(&state, false).await;
                        // Also cover partial startup: Pear may have reached
                        // Ready before the sync stack installed its handle.
                        stop_pear(&state).await;
                    });
                }
            }
        });
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
