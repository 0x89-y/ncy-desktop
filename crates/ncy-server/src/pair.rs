//! `ncy-server pair` — mint a pairing payload for a phone (or another desktop),
//! register it, and print it.
//!
//! The payload is the same §2 JSON the desktop's Settings UI produces, so the
//! phone's "Add device" flow (scan or paste) treats this peer identically. The
//! new pairing (with its pre-shared secret) is appended to `pairings.json` under
//! the configured `data_dir` so the running daemon authenticates the phone the
//! after its next restart. Registration and the printed secret share the same
//! freshly-generated key. Pair-file administration is deliberately offline: stop
//! the daemon, modify/list credentials with this module's commands, then restart.

use anyhow::Result;
use std::collections::HashMap;

use ncy_core::network::best_lan_ip;
use ncy_core::storage::{
    data_paths, load_pairings, load_peers, save_pairings, DesktopIdentity, EventStore, Pairing,
};

use crate::config::Config;

pub fn run(config: &Config, name: Option<String>) -> Result<()> {
    let _ = pairing_pear_public_key(config)?;
    let paths = data_paths(config.data_dir.clone());
    std::fs::create_dir_all(&paths.root)?;

    let identity = DesktopIdentity::load_or_create(&paths.identity, &config.device_name)?;
    let label = name.unwrap_or_else(|| "Phone".to_string());
    let pairing = Pairing::new_with_label(label);

    // Persist the pairing so the daemon recognizes this phone on connect.
    let mut file = load_pairings(&paths.pairings).unwrap_or_default();
    file.pairings.push(pairing.clone());
    save_pairings(&paths.pairings, &file)?;

    print_payload(config, &identity, &pairing, "Registered")
}

pub fn rotate(config: &Config, id: &str) -> Result<()> {
    let _ = pairing_pear_public_key(config)?;
    let paths = data_paths(config.data_dir.clone());
    let identity = DesktopIdentity::load_or_create(&paths.identity, &config.device_name)?;
    let mut file = load_pairings(&paths.pairings).unwrap_or_default();
    let pairing = file
        .pairings
        .iter_mut()
        .find(|pairing| pairing.id == id)
        .ok_or_else(|| anyhow::anyhow!("pairing ID not found: {id}"))?;
    let affected = pairing
        .phone_name
        .clone()
        .unwrap_or_else(|| pairing.label.clone());
    pairing.rotate_secret();
    let rotated = pairing.clone();
    save_pairings(&paths.pairings, &file)?;
    eprintln!(
        "Rotated credential for '{affected}'. The old device is revoked; the slot and chat data remain, and the device must pair again."
    );
    print_payload(config, &identity, &rotated, "Rotated")
}

pub fn revoke(config: &Config, id: &str) -> Result<()> {
    let paths = data_paths(config.data_dir.clone());
    let mut file = load_pairings(&paths.pairings).unwrap_or_default();
    let pairing = file
        .pairings
        .iter()
        .find(|pairing| pairing.id == id)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("pairing ID not found: {id}"))?;
    file.pairings.retain(|pairing| pairing.id != id);
    save_pairings(&paths.pairings, &file)?;
    eprintln!(
        "Revoked and removed '{}' ({id}). Chat data remains unchanged.",
        pairing.phone_name.unwrap_or(pairing.label)
    );
    warn_restart();
    Ok(())
}

pub fn list(config: &Config) -> Result<()> {
    let paths = data_paths(config.data_dir.clone());
    let file = load_pairings(&paths.pairings).unwrap_or_default();
    if file.pairings.is_empty() {
        println!("No pairings.");
        return Ok(());
    }
    let duplicate_names = duplicate_phone_names(&file.pairings);
    for pairing in file.pairings {
        let device = match (pairing.phone_name, pairing.phone_id) {
            (Some(name), Some(phone_id)) if duplicate_names.contains_key(&name_key(&name)) => {
                format!("{name} [device {}]", short_fingerprint(&phone_id))
            }
            (Some(name), _) => name,
            (None, _) => "unclaimed".into(),
        };
        println!("{}\t{}\t{}", pairing.id, pairing.label, device);
    }
    Ok(())
}

pub fn list_peers(config: &Config) -> Result<()> {
    let paths = data_paths(config.data_dir.clone());
    let file = load_peers(&paths.peers).unwrap_or_default();
    if file.peers.is_empty() {
        println!("No outbound peers.");
        return Ok(());
    }
    println!("ID\tLABEL\tENDPOINT\tLAST_DISCOVERY_MS\tLAST_AUTH_MS\tAUTH_ENDPOINT\tNEXT_RETRY_MS\tLAST_ERROR");
    for peer in file.peers {
        let auth_endpoint = match (peer.last_authenticated_host, peer.last_authenticated_port) {
            (Some(host), Some(port)) => format!("{host}:{port}"),
            _ => "-".into(),
        };
        println!(
            "{}\t{}\t{}:{}\t{}\t{}\t{}\t{}\t{}",
            peer.id,
            peer.label,
            peer.host,
            peer.port,
            peer.last_discovered_at
                .map_or_else(|| "-".into(), |v| v.to_string()),
            peer.last_authenticated_at
                .map_or_else(|| "-".into(), |v| v.to_string()),
            auth_endpoint,
            peer.next_retry_at
                .map_or_else(|| "-".into(), |v| v.to_string()),
            peer.last_error
                .unwrap_or_else(|| "-".into())
                .replace(['\t', '\n'], " "),
        );
    }
    Ok(())
}

pub fn purge_tombstones(config: &Config, confirmed: bool) -> Result<()> {
    if !confirmed {
        anyhow::bail!(
            "refusing: pass --yes-i-understand only after every stale device and backup is gone; old history may resurrect deleted chats or messages"
        );
    }
    let paths = data_paths(config.data_dir.clone());
    let events = EventStore::open(paths.events)?;
    let removed = events.purge_tombstones()?;
    println!("Forgot {removed} deletion record(s). Stale history may now restore those chats or messages.");
    Ok(())
}

fn duplicate_phone_names(pairings: &[Pairing]) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    for name in pairings
        .iter()
        .filter_map(|pairing| pairing.phone_name.as_deref())
    {
        let key = name_key(name);
        if !key.is_empty() {
            *counts.entry(key).or_insert(0) += 1;
        }
    }
    counts.retain(|_, count| *count > 1);
    counts
}

fn name_key(name: &str) -> String {
    name.trim().to_lowercase()
}

fn short_fingerprint(device_id: &str) -> String {
    let fingerprint: String = device_id
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .take(8)
        .collect();
    if fingerprint.is_empty() {
        "unknown".into()
    } else {
        fingerprint
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_fingerprint_ignores_uuid_punctuation() {
        assert_eq!(short_fingerprint("A1B2-C3D4-E5F6"), "a1b2c3d4");
    }

    #[test]
    fn duplicate_names_ignore_case_and_whitespace() {
        let mut first = Pairing::new_with_label("first");
        first.phone_name = Some("Pixel 9".into());
        let mut second = Pairing::new_with_label("second");
        second.phone_name = Some(" pixel 9 ".into());
        assert!(duplicate_phone_names(&[first, second]).contains_key("pixel 9"));
    }
}

fn print_payload(
    config: &Config,
    identity: &DesktopIdentity,
    pairing: &Pairing,
    verb: &str,
) -> Result<()> {
    let host = best_lan_ip();
    let pear_public_key = pairing_pear_public_key(config)?;
    let mut payload = serde_json::json!({
        "v": 1,
        "deviceId": identity.device_id,
        "name": config.device_name,
        "host": host,
        "port": config.port,
        "secret": pairing.secret_b64,
    });
    if let Some(public_key) = pear_public_key {
        payload["pearPublicKey"] = serde_json::Value::String(public_key);
    }

    let compact = serde_json::to_string(&payload)?;
    // JSON on stdout so it can be piped/copied; guidance + QR on stderr so they
    // don't pollute a `> pairing.json` redirect.
    println!("{}", serde_json::to_string_pretty(&payload)?);
    eprintln!(
        "\nWARNING: the JSON and QR below contain a pairing credential. Do not post, log, screenshot, or leave redirected copies behind.\n{verb} pairing slot {} ('{}') at {}:{}. On the phone: Add device → paste the JSON above, or scan:\n",
        pairing.id, pairing.label, host, config.port
    );
    if !config.discoverable {
        eprintln!(
            "NOTE: discoverable=false. This pairing works at the printed address, but the client cannot find the server automatically if that LAN address changes. Use stable DNS/IP or enable discoverable.\n"
        );
    }
    print_qr(&compact);
    warn_restart();
    Ok(())
}

fn pairing_pear_public_key(config: &Config) -> Result<Option<String>> {
    if config.sync_transport_mode.uses_pear() {
        let path = config.data_dir.join("pear-public-key.txt");
        let key = std::fs::read_to_string(&path).map_err(|_| {
            anyhow::anyhow!(
                "Pear public key is not available yet; start ncy-server once with Pear enabled, then run this command again"
            )
        })?;
        let key = key.trim().to_owned();
        if !ncy_core::sync::pear::valid_public_key(&key) {
            anyhow::bail!("stored Pear public key is invalid; restart ncy-server to regenerate it");
        }
        Ok(Some(key))
    } else {
        Ok(None)
    }
}

fn warn_restart() {
    eprintln!("A running ncy-server reloads this pairing change automatically.\n");
}

/// Render the payload as a terminal QR (half-block unicode). Best-effort: a render
/// failure (e.g. payload too large for the chosen EC level) just prints a note.
fn print_qr(data: &str) {
    use qrcode::render::unicode;
    use qrcode::QrCode;
    match QrCode::new(data.as_bytes()) {
        Ok(code) => {
            let image = code
                .render::<unicode::Dense1x2>()
                .dark_color(unicode::Dense1x2::Light)
                .light_color(unicode::Dense1x2::Dark)
                .quiet_zone(true)
                .build();
            eprintln!("{image}");
        }
        Err(e) => eprintln!("(could not render QR: {e} — use the JSON above)"),
    }
}
