use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{Context, Result};

use crate::model::event::{ChatEventWire, WIRE_VERSION};
use crate::model::HlcTimestamp;

/// On-disk append-only event log. One NDJSON file per chat under `events/{chatId}.ndjson`.
/// All access is serialized via a per-chat mutex behind a `Mutex<HashMap<...>>` to keep
/// the implementation simple — sync throughput is well below contention concerns.
pub struct EventStore {
    root: PathBuf,
    locks: Mutex<HashMap<String, std::sync::Arc<Mutex<()>>>>,
    seen: Mutex<HashMap<String, ()>>,
}

impl EventStore {
    pub fn open(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        fs::create_dir_all(&root).with_context(|| format!("create {:?}", root))?;
        let store = Self {
            root,
            locks: Mutex::new(HashMap::new()),
            seen: Mutex::new(HashMap::new()),
        };
        store.preload_seen()?;
        Ok(store)
    }

    fn preload_seen(&self) -> Result<()> {
        let mut seen = self.seen.lock().unwrap();
        if !self.root.exists() {
            return Ok(());
        }
        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("ndjson") {
                continue;
            }
            let file = File::open(&path)?;
            let reader = BufReader::new(file);
            for line in reader.lines() {
                let line = match line {
                    Ok(l) => l,
                    Err(_) => continue,
                };
                if line.trim().is_empty() {
                    continue;
                }
                if let Ok(wire) = serde_json::from_str::<ChatEventWire>(&line) {
                    seen.insert(wire.event_id, ());
                }
            }
        }
        Ok(())
    }

    fn lock_for(&self, chat_id: &str) -> std::sync::Arc<Mutex<()>> {
        let mut map = self.locks.lock().unwrap();
        map.entry(chat_id.to_string())
            .or_insert_with(|| std::sync::Arc::new(Mutex::new(())))
            .clone()
    }

    fn path_for(&self, chat_id: &str) -> PathBuf {
        let mut p = self.root.clone();
        p.push(format!("{}.ndjson", sanitize_id(chat_id)));
        p
    }

    /// Append the event if its id has not been seen before. Returns `true` if appended,
    /// `false` if it was a duplicate (idempotent dedup by `eventId`, per protocol §10.5).
    pub fn append(&self, wire: &ChatEventWire) -> Result<bool> {
        {
            let mut seen = self.seen.lock().unwrap();
            if seen.contains_key(&wire.event_id) {
                return Ok(false);
            }
            seen.insert(wire.event_id.clone(), ());
        }

        let lock = self.lock_for(&wire.chat_id);
        let _guard = lock.lock().unwrap();

        let path = self.path_for(&wire.chat_id);
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .with_context(|| format!("open {:?}", path))?;
        let line = serde_json::to_string(wire)?;
        f.write_all(line.as_bytes())?;
        f.write_all(b"\n")?;
        f.flush()?;
        Ok(true)
    }

    pub fn read_chat(&self, chat_id: &str) -> Result<Vec<ChatEventWire>> {
        let path = self.path_for(chat_id);
        if !path.exists() {
            return Ok(Vec::new());
        }
        let lock = self.lock_for(chat_id);
        let _guard = lock.lock().unwrap();
        let file = File::open(&path)?;
        let reader = BufReader::new(file);
        let mut out = Vec::new();
        for (i, line) in reader.lines().enumerate() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<ChatEventWire>(&line) {
                Ok(w) => {
                    if w.v != WIRE_VERSION {
                        tracing::warn!(
                            "skipping event with unknown wire version {} in {:?} line {}",
                            w.v,
                            path,
                            i + 1
                        );
                        continue;
                    }
                    out.push(w);
                }
                Err(e) => {
                    tracing::warn!("skipping malformed event in {:?} line {}: {e}", path, i + 1);
                }
            }
        }
        Ok(out)
    }

    pub fn list_chat_ids(&self) -> Result<Vec<String>> {
        if !self.root.exists() {
            return Ok(Vec::new());
        }
        let mut ids = Vec::new();
        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("ndjson") {
                continue;
            }
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                ids.push(stem.to_string());
            }
        }
        Ok(ids)
    }

    /// Highest HLC ever appended for the given chat. Used by callers that need a
    /// "since" watermark (currently informational only — Phase 1 receivers don't
    /// gate inbound events on it).
    pub fn max_hlc(&self, chat_id: &str) -> Result<Option<HlcTimestamp>> {
        let events = self.read_chat(chat_id)?;
        Ok(events.into_iter().map(|e| e.hlc).max())
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}

fn sanitize_id(id: &str) -> String {
    id.chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' => c,
            _ => '_',
        })
        .collect()
}
