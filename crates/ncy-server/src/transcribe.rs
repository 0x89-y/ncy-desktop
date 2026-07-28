//! Auto-transcription loop.
//!
//! Two cooperating tasks fed by the sync-event bus:
//!
//! - **dispatcher** — watches for inbound voice messages, decides which ones need
//!   a transcript, waits for the audio blob to arrive if it hasn't yet, and hands
//!   ready jobs to the worker. Owns the dedup state so the event loop never blocks
//!   on whisper.
//! - **worker** — runs `whisper-cli` one job at a time (off the async runtime via
//!   `spawn_blocking`) and re-authors the message as a `MessageEdited` carrying the
//!   transcript, exactly like the desktop's `author_transcribe_voice`.
//!
//! Protected chats are never transcribed: their payloads are AES-GCM sealed and
//! the server holds no key, so encrypted events are skipped and simply relayed.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use anyhow::{anyhow, Result};
use tokio::sync::{broadcast, mpsc};

use ncy_core::model::event::op_type;
use ncy_core::model::{Message, MessageType};
use ncy_core::storage::materialize::materialize;
use ncy_core::storage::now_ms;
use ncy_core::storage::whisper::{transcribe_bytes, WhisperPaths};
use ncy_core::storage::EventStore;
use ncy_core::sync::{author_event, BlobStore, DesktopClock, OutboundTx, SyncEvent};

/// Everything the loop needs. All shared with the rest of the daemon via `Arc`.
pub struct TranscribeDeps {
    pub device_id: String,
    pub events: Arc<EventStore>,
    pub blobs: Arc<BlobStore>,
    pub clock: Arc<DesktopClock>,
    pub outbound: OutboundTx,
    pub bus: broadcast::Sender<SyncEvent>,
    pub whisper: WhisperPaths,
}

/// One unit of work: transcribe `hash` and write it back onto this message.
struct Job {
    chat_id: String,
    message_id: String,
    hash: String,
}

pub async fn run_transcription_loop(deps: Arc<TranscribeDeps>) {
    let (job_tx, job_rx) = mpsc::channel::<Job>(256);

    let worker_deps = deps.clone();
    tokio::spawn(async move { worker(worker_deps, job_rx).await });

    dispatcher(deps, job_tx).await;
}

/// Watch the bus, gate candidate voice messages, and forward ready jobs. Blob-less
/// jobs park in `pending` keyed by hash until the matching `BlobArrived` fires.
async fn dispatcher(deps: Arc<TranscribeDeps>, job_tx: mpsc::Sender<Job>) {
    let mut rx = deps.bus.subscribe();
    let mut pending: HashMap<String, Vec<Job>> = HashMap::new();
    // Messages we've already acted on this session — stops catch-up replays and
    // our own re-emitted MessageEdited from re-dispatching.
    let mut handled: HashSet<(String, String)> = HashSet::new();

    loop {
        match rx.recv().await {
            Ok(SyncEvent::EventReceived { chat_id, event_id }) => {
                if let Some(job) = inspect(&deps, &chat_id, &event_id, &mut handled) {
                    if deps.blobs.exists(&job.hash, "voice") {
                        let _ = job_tx.send(job).await;
                    } else {
                        tracing::debug!("voice blob {} not here yet — parking", &job.hash[..8]);
                        pending.entry(job.hash.clone()).or_default().push(job);
                    }
                }
            }
            Ok(SyncEvent::BlobArrived { hash }) => {
                if let Some(jobs) = pending.remove(&hash) {
                    for job in jobs {
                        if deps.blobs.exists(&job.hash, "voice") {
                            let _ = job_tx.send(job).await;
                        }
                    }
                }
            }
            Ok(_) => {}
            Err(broadcast::error::RecvError::Lagged(n)) => {
                tracing::warn!("transcription dispatcher lagged, dropped {n} events");
            }
            Err(broadcast::error::RecvError::Closed) => {
                tracing::info!("bus closed — transcription dispatcher exiting");
                break;
            }
        }
    }
}

/// Decide whether the event named by `event_id` is a voice message that still needs
/// a transcript. Returns a `Job` (and marks it handled) when so; `None` otherwise.
fn inspect(
    deps: &TranscribeDeps,
    chat_id: &str,
    event_id: &str,
    handled: &mut HashSet<(String, String)>,
) -> Option<Job> {
    let events = deps.events.read_chat(chat_id).ok()?;
    let ev = events.iter().find(|e| e.event_id == event_id)?;

    // Our own event echoing back — never transcribe it (loop guard).
    if ev.origin_device == deps.device_id {
        return None;
    }
    // Only message create/edit events carry a voice body.
    if ev.op_type != op_type::MESSAGE_CREATED && ev.op_type != op_type::MESSAGE_EDITED {
        return None;
    }
    // Protected chats seal the payload; the server can't decrypt, so it can't
    // transcribe. Relay only.
    if ev.payload.get("enc").is_some() {
        return None;
    }

    let msg: Message = serde_json::from_value(ev.payload.clone()).ok()?;
    if !matches!(msg.message_type, MessageType::Voice) {
        return None;
    }
    let voice_file = msg.voice_file.as_deref()?;
    let hash = extract_hash(voice_file)?;

    let key = (chat_id.to_string(), msg.id.clone());
    if handled.contains(&key) {
        return None;
    }

    // Consult the *current* materialized state — another device may have already
    // transcribed this message and synced the result to us.
    let current = materialize(events).and_then(|m| m.messages.into_iter().find(|m| m.id == msg.id));
    if let Some(cur) = &current {
        if cur
            .transcript
            .as_deref()
            .map(|t| !t.trim().is_empty())
            .unwrap_or(false)
        {
            handled.insert(key);
            return None;
        }
    }

    handled.insert(key);
    Some(Job {
        chat_id: chat_id.to_string(),
        message_id: msg.id,
        hash,
    })
}

/// Run jobs serially. A per-session cache keyed on audio hash avoids re-running
/// whisper when several messages share the same recording.
async fn worker(deps: Arc<TranscribeDeps>, mut job_rx: mpsc::Receiver<Job>) {
    let mut text_cache: HashMap<String, String> = HashMap::new();
    while let Some(job) = job_rx.recv().await {
        match transcribe_one(&deps, &job, &mut text_cache).await {
            Ok(true) => tracing::info!(
                "transcribed {}/{} ({})",
                &job.chat_id[..job.chat_id.len().min(8)],
                &job.message_id[..job.message_id.len().min(8)],
                &job.hash[..8]
            ),
            Ok(false) => {}
            Err(e) => tracing::warn!(
                "transcription failed for {}/{}: {e}",
                job.chat_id,
                job.message_id
            ),
        }
    }
}

async fn transcribe_one(
    deps: &TranscribeDeps,
    job: &Job,
    text_cache: &mut HashMap<String, String>,
) -> Result<bool> {
    let text = if let Some(cached) = text_cache.get(&job.hash) {
        cached.clone()
    } else {
        let bytes = deps.blobs.read(&job.hash, "voice")?;
        let paths = deps.whisper.clone();
        // whisper-cli is a blocking subprocess — keep it off the async runtime.
        let t = tokio::task::spawn_blocking(move || transcribe_bytes(&paths, bytes)).await??;
        text_cache.insert(job.hash.clone(), t.clone());
        t
    };

    // Re-read current state: the message may have been edited or already
    // transcribed while whisper was running.
    let events = deps.events.read_chat(&job.chat_id)?;
    let mat = materialize(events).ok_or_else(|| anyhow!("chat gone"))?;
    let mut msg = mat
        .messages
        .into_iter()
        .find(|m| m.id == job.message_id)
        .ok_or_else(|| anyhow!("message gone"))?;
    if msg
        .transcript
        .as_deref()
        .map(|t| !t.trim().is_empty())
        .unwrap_or(false)
    {
        return Ok(false); // someone beat us to it
    }

    msg.transcript = Some(text);
    msg.edited_at = Some(now_ms());
    let payload = serde_json::to_value(&msg)?;
    author_event(
        &deps.clock,
        &deps.events,
        &deps.outbound,
        &deps.bus,
        &deps.device_id,
        &job.chat_id,
        op_type::MESSAGE_EDITED,
        payload,
    )?;
    Ok(true)
}

/// Pull the content-addressed hash out of a `voice/<hash>.m4a` reference. Legacy
/// random-named references (pre content-addressing) return `None` and are skipped.
fn extract_hash(reference: &str) -> Option<String> {
    let name = reference.rsplit('/').next()?;
    let stem = name.split('.').next()?;
    if stem.len() == 64 && stem.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
        Some(stem.to_string())
    } else {
        None
    }
}
