//! Reusable entry point for the Ncy server daemon and its administration CLI.

mod config;
mod daemon;
mod pair;
mod transcribe;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

use config::Config;

#[derive(Parser)]
#[command(
    name = "ncy-server",
    version,
    about = "Headless Ncy sync peer + transcription daemon"
)]
struct Cli {
    /// Path to config.toml (defaults to /etc/ncy-server/config.toml, or
    /// %APPDATA%\ncy-server\config.toml on Windows).
    #[arg(short, long, global = true)]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Run the sync daemon (the default when no subcommand is given).
    Run,
    /// Generate + register a pairing payload for a phone or another device.
    Pair {
        /// Human-readable label for the device being paired.
        #[arg(short, long)]
        name: Option<String>,
    },
    /// Rotate one pairing secret, preserving its slot/label and requiring re-pairing.
    Rotate {
        /// Pairing slot ID shown by `pairings`.
        #[arg(long)]
        id: String,
    },
    /// Permanently revoke and remove one pairing slot.
    Revoke {
        /// Pairing slot ID shown by `pairings`.
        #[arg(long)]
        id: String,
    },
    /// List pairing IDs and their claimed device, without printing secrets.
    Pairings,
    /// List outbound peer endpoints and persisted connection diagnostics, without secrets.
    Peers,
    /// Permanently forget deletion records; stale backups may resurrect chats or messages.
    PurgeTombstones {
        /// Required acknowledgement of the irreversible resurrection risk.
        #[arg(long)]
        yes_i_understand: bool,
    },
}

/// Run the server command line. `args` excludes the executable name. A desktop
/// distribution can supply its bundled Bare runtime and worker as defaults;
/// explicit paths in config.toml always win.
pub fn run(args: &[String], bundled_pear: Option<(PathBuf, PathBuf)>) -> Result<()> {
    init_tracing();
    let cli =
        Cli::try_parse_from(std::iter::once("ncy-server").chain(args.iter().map(String::as_str)))?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("create server async runtime")?;
    runtime.block_on(run_async(cli, bundled_pear))
}

async fn run_async(cli: Cli, bundled_pear: Option<(PathBuf, PathBuf)>) -> Result<()> {
    let config_path = cli
        .config
        .clone()
        .unwrap_or_else(Config::default_config_path);
    let mut config = load_config(&config_path)?;
    if let Some((runtime, bundle)) = bundled_pear {
        config.pear_runtime_path.get_or_insert(runtime);
        config.pear_bundle_path.get_or_insert(bundle);
    }

    match cli.command.unwrap_or(Command::Run) {
        Command::Run => daemon::run(config).await,
        Command::Pair { name } => pair::run(&config, name),
        Command::Rotate { id } => pair::rotate(&config, &id),
        Command::Revoke { id } => pair::revoke(&config, &id),
        Command::Pairings => pair::list(&config),
        Command::Peers => pair::list_peers(&config),
        Command::PurgeTombstones { yes_i_understand } => {
            pair::purge_tombstones(&config, yes_i_understand)
        }
    }
}

fn load_config(path: &Path) -> Result<Config> {
    if path.exists() {
        tracing::info!("config: {}", path.display());
        Config::load(path)
    } else {
        tracing::warn!(
            "no config at {} — using defaults (data_dir {}). Write one to customize.",
            path.display(),
            Config::default().data_dir.display()
        );
        Ok(Config::default())
    }
}

fn init_tracing() {
    use tracing_subscriber::EnvFilter;
    let _ = tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .try_init();
}
