//! Server configuration — a small TOML file. Unlike the desktop (which stores
//! settings in `settings.json` under its app-data dir and edits them from a UI),
//! the server is configured up front by an operator editing a file.
//!
//! Runtime *state* — identity, pairings, outbound peers, the event log, blobs —
//! is NOT in this file. It lives as JSON/NDJSON under `data_dir`, using the exact
//! same `ncy_core::storage::data_paths` layout the desktop uses, so the pairing
//! CLI can append to `pairings.json` at runtime and the daemon picks it up.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use ncy_core::sync::SyncTransportMode;
use serde::Deserialize;

/// Parsed `config.toml`. Every field has a default so a minimal file (even an
/// empty one) boots with sane values.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Human-readable label the phone shows for this peer. Also seeds the stable
    /// identity's `name` on first run.
    pub device_name: String,
    /// TCP port the WebSocket sync server binds (0.0.0.0:port).
    pub port: u16,
    /// Root for all runtime state (identity, pairings, peers, events, blobs).
    pub data_dir: PathBuf,
    /// Where `whisper-cli` + the ggml model live. Defaults to `<data_dir>/whisper`
    /// when left unset.
    pub whisper_dir: Option<PathBuf>,
    /// Advertise over mDNS (`_ncy-sync._tcp.local.`) so phones can discover this
    /// peer without typing its IP and paired clients can recover after DHCP/IP
    /// changes. Its LAN-visible label uses the random installation ID, never
    /// `device_name`. Off by default; QR pairing still works, address recovery does not.
    pub discoverable: bool,
    /// Master switch for the auto-transcription loop. Even when true, the loop
    /// only runs if `whisper-cli` and the model are actually present.
    pub transcription_enabled: bool,
    /// Connection routing policy. `localOnly` is the default and never starts
    /// the Pear worker.
    pub sync_transport_mode: SyncTransportMode,
    /// Bare runtime and bundled Ncy Pear worker. Required for Pear-enabled modes.
    pub pear_runtime_path: Option<PathBuf>,
    pub pear_bundle_path: Option<PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            device_name: default_device_name(),
            port: 8765,
            data_dir: default_data_dir(),
            whisper_dir: None,
            discoverable: false,
            transcription_enabled: false,
            sync_transport_mode: SyncTransportMode::LocalOnly,
            pear_runtime_path: None,
            pear_bundle_path: None,
        }
    }
}

impl Config {
    /// Load and parse the TOML at `path`. A missing file is an error (unlike the
    /// desktop's settings, which silently default) — running the daemon without a
    /// config almost always means a misconfigured deployment, so fail loud.
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("read config {}", path.display()))?;
        let cfg: Config =
            toml::from_str(&text).with_context(|| format!("parse config {}", path.display()))?;
        Ok(cfg)
    }

    /// The whisper install dir, honoring an explicit override or defaulting to
    /// `<data_dir>/whisper`.
    pub fn whisper_dir(&self) -> PathBuf {
        self.whisper_dir
            .clone()
            .unwrap_or_else(|| self.data_dir.join("whisper"))
    }

    pub fn pear_paths(&self) -> Result<(&Path, &Path)> {
        let runtime = self.pear_runtime_path.as_deref().ok_or_else(|| {
            anyhow::anyhow!("pear_runtime_path is required when Pear transport is enabled")
        })?;
        let bundle = self.pear_bundle_path.as_deref().ok_or_else(|| {
            anyhow::anyhow!("pear_bundle_path is required when Pear transport is enabled")
        })?;
        Ok((runtime, bundle))
    }

    /// Default config-file location, mirroring the TODO's design: a system path on
    /// unix, `%APPDATA%` on Windows.
    pub fn default_config_path() -> PathBuf {
        #[cfg(windows)]
        {
            appdata_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("ncy-server")
                .join("config.toml")
        }
        #[cfg(not(windows))]
        {
            PathBuf::from("/etc/ncy-server/config.toml")
        }
    }
}

fn default_device_name() -> String {
    hostname::get()
        .ok()
        .and_then(|h| h.into_string().ok())
        .map(|h| format!("{h} (ncy-server)"))
        .unwrap_or_else(|| "Ncy Server".to_string())
}

fn default_data_dir() -> PathBuf {
    #[cfg(windows)]
    {
        appdata_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("ncy-server")
            .join("data")
    }
    #[cfg(not(windows))]
    {
        PathBuf::from("/var/lib/ncy-server")
    }
}

#[cfg(windows)]
fn appdata_dir() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::Config;
    use ncy_core::sync::SyncTransportMode;

    #[test]
    fn pear_is_off_by_default() {
        let config: Config = toml::from_str("").unwrap();
        assert_eq!(config.sync_transport_mode, SyncTransportMode::LocalOnly);
        assert!(config.pear_runtime_path.is_none());
    }

    #[test]
    fn parses_pear_policy_and_artifact_paths() {
        let config: Config = toml::from_str(
            r#"
sync_transport_mode = "pearPreferred"
pear_runtime_path = "/opt/ncy/bare"
pear_bundle_path = "/opt/ncy/ncy-pear-sidecar.bundle"
"#,
        )
        .unwrap();
        assert_eq!(config.sync_transport_mode, SyncTransportMode::PearPreferred);
        assert!(config.pear_paths().is_ok());
    }
}
