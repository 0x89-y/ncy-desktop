use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use rand::RngCore;
use serde::{Deserialize, Serialize};

use crate::model::HlcTimestamp;

/// This desktop's stable identity. Generated once on first launch and reused forever.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesktopIdentity {
    #[serde(rename = "deviceId")]
    pub device_id: String,
    pub name: String,
}

impl DesktopIdentity {
    pub fn load_or_create(path: &Path, default_name: &str) -> Result<Self> {
        if let Ok(bytes) = fs::read(path) {
            if let Ok(id) = serde_json::from_slice::<DesktopIdentity>(&bytes) {
                return Ok(id);
            }
        }
        let id = DesktopIdentity {
            device_id: uuid::Uuid::new_v4().to_string(),
            name: default_name.to_string(),
        };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, serde_json::to_vec_pretty(&id)?)
            .with_context(|| format!("write identity to {:?}", path))?;
        Ok(id)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }
}

/// One pairing slot. The desktop allows multiple of these so several phones can sync
/// to the same machine. Each carries its own pre-shared secret.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pairing {
    /// Internal id assigned by the desktop. Unrelated to the phone's deviceId — until
    /// a phone authenticates against this slot it is "unclaimed", after which we
    /// remember `phoneId` for display.
    pub id: String,
    pub label: String,
    #[serde(rename = "secretB64")]
    pub secret_b64: String,
    #[serde(rename = "phoneId")]
    pub phone_id: Option<String>,
    #[serde(rename = "phoneName")]
    pub phone_name: Option<String>,
    #[serde(rename = "createdAt")]
    pub created_at: i64,
    #[serde(rename = "lastSeenAt")]
    pub last_seen_at: Option<i64>,
    /// Highest HLC the phone has acked from us. Drives the outbound catchup the next
    /// time this pairing reconnects. `None` for fresh or pre-phase-2a pairings —
    /// such a connection sends every event we have.
    #[serde(default, rename = "lastSentToPhoneHlc")]
    pub last_sent_to_phone_hlc: Option<HlcTimestamp>,
}

impl Pairing {
    pub fn new_with_label(label: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            label: label.into(),
            secret_b64: generate_secret_b64(),
            phone_id: None,
            phone_name: None,
            created_at: now_ms(),
            last_seen_at: None,
            last_sent_to_phone_hlc: None,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PairingFile {
    #[serde(default)]
    pub pairings: Vec<Pairing>,
}

pub fn load_pairings(path: &Path) -> Result<PairingFile> {
    if !path.exists() {
        return Ok(PairingFile::default());
    }
    let bytes = fs::read(path)?;
    if bytes.is_empty() {
        return Ok(PairingFile::default());
    }
    Ok(serde_json::from_slice::<PairingFile>(&bytes).unwrap_or_default())
}

pub fn save_pairings(path: &Path, file: &PairingFile) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_vec_pretty(file)?)?;
    Ok(())
}

/// A remote peer this desktop connects *out* to (the client role) — e.g. another
/// desktop. Built from the pairing JSON that peer produced, so we already know its
/// `deviceId`, host/port and the shared secret. Watermarks mirror the phone's
/// `SyncPeer`: `last_sent_hlc` is the high-water the peer has acked from us
/// (drives our outbound catch-up); `last_received_hlc` is the highest we've
/// applied from it (sent as `Hello.sinceHlc`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboundPeer {
    /// The remote's `deviceId` (from its pairing payload). Loop-guard + identity.
    pub id: String,
    pub label: String,
    pub host: String,
    pub port: u16,
    #[serde(rename = "secretB64")]
    pub secret_b64: String,
    #[serde(rename = "createdAt")]
    pub created_at: i64,
    #[serde(default, rename = "lastSeenAt")]
    pub last_seen_at: Option<i64>,
    #[serde(default, rename = "lastSentHlc")]
    pub last_sent_hlc: Option<HlcTimestamp>,
    #[serde(default, rename = "lastReceivedHlc")]
    pub last_received_hlc: Option<HlcTimestamp>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PeerFile {
    #[serde(default)]
    pub peers: Vec<OutboundPeer>,
}

pub fn load_peers(path: &Path) -> Result<PeerFile> {
    if !path.exists() {
        return Ok(PeerFile::default());
    }
    let bytes = fs::read(path)?;
    if bytes.is_empty() {
        return Ok(PeerFile::default());
    }
    Ok(serde_json::from_slice::<PeerFile>(&bytes).unwrap_or_default())
}

pub fn save_peers(path: &Path, file: &PeerFile) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_vec_pretty(file)?)?;
    Ok(())
}

pub fn generate_secret_b64() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    B64.encode(bytes)
}

pub fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

pub fn data_paths(root: PathBuf) -> DataPaths {
    DataPaths {
        identity: root.join("identity.json"),
        pairings: root.join("pairings.json"),
        peers: root.join("peers.json"),
        events: root.join("events"),
        blobs: root.join("blobs"),
        settings: root.join("settings.json"),
        root,
    }
}

pub struct DataPaths {
    pub root: PathBuf,
    pub identity: PathBuf,
    pub pairings: PathBuf,
    /// Outbound peers this desktop connects to (the client role).
    pub peers: PathBuf,
    pub events: PathBuf,
    pub blobs: PathBuf,
    pub settings: PathBuf,
}
