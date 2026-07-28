use std::path::PathBuf;
use std::sync::{Arc, RwLock as StdRwLock};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use tokio::sync::{broadcast, Notify, RwLock};

use crate::commands::{self, PairingPayload};
use crate::storage::{
    self, data_paths, load_pairings, load_peers, save_pairings, save_peers, DesktopIdentity,
    EventStore, OutboundPeer, Pairing,
};
use crate::sync::pear::valid_public_key;
use crate::sync::{
    BlobStore, DesktopClock, OutboundEvent, PairSecurityState, PairingRegistry, PearGateway,
    PearSidecar, PeerSyncDeps, ProtectionKeyStore, ServerContext, SyncEvent,
};
use crate::{apply_sync_enabled, max_stored_hlc, network, AppState};

const HELP: &str = r#"Ncy headless client

Usage:
  ncy-desktop client run
  ncy-desktop --no-gui
  ncy-desktop client status
  ncy-desktop client peers
  ncy-desktop client sync [--seconds <n>]
  ncy-desktop client pair create [--name <label>]
  ncy-desktop client pair add --file <pairing.json>
  ncy-desktop client pair add --json <pairing-json>

The CLI uses the same profile, identity, settings, and peers as Ncy Desktop.
Do not run the GUI and a headless client against that profile simultaneously."#;

pub fn run(args: &[String]) -> Result<()> {
    // main passes the full argv vector; skip the executable name.
    let args = &args[1..];
    let command = args.first().map(String::as_str).unwrap_or("run");
    if matches!(command, "help" | "--help" | "-h") {
        println!("{HELP}");
        return Ok(());
    }

    init_tracing();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("create async runtime")?;
    runtime.block_on(run_async(command, &args[1..]))
}

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .try_init();
}

async fn run_async(command: &str, args: &[String]) -> Result<()> {
    let state = build_state()?;
    match command {
        "run" => run_foreground(&state).await,
        "sync" => sync_once(&state, parse_seconds(args)?).await,
        "status" => print_status(&state).await,
        "peers" => print_peers(&state).await,
        "pair" => pair_command(&state, args).await,
        other => bail!("unknown client command '{other}'\n\n{HELP}"),
    }
}

#[allow(clippy::needless_return)] // early returns keep the cfg-specific branches readable
fn data_root() -> Result<PathBuf> {
    if let Some(root) = std::env::var_os("NCY_DATA_DIR") {
        return Ok(PathBuf::from(root));
    }
    #[cfg(target_os = "windows")]
    {
        let app_data = std::env::var_os("APPDATA").context("APPDATA is not set")?;
        return Ok(PathBuf::from(app_data).join("com.ncy.desktop").join("ncy"));
    }
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var_os("HOME").context("HOME is not set")?;
        return Ok(PathBuf::from(home).join("Library/Application Support/com.ncy.desktop/ncy"));
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Some(root) = std::env::var_os("XDG_DATA_HOME") {
            return Ok(PathBuf::from(root).join("com.ncy.desktop/ncy"));
        }
        let home = std::env::var_os("HOME").context("HOME is not set")?;
        Ok(PathBuf::from(home).join(".local/share/com.ncy.desktop/ncy"))
    }
}

fn sidecar_root() -> Result<PathBuf> {
    let development = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../ncy-core/pear-sidecar");
    if development.exists() {
        return Ok(development);
    }
    let executable = std::env::current_exe().context("locate Ncy executable")?;
    Ok(executable
        .parent()
        .context("Ncy executable has no parent directory")?
        .join("pear-sidecar"))
}

fn build_state() -> Result<AppState> {
    let root = data_root()?;
    std::fs::create_dir_all(&root)?;
    let paths = data_paths(root);
    let sidecar = sidecar_root()?;
    let pear_runtime_path = if cfg!(target_os = "windows") {
        sidecar.join("node_modules/bare-runtime-win32-x64/bin/bare.exe")
    } else {
        sidecar.join("node_modules/.bin/bare")
    };
    let pear_bundle_path = sidecar.join("out/ncy-pear-sidecar.bundle");
    let pear_seed_path = paths.identity.with_file_name("pear-seed.txt");
    let settings = storage::settings::load(&paths.settings).unwrap_or_default();
    let desktop_prefs = commands::load_desktop_prefs(&paths.settings);
    let identity = DesktopIdentity::load_or_create(&paths.identity, &settings.device_name)?;
    let pairings = Arc::new(RwLock::new(
        load_pairings(&paths.pairings).unwrap_or_default(),
    ));
    let peers = Arc::new(RwLock::new(load_peers(&paths.peers).unwrap_or_default()));
    let peer_notify = Arc::new(Notify::new());
    let events = Arc::new(EventStore::open(&paths.events)?);
    let blobs = Arc::new(BlobStore::open(&paths.blobs)?);
    let (bus, _) = broadcast::channel::<SyncEvent>(256);
    let (outbound, _) = broadcast::channel::<OutboundEvent>(256);
    let (blob_requests, _) = broadcast::channel::<crate::sync::ManualBlobRequest>(64);
    let automatic_file_download_limit_bytes = Arc::new(std::sync::atomic::AtomicU64::new(
        settings.automatic_file_download_limit_bytes,
    ));
    let advertised_host = network::best_lan_ip();
    let clock = Arc::new(DesktopClock::new(
        identity.device_id.clone(),
        max_stored_hlc(&events),
    ));
    let server_status = Arc::new(RwLock::new("stopped"));
    let server_addr = Arc::new(RwLock::new(None));
    let pairing_registry: PairingRegistry =
        Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new()));
    let pair_security = PairSecurityState::new(settings.discoverable);
    let transport_mode = Arc::new(RwLock::new(settings.sync_transport_mode));
    let pear_gateway = Arc::new(RwLock::new(None::<Arc<PearGateway>>));
    let pear_sidecar = Arc::new(RwLock::new(None::<Arc<PearSidecar>>));

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

    Ok(AppState {
        paths,
        identity_device_id: identity.device_id,
        events,
        pairings,
        settings: Arc::new(RwLock::new(settings)),
        bus,
        advertised_host: Arc::new(RwLock::new(advertised_host)),
        clock,
        outbound,
        blobs,
        automatic_file_download_limit_bytes,
        blob_requests,
        blob_gc_pending: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        keystore: Arc::new(ProtectionKeyStore::new()),
        server_status,
        server_addr,
        mdns: std::sync::Mutex::new(None),
        peers,
        peer_notify,
        pairing_registry,
        pair_security,
        desktop_prefs: Arc::new(StdRwLock::new(desktop_prefs)),
        server_ctx,
        peer_deps,
        transport_mode,
        pear_gateway,
        pear_sidecar,
        pear_public_key: Arc::new(RwLock::new(None)),
        pear_runtime_path,
        pear_bundle_path,
        pear_seed_path,
        sync_shutdown: std::sync::Mutex::new(None),
    })
}

async fn start(state: &AppState) {
    apply_sync_enabled(state, true).await;
    let settings = state.settings.read().await;
    println!(
        "Ncy client running as {} on port {} ({:?})",
        settings.device_name, settings.port, settings.sync_transport_mode
    );
}

async fn run_foreground(state: &AppState) -> Result<()> {
    start(state).await;
    println!("Press Ctrl+C to stop.");
    tokio::signal::ctrl_c().await.context("wait for Ctrl+C")?;
    apply_sync_enabled(state, false).await;
    Ok(())
}

fn parse_seconds(args: &[String]) -> Result<u64> {
    let Some(index) = args.iter().position(|arg| arg == "--seconds") else {
        return Ok(15);
    };
    args.get(index + 1)
        .context("--seconds requires a value")?
        .parse()
        .context("--seconds must be a positive integer")
}

async fn sync_once(state: &AppState, seconds: u64) -> Result<()> {
    if seconds == 0 {
        bail!("--seconds must be greater than zero");
    }
    start(state).await;
    println!("Syncing for {seconds} seconds...");
    tokio::time::sleep(Duration::from_secs(seconds)).await;
    apply_sync_enabled(state, false).await;
    println!("Sync window complete.");
    Ok(())
}

async fn print_status(state: &AppState) -> Result<()> {
    let settings = state.settings.read().await;
    let peers = state.peers.read().await;
    let pairings = state.pairings.read().await;
    println!("Profile: {}", state.paths.root.display());
    println!(
        "Device: {} ({})",
        settings.device_name, state.identity_device_id
    );
    println!("Transport: {:?}", settings.sync_transport_mode);
    println!("Port: {}", settings.port);
    println!(
        "Peers: {} ({} enabled)",
        peers.peers.len(),
        peers.peers.iter().filter(|p| !p.disabled).count()
    );
    println!("Inbound pairings: {}", pairings.pairings.len());
    println!("Chats: {}", state.events.list_chat_ids()?.len());
    Ok(())
}

async fn print_peers(state: &AppState) -> Result<()> {
    let peers = state.peers.read().await;
    if peers.peers.is_empty() {
        println!("No outbound peers configured.");
        return Ok(());
    }
    for peer in &peers.peers {
        let route = if peer.pear_public_key.is_some() {
            "Pear/LAN"
        } else {
            "LAN"
        };
        println!(
            "{}\t{}\t{}\t{}:{}\tlast seen: {}\t{}",
            peer.id,
            peer.label,
            if peer.disabled { "disabled" } else { "enabled" },
            peer.host,
            peer.port,
            peer.last_seen_at
                .map(|v| v.to_string())
                .unwrap_or_else(|| "never".into()),
            route
        );
    }
    Ok(())
}

async fn pair_command(state: &AppState, args: &[String]) -> Result<()> {
    match args.first().map(String::as_str) {
        Some("create") => {
            create_pairing(state, option_value(args, "--name").unwrap_or("CLI pairing")).await
        }
        Some("add") => add_pairing(state, args).await,
        _ => bail!("usage: ncy-desktop client pair <create|add> [options]"),
    }
}

fn option_value<'a>(args: &'a [String], option: &str) -> Option<&'a str> {
    args.iter()
        .position(|arg| arg == option)
        .and_then(|index| args.get(index + 1))
        .map(String::as_str)
}

async fn create_pairing(state: &AppState, label: &str) -> Result<()> {
    let settings = state.settings.read().await.clone();
    if settings.sync_transport_mode.uses_pear() {
        crate::apply_pear_mode(state, settings.sync_transport_mode).await?;
    }
    let pairing = Pairing::new_with_label(label.to_owned());
    {
        let mut pairings = state.pairings.write().await;
        pairings.pairings.push(pairing.clone());
        save_pairings(&state.paths.pairings, &pairings)?;
    }
    let payload = PairingPayload {
        v: 1,
        device_id: state.identity_device_id.clone(),
        name: settings.device_name,
        host: state.advertised_host.read().await.clone(),
        port: settings.port,
        secret: pairing.secret_b64,
        pear_public_key: state.pear_public_key.read().await.clone(),
    };
    println!("{}", serde_json::to_string_pretty(&payload)?);
    crate::apply_pear_mode(state, crate::sync::SyncTransportMode::LocalOnly).await?;
    Ok(())
}

#[derive(Deserialize)]
struct IncomingPairing {
    #[serde(rename = "deviceId")]
    device_id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    host: String,
    port: u16,
    secret: String,
    #[serde(default, rename = "pearPublicKey")]
    pear_public_key: Option<String>,
    #[serde(default)]
    availability: Option<String>,
}

async fn add_pairing(state: &AppState, args: &[String]) -> Result<()> {
    let json = if let Some(file) = option_value(args, "--file") {
        std::fs::read_to_string(file).with_context(|| format!("read pairing file {file}"))?
    } else if let Some(json) = option_value(args, "--json") {
        json.to_owned()
    } else {
        bail!("pair add requires --file <path> or --json <pairing-json>");
    };
    let incoming: IncomingPairing =
        serde_json::from_str(json.trim()).context("parse pairing JSON")?;
    if incoming.device_id == state.identity_device_id {
        bail!("cannot add this device's own pairing");
    }
    if incoming.secret.trim().is_empty() {
        bail!("pairing JSON is missing its secret");
    }
    if incoming
        .pear_public_key
        .as_deref()
        .is_some_and(|key| !valid_public_key(key))
    {
        bail!("pairing JSON contains an invalid Pear public key");
    }
    if incoming.host.trim().is_empty() && incoming.pear_public_key.is_none() {
        bail!("pairing JSON has neither a LAN endpoint nor a Pear public key");
    }
    let mut peers = state.peers.write().await;
    if peers.peers.iter().any(|peer| peer.id == incoming.device_id) {
        bail!("already syncing with that device");
    }
    let label = if incoming.name.trim().is_empty() {
        incoming.device_id.clone()
    } else {
        incoming.name.clone()
    };
    peers.peers.push(OutboundPeer {
        id: incoming.device_id,
        label,
        authenticated_name: (!incoming.name.trim().is_empty()).then_some(incoming.name),
        host: incoming.host,
        port: incoming.port,
        pear_public_key: incoming.pear_public_key,
        availability: incoming.availability,
        disabled: false,
        secret_b64: incoming.secret,
        created_at: storage::now_ms(),
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
    save_peers(&state.paths.peers, &peers)?;
    println!("Pairing added.");
    Ok(())
}
