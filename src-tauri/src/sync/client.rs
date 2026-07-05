//! Outbound sync client — the desktop's *client* role, mirroring the phone's
//! `SyncClient.kt`. Lets one desktop connect to another desktop's (or any)
//! sync server: challenge/HMAC handshake, then bidirectional Event/Ack/Blob
//! streaming with per-peer watermarks and a loop guard by `originDevice`.
//!
//! A single [`run_peer_supervisor`] task reconciles the persisted peer list
//! (`peers.json`) with one reconnecting connection task per peer.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Result};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use futures_util::stream::SplitSink;
use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio::sync::{broadcast, Notify, RwLock};
use tokio_tungstenite::tungstenite::protocol::Message as WsMessage;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

use crate::model::event::ChatEventWire;
use crate::model::HlcTimestamp;
use crate::storage::{now_ms, save_peers, EventStore, OutboundPeer, PeerFile};

use super::auth::sign_auth;
use super::blob::{extract_media_refs, BlobStore};
use super::clock::DesktopClock;
use super::outbound::{OutboundEvent, OutboundTx};
use super::scrub_superseded;
use super::server::SyncEvent;
use super::wire::WireMessage;

type ClientSink = SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, WsMessage>;

/// Everything a peer connection task needs. Same shared singletons the server
/// uses, plus the peer registry for watermark persistence.
pub struct PeerSyncDeps {
    pub identity_device_id: String,
    pub events: Arc<EventStore>,
    pub bus: broadcast::Sender<SyncEvent>,
    pub clock: Arc<DesktopClock>,
    pub outbound: OutboundTx,
    pub blobs: Arc<BlobStore>,
    pub peers: Arc<RwLock<PeerFile>>,
    pub peers_path: PathBuf,
}

/// Outbound blob chunk size (pre-base64). Matches the server / phone.
const BLOB_CHUNK_SIZE: usize = 64 * 1024;

/// Reconcile `peers.json` with running connection tasks. Re-runs whenever
/// `notify` is signalled (peer added/removed) so changes take effect without a
/// restart. A peer whose host/port/secret changed is treated as new (old task
/// aborted, fresh one spawned).
pub async fn run_peer_supervisor(deps: Arc<PeerSyncDeps>, notify: Arc<Notify>) {
    let mut running: HashMap<String, (OutboundPeer, tokio::task::JoinHandle<()>)> = HashMap::new();
    loop {
        let desired: Vec<OutboundPeer> = deps.peers.read().await.peers.clone();

        // Drop tasks for peers that were removed or whose connection config changed.
        running.retain(|id, (cfg, handle)| {
            let keep = desired.iter().any(|p| {
                &p.id == id
                    && p.host == cfg.host
                    && p.port == cfg.port
                    && p.secret_b64 == cfg.secret_b64
            });
            if !keep {
                handle.abort();
            }
            keep
        });

        // Spawn a reconnecting task for every new/changed peer.
        for p in &desired {
            if running.contains_key(&p.id) {
                continue;
            }
            let d = deps.clone();
            let pid = p.id.clone();
            let handle = tokio::spawn(async move { run_peer(d, pid).await });
            running.insert(p.id.clone(), (p.clone(), handle));
        }

        notify.notified().await;
    }
}

/// One peer: connect, run a session, and on any disconnect retry with capped
/// exponential backoff. Returns when the peer is removed from the registry.
async fn run_peer(deps: Arc<PeerSyncDeps>, peer_id: String) {
    let mut backoff = Duration::from_secs(2);
    loop {
        let peer = match deps
            .peers
            .read()
            .await
            .peers
            .iter()
            .find(|p| p.id == peer_id)
            .cloned()
        {
            Some(p) => p,
            None => return, // removed — supervisor will also abort us
        };

        match connect_and_run(&deps, &peer).await {
            Ok(()) => backoff = Duration::from_secs(2),
            Err(e) => tracing::debug!("peer {peer_id} session ended: {e}"),
        }
        let _ = deps.bus.send(SyncEvent::PeerDisconnected {
            peer: peer_id.clone(),
        });
        tokio::time::sleep(backoff).await;
        backoff = (backoff * 2).min(Duration::from_secs(30));
    }
}

async fn connect_and_run(deps: &Arc<PeerSyncDeps>, peer: &OutboundPeer) -> Result<()> {
    let url = format!("ws://{}:{}/sync", peer.host, peer.port);
    let (ws, _resp) = connect_async(&url).await?;
    let (mut tx, mut rx) = ws.split();

    // ---- Handshake: wait for Challenge, answer with a signed Hello, await Welcome.
    let mut authenticated = false;
    while let Some(frame) = rx.next().await {
        let frame = frame?;
        let text = match frame {
            WsMessage::Text(t) => t,
            WsMessage::Ping(p) => {
                let _ = tx.send(WsMessage::Pong(p)).await;
                continue;
            }
            WsMessage::Close(_) => return Ok(()),
            _ => continue,
        };
        match serde_json::from_str::<WireMessage>(&text) {
            Ok(WireMessage::Challenge { nonce }) => {
                let auth = sign_auth(&peer.secret_b64, &nonce)
                    .ok_or_else(|| anyhow!("could not sign challenge"))?;
                let hello = WireMessage::Hello {
                    device_id: deps.identity_device_id.clone(),
                    auth,
                    since_hlc: peer.last_received_hlc.clone(),
                };
                tx.send(WsMessage::text(serde_json::to_string(&hello)?))
                    .await?;
            }
            Ok(WireMessage::Welcome { .. }) => {
                authenticated = true;
                break;
            }
            Ok(WireMessage::Error { code, message }) => {
                return Err(anyhow!("peer rejected us: {code}: {message}"));
            }
            _ => { /* ignore anything else pre-auth */ }
        }
    }
    if !authenticated {
        return Err(anyhow!("connection closed before Welcome"));
    }

    deps.bus.send(SyncEvent::PeerConnected {
        peer: peer.id.clone(),
        pairing_id: None,
        phone_id: Some(peer.id.clone()),
    })
    .ok();
    touch_last_seen(deps, &peer.id).await;

    // Loop guard: never ship the peer its own events back.
    let remote_id = peer.id.clone();
    let mut last_sent: Option<HlcTimestamp> = peer.last_sent_hlc.clone();
    let mut pending: HashMap<String, PendingBlob> = HashMap::new();

    // Push everything the peer hasn't acked yet (mirrors server `stream_catchup`).
    catchup(deps, &mut tx, &remote_id, &mut last_sent).await?;

    let mut outbound_rx = deps.outbound.subscribe();

    loop {
        tokio::select! {
            inbound = rx.next() => {
                let frame = match inbound {
                    Some(Ok(f)) => f,
                    Some(Err(e)) => return Err(e.into()),
                    None => return Ok(()),
                };
                match frame {
                    WsMessage::Text(t) => {
                        handle_frame(deps, &mut tx, &remote_id, &t, &mut last_sent, &mut pending).await?;
                    }
                    WsMessage::Ping(p) => { let _ = tx.send(WsMessage::Pong(p)).await; }
                    WsMessage::Close(_) => return Ok(()),
                    _ => {}
                }
            }
            broadcast_msg = outbound_rx.recv() => {
                match broadcast_msg {
                    Ok(OutboundEvent(wire)) => {
                        if wire.origin_device == remote_id { continue; }
                        if let Some(ref w) = last_sent {
                            if &wire.hlc <= w { continue; }
                        }
                        let frame = WireMessage::Event {
                            event: serde_json::to_value(&wire).unwrap_or(serde_json::Value::Null),
                        };
                        if tx.send(WsMessage::text(serde_json::to_string(&frame)?)).await.is_err() {
                            return Ok(());
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        let snap = last_sent.clone();
                        let _ = snap;
                        catchup(deps, &mut tx, &remote_id, &mut last_sent).await?;
                    }
                    Err(broadcast::error::RecvError::Closed) => return Ok(()),
                }
            }
        }
    }
}

/// Stream every local event with `hlc > last_sent`, HLC-ascending, skipping the
/// peer's own events. Advances `last_sent` optimistically; the peer's `Ack`
/// persists it.
async fn catchup(
    deps: &Arc<PeerSyncDeps>,
    tx: &mut ClientSink,
    remote_id: &str,
    last_sent: &mut Option<HlcTimestamp>,
) -> Result<()> {
    let chat_ids = deps.events.list_chat_ids().unwrap_or_default();
    let mut all: Vec<ChatEventWire> = Vec::new();
    for cid in chat_ids {
        if let Ok(evs) = deps.events.read_chat(&cid) {
            all.extend(evs);
        }
    }
    all.sort_by(|a, b| a.hlc.cmp(&b.hlc));
    for wire in all {
        if wire.origin_device == remote_id {
            continue;
        }
        if let Some(ref w) = last_sent {
            if &wire.hlc <= w {
                continue;
            }
        }
        let frame = WireMessage::Event {
            event: serde_json::to_value(&wire).unwrap_or(serde_json::Value::Null),
        };
        tx.send(WsMessage::text(serde_json::to_string(&frame)?))
            .await?;
        *last_sent = Some(wire.hlc.clone());
    }
    Ok(())
}

struct PendingBlob {
    media_type: String,
    buffer: Vec<u8>,
}

async fn handle_frame(
    deps: &Arc<PeerSyncDeps>,
    tx: &mut ClientSink,
    remote_id: &str,
    raw: &str,
    last_sent: &mut Option<HlcTimestamp>,
    pending: &mut HashMap<String, PendingBlob>,
) -> Result<()> {
    let msg: WireMessage = match serde_json::from_str(raw) {
        Ok(m) => m,
        Err(e) => {
            tracing::debug!("peer sent malformed frame: {e}");
            return Ok(());
        }
    };
    match msg {
        WireMessage::Event { event } => {
            let wire: ChatEventWire = match serde_json::from_value(event) {
                Ok(w) => w,
                Err(_) => return Ok(()),
            };
            deps.clock.receive(&wire.hlc, now_ms());
            let hlc = wire.hlc.clone();
            let chat_id = wire.chat_id.clone();
            let event_id = wire.event_id.clone();
            let payload = wire.payload.clone();
            let is_new = deps.events.append(&wire).unwrap_or(false);
            if is_new {
                if payload.get("supersedes").is_some() {
                    if let Ok(evs) = deps.events.read_chat(&chat_id) {
                        if let Some(scrubbed) = scrub_superseded(&evs) {
                            if let Err(e) = deps.events.replace_chat(&chat_id, &scrubbed) {
                                tracing::warn!("supersede scrub failed for {chat_id}: {e}");
                            }
                        }
                    }
                }
                let _ = deps.bus.send(SyncEvent::EventReceived {
                    chat_id: chat_id.clone(),
                    event_id,
                });
            }
            persist_received(deps, remote_id, &hlc).await;
            let ack = WireMessage::Ack { up_to_hlc: hlc };
            let _ = tx
                .send(WsMessage::text(serde_json::to_string(&ack).unwrap()))
                .await;

            for (hash, media_type) in extract_media_refs(&payload) {
                if deps.blobs.exists(&hash, &media_type) || pending.contains_key(&hash) {
                    continue;
                }
                pending.insert(
                    hash.clone(),
                    PendingBlob {
                        media_type: media_type.clone(),
                        buffer: Vec::new(),
                    },
                );
                let req = WireMessage::BlobRequest {
                    hash: hash.clone(),
                    media_type,
                };
                if tx
                    .send(WsMessage::text(serde_json::to_string(&req).unwrap()))
                    .await
                    .is_err()
                {
                    pending.remove(&hash);
                }
            }
        }
        WireMessage::Ack { up_to_hlc } => {
            let advance = match last_sent.as_ref() {
                Some(cur) => up_to_hlc > *cur,
                None => true,
            };
            if advance {
                *last_sent = Some(up_to_hlc.clone());
            }
            persist_sent(deps, remote_id, &up_to_hlc).await;
        }
        WireMessage::BlobRequest { hash, media_type } => {
            serve_blob(deps, tx, &hash, &media_type).await;
        }
        WireMessage::BlobChunk {
            hash,
            media_type: _,
            offset,
            total_size,
            data,
            is_last,
        } => {
            let entry = match pending.get_mut(&hash) {
                Some(e) => e,
                None => return Ok(()),
            };
            let bytes = match B64.decode(&data) {
                Ok(b) => b,
                Err(_) => {
                    pending.remove(&hash);
                    return Ok(());
                }
            };
            if offset as usize != entry.buffer.len() {
                pending.remove(&hash);
                return Ok(());
            }
            entry.buffer.extend_from_slice(&bytes);
            if !is_last {
                return Ok(());
            }
            let done = pending.remove(&hash).expect("just had it");
            if done.buffer.len() != total_size as usize {
                return Ok(());
            }
            if deps
                .blobs
                .write_verified(&hash, &done.media_type, &done.buffer)
                .is_ok()
            {
                let _ = deps.bus.send(SyncEvent::BlobArrived { hash });
            }
        }
        WireMessage::BlobMissing { hash } => {
            pending.remove(&hash);
        }
        _ => {}
    }
    Ok(())
}

async fn serve_blob(deps: &Arc<PeerSyncDeps>, tx: &mut ClientSink, hash: &str, media_type: &str) {
    if !deps.blobs.exists(hash, media_type) {
        let frame = WireMessage::BlobMissing {
            hash: hash.to_string(),
        };
        let _ = tx
            .send(WsMessage::text(serde_json::to_string(&frame).unwrap()))
            .await;
        return;
    }
    let bytes = match deps.blobs.read(hash, media_type) {
        Ok(b) => b,
        Err(_) => {
            let frame = WireMessage::BlobMissing {
                hash: hash.to_string(),
            };
            let _ = tx
                .send(WsMessage::text(serde_json::to_string(&frame).unwrap()))
                .await;
            return;
        }
    };
    let total = bytes.len();
    if total == 0 {
        let frame = WireMessage::BlobChunk {
            hash: hash.to_string(),
            media_type: media_type.to_string(),
            offset: 0,
            total_size: 0,
            data: String::new(),
            is_last: true,
        };
        let _ = tx
            .send(WsMessage::text(serde_json::to_string(&frame).unwrap()))
            .await;
        return;
    }
    let mut offset = 0usize;
    while offset < total {
        let n = std::cmp::min(BLOB_CHUNK_SIZE, total - offset);
        let is_last = offset + n >= total;
        let frame = WireMessage::BlobChunk {
            hash: hash.to_string(),
            media_type: media_type.to_string(),
            offset: offset as u32,
            total_size: total as u32,
            data: B64.encode(&bytes[offset..offset + n]),
            is_last,
        };
        if tx
            .send(WsMessage::text(serde_json::to_string(&frame).unwrap()))
            .await
            .is_err()
        {
            return;
        }
        offset += n;
    }
}

/// Advance + persist the high-water HLC the peer has acked from us.
async fn persist_sent(deps: &Arc<PeerSyncDeps>, peer_id: &str, hlc: &HlcTimestamp) {
    let mut guard = deps.peers.write().await;
    if let Some(p) = guard.peers.iter_mut().find(|p| p.id == peer_id) {
        let bump = match p.last_sent_hlc.as_ref() {
            Some(cur) => hlc > cur,
            None => true,
        };
        if bump {
            p.last_sent_hlc = Some(hlc.clone());
            p.last_seen_at = Some(now_ms());
            let _ = save_peers(&deps.peers_path, &guard);
        }
    }
}

/// Advance + persist the high-water HLC we've applied from the peer (sent as
/// `Hello.sinceHlc` next reconnect).
async fn persist_received(deps: &Arc<PeerSyncDeps>, peer_id: &str, hlc: &HlcTimestamp) {
    let mut guard = deps.peers.write().await;
    if let Some(p) = guard.peers.iter_mut().find(|p| p.id == peer_id) {
        let bump = match p.last_received_hlc.as_ref() {
            Some(cur) => hlc > cur,
            None => true,
        };
        if bump {
            p.last_received_hlc = Some(hlc.clone());
            p.last_seen_at = Some(now_ms());
            let _ = save_peers(&deps.peers_path, &guard);
        }
    }
}

async fn touch_last_seen(deps: &Arc<PeerSyncDeps>, peer_id: &str) {
    let mut guard = deps.peers.write().await;
    if let Some(p) = guard.peers.iter_mut().find(|p| p.id == peer_id) {
        p.last_seen_at = Some(now_ms());
        let _ = save_peers(&deps.peers_path, &guard);
    }
}
