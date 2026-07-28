//! Daemon bootstrap — the headless equivalent of `ncy-desktop`'s
//! `lib.rs::run()::setup`, minus the tray, the webview, and Tauri's managed
//! state. It wires the exact same `ncy_core` pieces the desktop uses: the event
//! store, the blob store, the HLC clock, the WebSocket sync server (server role),
//! the outbound-peer supervisor (client role), optional mDNS, and — new here —
//! the auto-transcription loop.

use std::sync::Arc;

use anyhow::{Context, Result};
use tokio::sync::{broadcast, Notify, RwLock};

use ncy_core::model::HlcTimestamp;
use ncy_core::network::best_lan_ip;
use ncy_core::storage::compaction;
use ncy_core::storage::whisper::WhisperPaths;
use ncy_core::storage::{
    data_paths, generate_secret_b64, load_pairings, load_peers, DesktopIdentity, EventStore,
    PairingFile,
};
use ncy_core::sync::{
    run_peer_supervisor, run_server, BlobStore, DesktopClock, MdnsRegistration, OutboundEvent,
    PairSecurityState, PairingRegistry, PearGateway, PearSidecar, PearSidecarStatus, PeerSyncDeps,
    ServerContext, SyncEvent,
};

use crate::config::Config;
use crate::pair;
use crate::transcribe::{self, TranscribeDeps};

/// Boot every subsystem and block until a shutdown signal (Ctrl-C / SIGTERM).
pub async fn run(config: Config) -> Result<()> {
    let paths = data_paths(config.data_dir.clone());
    std::fs::create_dir_all(&paths.root)
        .with_context(|| format!("create data dir {}", paths.root.display()))?;

    let identity = DesktopIdentity::load_or_create(&paths.identity, &config.device_name)?;
    tracing::info!(
        "identity: {} ({})",
        identity.name,
        &identity.device_id[..identity.device_id.len().min(8)]
    );

    let pairings: PairingFile = load_pairings(&paths.pairings).unwrap_or_default();
    tracing::info!("{} pairing(s) loaded", pairings.pairings.len());
    let pairings = Arc::new(RwLock::new(pairings));
    let peers = Arc::new(RwLock::new(load_peers(&paths.peers).unwrap_or_default()));
    let peer_notify = Arc::new(Notify::new());
    let transport_mode = Arc::new(RwLock::new(config.sync_transport_mode));
    let pear_gateway = Arc::new(RwLock::new(None));

    let events = Arc::new(EventStore::open(&paths.events)?);
    if let Err(e) = compaction::run_if_needed(&events) {
        tracing::warn!("compaction error: {e}");
    }
    let blobs = Arc::new(BlobStore::open(&paths.blobs)?);

    let (bus, _initial_rx) = broadcast::channel::<SyncEvent>(256);
    let (outbound, _initial_outbound_rx) = broadcast::channel::<OutboundEvent>(256);
    let (blob_requests, _) = broadcast::channel::<ncy_core::sync::ManualBlobRequest>(64);
    let automatic_file_download_limit_bytes = Arc::new(std::sync::atomic::AtomicU64::new(
        config
            .automatic_file_download_limit_mb
            .min(1024)
            .saturating_mul(1024 * 1024),
    ));

    // Seed the clock from the highest HLC already on disk so we never author an
    // event that's causally behind something we've stored.
    let clock = Arc::new(DesktopClock::new(
        identity.device_id.clone(),
        max_stored_hlc(&events),
    ));

    let advertised_host = best_lan_ip();
    let mdns = if config.discoverable {
        MdnsRegistration::announce(&advertised_host, config.port, &identity.device_id, "qr")
            .map_err(|e| tracing::warn!("mDNS announce failed: {e}"))
            .ok()
    } else {
        tracing::warn!(
            "mDNS advertising disabled: QR/JSON pairing still works, but paired clients cannot recover automatically after this server's LAN address changes"
        );
        None
    };

    let server_status = Arc::new(RwLock::new("starting"));
    let server_addr = Arc::new(RwLock::new(None::<String>));

    // Pear is an explicit opt-in. Its seed is installation-local and stable so
    // pairing payloads keep the same public key across daemon restarts.
    let pear_sidecar = if config.sync_transport_mode.uses_pear() {
        let (runtime, bundle) = config.pear_paths()?;
        let seed_path = paths.root.join("pear-seed.txt");
        let seed = load_or_create_pear_seed(&seed_path)?;
        let sidecar = PearSidecar::spawn(runtime, bundle, seed).await?;
        let public_key = wait_for_pear_ready(&sidecar).await?;
        std::fs::write(paths.root.join("pear-public-key.txt"), &public_key)?;
        *pear_gateway.write().await = Some(PearGateway::start(
            sidecar.clone(),
            format!("ws://127.0.0.1:{}/sync", config.port),
        ));
        tracing::info!(target: "ncy_pear", public_key, "Pear transport ready");
        Some(sidecar)
    } else {
        tracing::info!(target: "ncy_pear", "Pear transport disabled (localOnly)");
        None
    };

    // --- WebSocket sync server (server role: phones + inbound desktops dial in) ---
    {
        let ctx = Arc::new(ServerContext {
            identity_device_id: identity.device_id.clone(),
            identity_name: config.device_name.clone(),
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
            // Discovery-initiated pair requests park a oneshot here for a UI to
            // accept or reject. A daemon has no UI, so nothing ever resolves one
            // and `handle_pair_request` rejects on its 60s timeout. That is the
            // intended behaviour: pairing happens out-of-band via the
            // `ncy-server pair` subcommand, which writes `pairings.json` directly.
            pairing_registry: PairingRegistry::default(),
            pair_security: PairSecurityState::new(config.discoverable),
            active_peers: Default::default(),
        });
        let addr = ncy_core::network::wildcard_bind_addr(&advertised_host, config.port);
        let bus_for_err = bus.clone();
        let status_for_err = server_status.clone();
        tokio::spawn(async move {
            if let Err(e) = run_server(ctx, addr).await {
                tracing::error!("sync server fatal: {e}");
                *status_for_err.write().await = "error";
                let _ = bus_for_err.send(SyncEvent::ServerError {
                    message: e.to_string(),
                });
            }
        });
        tracing::info!("sync server listening on 0.0.0.0:{}", config.port);
    }

    // Pairing administration may happen in this terminal or through a second
    // short-lived CLI process. Keep the daemon's authentication registry in
    // sync with valid on-disk changes without requiring a restart.
    tokio::spawn(watch_pairings(paths.pairings.clone(), pairings.clone()));

    // --- Outbound-peer supervisor (client role: dial other desktops/servers) ---
    {
        let peer_deps = Arc::new(PeerSyncDeps {
            identity_device_id: identity.device_id.clone(),
            identity_name: config.device_name.clone(),
            events: events.clone(),
            bus: bus.clone(),
            clock: clock.clone(),
            outbound: outbound.clone(),
            blobs: blobs.clone(),
            automatic_file_download_limit_bytes: automatic_file_download_limit_bytes.clone(),
            blob_requests: blob_requests.clone(),
            peers: peers.clone(),
            peers_path: paths.peers.clone(),
            transport_mode,
            pear_gateway,
        });
        let peer_notify_for_sup = peer_notify.clone();
        tokio::spawn(async move {
            run_peer_supervisor(peer_deps, peer_notify_for_sup).await;
        });
    }

    // --- Auto-transcription loop ---
    let whisper = WhisperPaths::from_dir(config.whisper_dir());
    if config.transcription_enabled {
        if whisper.cli_present() && whisper.model_present() {
            let deps = Arc::new(TranscribeDeps {
                device_id: identity.device_id.clone(),
                events: events.clone(),
                blobs: blobs.clone(),
                clock: clock.clone(),
                outbound: outbound.clone(),
                bus: bus.clone(),
                whisper: whisper.clone(),
            });
            tokio::spawn(transcribe::run_transcription_loop(deps));
            tracing::info!(
                "auto-transcription enabled (whisper dir {})",
                whisper.dir.display()
            );
        } else {
            tracing::warn!(
                "transcription_enabled = true but whisper-cli/model missing in {} — running as a pure relay (no transcripts). Install whisper-cli + ggml-tiny.bin there and restart.",
                whisper.dir.display()
            );
        }
    } else {
        tracing::info!("auto-transcription disabled (transcription_enabled = false)");
    }

    tracing::info!("ncy-server up. Type 'help' for commands; Ctrl-C to stop.");
    wait_for_shutdown(&config, pairings, &paths.pairings, blob_requests).await;
    tracing::info!("shutting down");
    if let Some(sidecar) = pear_sidecar {
        sidecar.shutdown().await;
    }
    drop(mdns); // unregister the mDNS service explicitly before exit
    Ok(())
}

fn load_or_create_pear_seed(path: &std::path::Path) -> Result<String> {
    if let Ok(seed) = std::fs::read_to_string(path) {
        let seed = seed.trim();
        if !seed.is_empty() {
            return Ok(seed.to_owned());
        }
    }
    let seed = generate_secret_b64();
    write_private_seed(path, seed.as_bytes())?;
    Ok(seed)
}

fn write_private_seed(path: &std::path::Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

async fn wait_for_pear_ready(sidecar: &Arc<PearSidecar>) -> Result<String> {
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
    match ready {
        Ok(public_key) => Ok(public_key),
        Err(error) => {
            sidecar.shutdown().await;
            Err(error)
        }
    }
}

/// Largest HLC across every chat log on disk — seeds the clock so a fresh process
/// can't tick to a value older than something already stored. Mirrors the helper
/// of the same name in the desktop's `lib.rs`.
fn max_stored_hlc(events: &EventStore) -> Option<HlcTimestamp> {
    let chat_ids = events.list_chat_ids().ok()?;
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

/// Resolve when the process receives Ctrl-C or (on unix) SIGTERM — the signal
/// systemd sends on `stop`.
async fn wait_for_shutdown(
    config: &Config,
    pairings: Arc<RwLock<PairingFile>>,
    pairings_path: &std::path::Path,
    blob_requests: broadcast::Sender<ncy_core::sync::ManualBlobRequest>,
) {
    tokio::select! {
        _ = wait_for_os_shutdown() => {}
        _ = console_loop(config, pairings, pairings_path, blob_requests) => {}
    }
}

async fn wait_for_os_shutdown() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut term = match signal(SignalKind::terminate()) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("cannot install SIGTERM handler: {e}");
                let _ = tokio::signal::ctrl_c().await;
                return;
            }
        };
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = term.recv() => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

async fn watch_pairings(path: std::path::PathBuf, pairings: Arc<RwLock<PairingFile>>) {
    let mut previous = std::fs::read(&path).ok();
    let mut interval = tokio::time::interval(std::time::Duration::from_millis(500));
    loop {
        interval.tick().await;
        let current = match std::fs::read(&path) {
            Ok(bytes) => Some(bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => {
                tracing::warn!("could not check pairing updates: {error}");
                continue;
            }
        };
        if current == previous {
            continue;
        }
        let Some(bytes) = current.as_deref() else {
            // A transient replace/delete must not revoke every device. Keep the
            // last valid registry and wait for a valid file to appear.
            continue;
        };
        match serde_json::from_slice::<PairingFile>(bytes) {
            Ok(updated) => {
                let count = updated.pairings.len();
                *pairings.write().await = updated;
                previous = current;
                tracing::info!("pairing registry reloaded ({count} pairing(s))");
            }
            Err(error) => {
                tracing::warn!("ignored invalid pairing file update: {error}");
            }
        }
    }
}

async fn reload_pairings(
    path: &std::path::Path,
    pairings: &Arc<RwLock<PairingFile>>,
) -> Result<()> {
    let updated = load_pairings(path)?;
    let count = updated.pairings.len();
    *pairings.write().await = updated;
    tracing::info!("pairing registry reloaded ({count} pairing(s))");
    Ok(())
}

async fn console_loop(
    config: &Config,
    pairings: Arc<RwLock<PairingFile>>,
    pairings_path: &std::path::Path,
    blob_requests: broadcast::Sender<ncy_core::sync::ManualBlobRequest>,
) {
    use tokio::io::{AsyncBufReadExt, BufReader};

    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    loop {
        let line = match lines.next_line().await {
            Ok(Some(line)) => line,
            Ok(None) => std::future::pending::<String>().await,
            Err(error) => {
                tracing::warn!("interactive console unavailable: {error}");
                std::future::pending::<String>().await
            }
        };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let (command, argument) = line
            .split_once(char::is_whitespace)
            .map_or((line, ""), |(command, argument)| (command, argument.trim()));
        let result = match command.to_ascii_lowercase().as_str() {
            "pair" => {
                match pair::run(config, (!argument.is_empty()).then(|| argument.to_owned())) {
                    Ok(()) => reload_pairings(pairings_path, &pairings).await,
                    Err(error) => Err(error),
                }
            }
            "pairings" => pair::list(config),
            "peers" => pair::list_peers(config),
            "download" if !argument.is_empty() => {
                if argument.len() != 64
                    || !argument
                        .bytes()
                        .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
                {
                    Err(anyhow::anyhow!(
                        "usage: download <64-character-lowercase-hash>"
                    ))
                } else {
                    blob_requests
                        .send(ncy_core::sync::ManualBlobRequest {
                            hash: argument.to_owned(),
                            media_type: "file".to_owned(),
                            cancel: false,
                        })
                        .map(|_| ())
                        .map_err(|_| anyhow::anyhow!("no connected device can provide the file"))
                }
            }
            "replace" | "rotate" if !argument.is_empty() => {
                match pair::replace_invitation(config, argument) {
                    Ok(()) => reload_pairings(pairings_path, &pairings).await,
                    Err(error) => Err(error),
                }
            }
            "revoke" if !argument.is_empty() => match pair::revoke(config, argument) {
                Ok(()) => reload_pairings(pairings_path, &pairings).await,
                Err(error) => Err(error),
            },
            "help" | "?" => {
                print_console_help();
                Ok(())
            }
            "stop" | "quit" | "exit" => return,
            "replace" | "rotate" => Err(anyhow::anyhow!("usage: replace <pairing-id>")),
            "revoke" => Err(anyhow::anyhow!("usage: revoke <pairing-id>")),
            "download" => Err(anyhow::anyhow!("usage: download <file-hash>")),
            _ => Err(anyhow::anyhow!("unknown command '{command}'; type 'help'")),
        };
        if let Err(error) = result {
            eprintln!("error: {error:#}");
        }
    }
}

fn print_console_help() {
    println!("Server commands:");
    println!("  pair [name]       Create and activate a pairing, then print JSON and QR");
    println!("  pairings          List pairing slots without secrets");
    println!("  peers             List outbound peer diagnostics");
    println!("  download <hash>   Download a deferred file from a connected device");
    println!("  replace <id>      Replace an unused pairing invitation");
    println!("  revoke <id>       Revoke and remove a pairing credential");
    println!("  stop              Shut down the server");
}
