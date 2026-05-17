use std::fs;
use std::path::Path;

use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub port: u16,
    #[serde(rename = "deviceName")]
    pub device_name: String,
    #[serde(default = "default_theme")]
    pub theme: String,
    /// When false (the default), the desktop does NOT advertise itself over mDNS,
    /// so it stays invisible to network scanners / casual browsers on shared
    /// networks. Pairing still works via the QR / pasted JSON, which carries the
    /// host:port directly. Flipping this takes effect immediately (no restart).
    #[serde(default)]
    pub discoverable: bool,
}

fn default_theme() -> String {
    "system".to_string()
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            port: 8765,
            device_name: hostname::get()
                .ok()
                .and_then(|h| h.into_string().ok())
                .unwrap_or_else(|| "Ncy Desktop".to_string()),
            theme: default_theme(),
            discoverable: false,
        }
    }
}

pub fn load(path: &Path) -> Result<AppSettings> {
    if !path.exists() {
        return Ok(AppSettings::default());
    }
    let bytes = fs::read(path)?;
    if bytes.is_empty() {
        return Ok(AppSettings::default());
    }
    Ok(serde_json::from_slice(&bytes).unwrap_or_default())
}

pub fn save(path: &Path, settings: &AppSettings) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_vec_pretty(settings)?)?;
    Ok(())
}
