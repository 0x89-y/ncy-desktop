use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;

use anyhow::Result;

use crate::model::event::{op_type, ChatEventWire};

use super::EventStore;

/// Compaction trigger: chats with strictly more events than this get rewritten down
/// to the minimum log that materializes identically. Smaller chats are left alone.
const THRESHOLD: usize = 2_000;

/// Walk every chat's `events.ndjson` and compact any oversized one. Idempotent:
/// re-running on a freshly compacted chat is a no-op.
pub fn run_if_needed(store: &Arc<EventStore>) -> Result<()> {
    let chat_ids = store.list_chat_ids().unwrap_or_default();
    for cid in chat_ids {
        let events = match store.read_chat(&cid) {
            Ok(e) => e,
            Err(_) => continue,
        };
        if events.len() < THRESHOLD {
            continue;
        }
        let compacted = compact(&events);
        if compacted.len() >= events.len() {
            continue;
        }
        if let Err(e) = rewrite_log(store.root(), &cid, &compacted) {
            tracing::warn!("compaction rewrite failed for {cid}: {e}");
            continue;
        }
        tracing::info!(
            "compacted chat {cid}: {} → {} events",
            events.len(),
            compacted.len()
        );
    }
    Ok(())
}

/// Pure: smallest subset of `events` that materializes identically and preserves
/// tombstone semantics. Mirrors `EventLogCompactor.compact` on the phone — the same
/// algorithm so the two sides converge to the same compacted state.
pub fn compact(events: &[ChatEventWire]) -> Vec<ChatEventWire> {
    let mut sorted: Vec<&ChatEventWire> = events.iter().collect();
    sorted.sort_by(|a, b| a.hlc.cmp(&b.hlc));

    let mut latest_metadata: Option<&ChatEventWire> = None;
    let mut tombstones: BTreeMap<String, &ChatEventWire> = BTreeMap::new();
    let mut latest_per_message: BTreeMap<String, &ChatEventWire> = BTreeMap::new();

    for event in &sorted {
        match event.op_type.as_str() {
            op_type::CHAT_TOMBSTONED => {
                // Whole chat is dead.
                return vec![(*event).clone()];
            }
            op_type::CHAT_METADATA_CHANGED => {
                latest_metadata = Some(*event);
            }
            op_type::MESSAGE_TOMBSTONED => {
                if let Some(id) = event.payload.get("messageId").and_then(|v| v.as_str()) {
                    tombstones.insert(id.to_string(), *event);
                    latest_per_message.remove(id);
                }
            }
            op_type::MESSAGE_CREATED | op_type::MESSAGE_EDITED => {
                let id = event
                    .payload
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if id.is_empty() || tombstones.contains_key(id) {
                    continue;
                }
                latest_per_message.insert(id.to_string(), *event);
            }
            _ => {} // unknown op — drop on compaction (forward compat: re-arriving via sync would re-add)
        }
    }

    let mut result: Vec<ChatEventWire> = Vec::with_capacity(
        latest_metadata.iter().count() + tombstones.len() + latest_per_message.len(),
    );
    if let Some(m) = latest_metadata {
        result.push(m.clone());
    }
    for v in latest_per_message.values() {
        result.push((*v).clone());
    }
    for v in tombstones.values() {
        result.push((*v).clone());
    }
    result.sort_by(|a, b| a.hlc.cmp(&b.hlc));
    result
}

fn rewrite_log(root: &Path, chat_id: &str, events: &[ChatEventWire]) -> Result<()> {
    // Atomic-ish: write to temp, rename. EventStore's per-chat lock isn't exposed
    // here, so we accept a small risk window — compaction runs at startup before any
    // peer connects, so concurrent writes shouldn't happen.
    let path = root.join(format!("{}.ndjson", sanitize_id(chat_id)));
    let tmp = path.with_extension("ndjson.tmp");
    {
        let mut f = fs::File::create(&tmp)?;
        for e in events {
            let line = serde_json::to_string(e)?;
            f.write_all(line.as_bytes())?;
            f.write_all(b"\n")?;
        }
        f.flush()?;
    }
    fs::rename(&tmp, &path)?;
    Ok(())
}

fn sanitize_id(id: &str) -> String {
    id.chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' => c,
            _ => '_',
        })
        .collect()
}
