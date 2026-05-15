use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use serde_json::Value;
use sha2::{Digest, Sha256};

/// Filesystem-backed content-addressed store. One flat directory under the data root —
/// since content is hash-keyed, two chats with the same image share storage automatically.
pub struct BlobStore {
    root: PathBuf,
}

impl BlobStore {
    pub fn open(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        fs::create_dir_all(&root).with_context(|| format!("create {:?}", root))?;
        Ok(Self { root })
    }

    pub fn path_for(&self, hash: &str, media_type: &str) -> PathBuf {
        self.root.join(format!("{}.{}", hash, ext_for(media_type)))
    }

    pub fn exists(&self, hash: &str, media_type: &str) -> bool {
        self.path_for(hash, media_type).is_file()
    }

    pub fn read(&self, hash: &str, media_type: &str) -> Result<Vec<u8>> {
        let path = self.path_for(hash, media_type);
        fs::read(&path).with_context(|| format!("read blob {:?}", path))
    }

    /// Write atomically: temp file + rename. Verifies the bytes hash to the expected
    /// `hash` before committing — refuses to store mismatched payloads.
    pub fn write_verified(&self, hash: &str, media_type: &str, bytes: &[u8]) -> Result<()> {
        if !is_valid_hash(hash) {
            return Err(anyhow!("bad hash: {hash}"));
        }
        let computed = sha256_hex(bytes);
        if computed != hash {
            return Err(anyhow!(
                "hash mismatch: expected {hash}, got {computed}"
            ));
        }
        let path = self.path_for(hash, media_type);
        if path.is_file() {
            return Ok(()); // already have it
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let tmp = path.with_extension(format!(
            "{}.tmp.{}",
            ext_for(media_type),
            uuid::Uuid::new_v4()
        ));
        fs::write(&tmp, bytes)?;
        if let Err(e) = fs::rename(&tmp, &path) {
            // If rename fails (e.g. cross-fs, very rare here), fall back.
            let _ = fs::copy(&tmp, &path);
            let _ = fs::remove_file(&tmp);
            return Err(e.into());
        }
        Ok(())
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}

fn ext_for(media_type: &str) -> &'static str {
    match media_type {
        "voice" => "m4a",
        "image" => "jpg",
        _ => "bin",
    }
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    hex_lower(&h.finalize())
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0F) as usize] as char);
    }
    out
}

fn is_valid_hash(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// Pull every (hash, mediaType) reference out of an event payload (`MessageCreated`,
/// `MessageEdited`, or `ChatMetadataChanged`). Returns dedup'd, lowercase hex hashes
/// — matches the phone's `MediaHash.extractHash` on what the wire actually carries.
pub fn extract_media_refs(payload: &Value) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();

    // Message-shaped payloads.
    if let Some(s) = payload.get("voiceFile").and_then(|v| v.as_str()) {
        if let Some(h) = extract_hash(s) {
            out.push((h, "voice".into()));
        }
    }

    if let Some(arr) = payload.get("imageFiles").and_then(|v| v.as_array()) {
        for v in arr {
            if let Some(s) = v.as_str() {
                if let Some(h) = extract_hash(s) {
                    out.push((h, "image".into()));
                }
            }
        }
    }

    if let Some(blocks) = payload.get("content").and_then(|v| v.as_array()) {
        for block in blocks {
            if let Some(spans) = block.get("spans").and_then(|v| v.as_array()) {
                for span in spans {
                    if let Some(s) = span.get("inlineImage").and_then(|v| v.as_str()) {
                        if let Some(h) = extract_hash(s) {
                            out.push((h, "image".into()));
                        }
                    }
                }
            }
        }
    }

    // ChatMetadata-shaped payloads (phase 2b slice 2): pull the avatar (and the
    // background, even though no client currently renders it — cheap, future-proof).
    if let Some(s) = payload.get("avatarFile").and_then(|v| v.as_str()) {
        if let Some(h) = extract_hash(s) {
            out.push((h, "image".into()));
        }
    }
    if let Some(s) = payload.get("backgroundFile").and_then(|v| v.as_str()) {
        if let Some(h) = extract_hash(s) {
            out.push((h, "image".into()));
        }
    }

    out.sort();
    out.dedup();
    out
}

fn extract_hash(reference: &str) -> Option<String> {
    let name = reference.rsplit('/').next()?;
    let stem = name.split('.').next()?;
    if is_valid_hash(stem) {
        Some(stem.to_string())
    } else {
        None
    }
}
