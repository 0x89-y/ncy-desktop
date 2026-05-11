use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::{Context, Result};
use futures_util::stream::SplitSink;
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, RwLock};
use tokio_tungstenite::tungstenite::protocol::Message as WsMessage;
use tokio_tungstenite::WebSocketStream;

type WsSink = SplitSink<WebSocketStream<TcpStream>, WsMessage>;

use base64::{engine::general_purpose::STANDARD as B64, Engine};
use std::collections::HashMap;

use crate::model::event::ChatEventWire;
use crate::model::HlcTimestamp;
use crate::storage::{now_ms, save_pairings, EventStore, PairingFile};

use super::auth::{random_nonce_b64, verify_auth};
use super::blob::{extract_media_refs, BlobStore};
use super::clock::DesktopClock;
use super::outbound::{OutboundEvent, OutboundTx};
use super::wire::WireMessage;

#[derive(Debug, Clone)]
pub enum SyncEvent {
    PeerConnected {
        peer: String,
        pairing_id: Option<String>,
        phone_id: Option<String>,
    },
    PeerAuthFailed {
        peer: String,
    },
    EventReceived {
        chat_id: String,
        event_id: String,
    },
    /// A locally-authored event was appended to the store. Frontend should refresh.
    EventAuthored {
        chat_id: String,
        event_id: String,
    },
    /// A media blob finished downloading and is now on disk. Frontend should re-render
    /// any open chat view referencing it.
    BlobArrived {
        hash: String,
    },
    PeerDisconnected {
        peer: String,
    },
    ServerStarted {
        addr: String,
    },
    ServerStopped,
    ServerError {
        message: String,
    },
}

/// Shared state every connection handler reads.
pub struct ServerContext {
    pub identity_device_id: String,
    pub identity_name: String,
    pub pairings: Arc<RwLock<PairingFile>>,
    pub pairings_path: std::path::PathBuf,
    pub events: Arc<EventStore>,
    pub bus: broadcast::Sender<SyncEvent>,
    pub clock: Arc<DesktopClock>,
    pub outbound: OutboundTx,
    pub blobs: Arc<BlobStore>,
}

pub async fn run(ctx: Arc<ServerContext>, addr: SocketAddr) -> Result<()> {
    let listener = TcpListener::bind(addr)
        .await
        .with_context(|| format!("bind {addr}"))?;
    let bound = listener.local_addr().unwrap_or(addr);
    let _ = ctx.bus.send(SyncEvent::ServerStarted {
        addr: bound.to_string(),
    });
    tracing::info!("sync server listening on {bound}");

    loop {
        let (stream, peer_addr) = match listener.accept().await {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("accept error: {e}");
                continue;
            }
        };
        let ctx = ctx.clone();
        tokio::spawn(async move {
            let peer_label = peer_addr.to_string();
            if let Err(e) = handle_connection(ctx.clone(), stream, &peer_label).await {
                tracing::debug!("connection {peer_label} ended: {e}");
            }
            let _ = ctx.bus.send(SyncEvent::PeerDisconnected {
                peer: peer_label,
            });
        });
    }
}

async fn handle_connection(
    ctx: Arc<ServerContext>,
    stream: tokio::net::TcpStream,
    peer_label: &str,
) -> Result<()> {
    let ws = tokio_tungstenite::accept_async(stream).await?;
    let (mut tx, mut rx) = ws.split();

    // 1. Send Challenge
    let nonce = random_nonce_b64();
    let challenge = WireMessage::Challenge {
        nonce: nonce.clone(),
    };
    tx.send(WsMessage::text(serde_json::to_string(&challenge)?))
        .await?;

    // 2. Wait for Hello
    let hello_raw = match rx.next().await {
        Some(Ok(WsMessage::Text(t))) => t.to_string(),
        Some(Ok(WsMessage::Binary(_))) => {
            send_error(&mut tx, "bad_hello", "binary frame not allowed").await;
            return Ok(());
        }
        _ => return Ok(()),
    };
    let hello: WireMessage = match serde_json::from_str(&hello_raw) {
        Ok(m) => m,
        Err(_) => {
            send_error(&mut tx, "bad_hello", "not JSON").await;
            return Ok(());
        }
    };
    let (phone_id, auth, since_hlc) = match hello {
        WireMessage::Hello {
            device_id,
            auth,
            since_hlc,
        } => (device_id, auth, since_hlc),
        _ => {
            send_error(&mut tx, "bad_hello", "expected Hello").await;
            return Ok(());
        }
    };

    // 3. Find a pairing whose secret authenticates this Hello
    let pairing_id = {
        let guard = ctx.pairings.read().await;
        guard
            .pairings
            .iter()
            .find(|p| verify_auth(&p.secret_b64, &nonce, &auth))
            .map(|p| p.id.clone())
    };
    let pairing_id = match pairing_id {
        Some(id) => id,
        None => {
            send_error(&mut tx, "bad_auth", "HMAC mismatch").await;
            let _ = ctx.bus.send(SyncEvent::PeerAuthFailed {
                peer: peer_label.to_string(),
            });
            return Ok(());
        }
    };

    // 4. Mark this pairing claimed by this phone, persist
    {
        let mut guard = ctx.pairings.write().await;
        if let Some(p) = guard.pairings.iter_mut().find(|p| p.id == pairing_id) {
            if p.phone_id.is_none() {
                p.phone_id = Some(phone_id.clone());
            }
            p.last_seen_at = Some(now_ms());
        }
        let _ = save_pairings(&ctx.pairings_path, &guard);
    }

    // 5. Welcome
    let welcome = WireMessage::Welcome {
        device_id: ctx.identity_device_id.clone(),
        name: ctx.identity_name.clone(),
    };
    tx.send(WsMessage::text(serde_json::to_string(&welcome)?))
        .await?;
    let _ = ctx.bus.send(SyncEvent::PeerConnected {
        peer: peer_label.to_string(),
        pairing_id: Some(pairing_id.clone()),
        phone_id: Some(phone_id.clone()),
    });

    // 6. Phase 2a outbound — push every event the phone hasn't acked yet, sorted by HLC.
    //    The phone's `Hello.sinceHlc` is its inbound watermark ("send me events past this").
    //    We respect the larger of (a) what the phone says it has and (b) what we've
    //    persisted as `last_sent_to_phone_hlc` for this pairing — defensive against a
    //    phone that wipes local state.
    let stored_watermark = {
        let guard = ctx.pairings.read().await;
        guard
            .pairings
            .iter()
            .find(|p| p.id == pairing_id)
            .and_then(|p| p.last_sent_to_phone_hlc.clone())
    };
    let initial_watermark = match (&since_hlc, &stored_watermark) {
        (Some(a), Some(b)) => Some(if a >= b { a.clone() } else { b.clone() }),
        (Some(a), None) => Some(a.clone()),
        (None, Some(b)) => Some(b.clone()),
        (None, None) => None,
    };
    let mut last_sent_hlc: Option<HlcTimestamp> = initial_watermark.clone();
    let mut pending_fetches: HashMap<String, PendingBlob> = HashMap::new();

    if let Err(e) = stream_catchup(&ctx, &mut tx, initial_watermark.as_ref(), &phone_id, &mut last_sent_hlc).await {
        tracing::warn!("catchup error: {e}");
    }

    // 7. Subscribe to live outbound events. Forward each (skipping the phone's own
    //    events; loop guard) over the same socket as Event frames.
    let mut outbound_rx = ctx.outbound.subscribe();

    // 8. Receive loop: phone sends Events (with Acks for our outbound) and Acks (for
    //    its own outbound that we already handled in step 6/7). Multiplexed with the
    //    outbound forwarder via select!.
    loop {
        tokio::select! {
            // Inbound from the phone.
            msg = rx.next() => {
                let msg = match msg {
                    Some(Ok(m)) => m,
                    Some(Err(e)) => {
                        tracing::debug!("ws recv error: {e}");
                        break;
                    }
                    None => break,
                };
                match msg {
                    WsMessage::Text(t) => {
                        if let Err(e) = process_inbound(
                            &ctx,
                            &mut tx,
                            &pairing_id,
                            &t,
                            &mut last_sent_hlc,
                            &mut pending_fetches,
                        ).await {
                            tracing::warn!("inbound error: {e}");
                        }
                    }
                    WsMessage::Ping(p) => {
                        let _ = tx.send(WsMessage::Pong(p)).await;
                    }
                    WsMessage::Close(_) => break,
                    _ => {}
                }
            }
            // Outbound: a desktop-authored event was broadcast. Forward to this peer
            // unless it originated from the peer itself (loop guard) or has already
            // been sent (watermark dedupe).
            broadcast_msg = outbound_rx.recv() => {
                match broadcast_msg {
                    Ok(OutboundEvent(wire)) => {
                        if wire.origin_device == phone_id {
                            continue;
                        }
                        if let Some(ref w) = last_sent_hlc {
                            if &wire.hlc <= w {
                                continue;
                            }
                        }
                        let frame = WireMessage::Event {
                            event: serde_json::to_value(&wire).unwrap_or(serde_json::Value::Null),
                        };
                        if tx.send(WsMessage::text(serde_json::to_string(&frame)?)).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        // Catch up by re-streaming every event past our current watermark.
                        let snap = last_sent_hlc.clone();
                        if let Err(e) = stream_catchup(&ctx, &mut tx, snap.as_ref(), &phone_id, &mut last_sent_hlc).await {
                            tracing::warn!("re-catchup after lag failed: {e}");
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }

    {
        let mut guard = ctx.pairings.write().await;
        if let Some(p) = guard.pairings.iter_mut().find(|p| p.id == pairing_id) {
            p.last_seen_at = Some(now_ms());
        }
        let _ = save_pairings(&ctx.pairings_path, &guard);
    }

    Ok(())
}

/// Stream every event whose HLC > `watermark` to the connected phone, sorted by HLC
/// across all chats. Updates `last_sent_hlc` as each frame is enqueued (the phone will
/// ack and we'll persist later).
async fn stream_catchup(
    ctx: &Arc<ServerContext>,
    tx: &mut WsSink,
    watermark: Option<&HlcTimestamp>,
    phone_id: &str,
    last_sent_hlc: &mut Option<HlcTimestamp>,
) -> Result<()> {
    let chat_ids = ctx.events.list_chat_ids().unwrap_or_default();
    let mut all: Vec<ChatEventWire> = Vec::new();
    for cid in chat_ids {
        if let Ok(events) = ctx.events.read_chat(&cid) {
            all.extend(events);
        }
    }
    all.sort_by(|a, b| a.hlc.cmp(&b.hlc));

    for wire in all {
        if let Some(w) = watermark {
            if &wire.hlc <= w {
                continue;
            }
        }
        // Loop guard: don't ship events the phone authored back to it.
        if wire.origin_device == phone_id {
            continue;
        }
        if let Some(ref w) = last_sent_hlc {
            if &wire.hlc <= w {
                continue;
            }
        }
        let frame = WireMessage::Event {
            event: serde_json::to_value(&wire).unwrap_or(serde_json::Value::Null),
        };
        tx.send(WsMessage::text(serde_json::to_string(&frame)?)).await?;
        // Optimistically advance — the phone will ack and we'll persist on Ack.
        *last_sent_hlc = Some(wire.hlc.clone());
    }
    Ok(())
}

/// Per-connection state for the receive side of a blob the desktop is fetching from
/// the phone. Lives in a `HashMap<hash, PendingBlob>` on the connection task's stack.
struct PendingBlob {
    media_type: String,
    buffer: Vec<u8>,
}

/// Outbound blob chunk size, bytes (pre-base64). 64 KiB matches the phone.
const BLOB_CHUNK_SIZE: usize = 64 * 1024;

async fn process_inbound(
    ctx: &Arc<ServerContext>,
    tx: &mut WsSink,
    pairing_id: &str,
    raw: &str,
    last_sent_hlc: &mut Option<HlcTimestamp>,
    pending_fetches: &mut HashMap<String, PendingBlob>,
) -> Result<()> {
    let msg: WireMessage = match serde_json::from_str(raw) {
        Ok(m) => m,
        Err(e) => {
            tracing::debug!("ignoring malformed frame: {e}");
            return Ok(());
        }
    };
    match msg {
        WireMessage::Event { event } => {
            let wire = match parse_event(&event) {
                Some(w) => w,
                None => {
                    tracing::warn!("ignored event missing required fields");
                    return Ok(());
                }
            };
            // Absorb the remote HLC so future ticks stay causally ahead.
            ctx.clock.receive(&wire.hlc, now_ms());

            let hlc = wire.hlc.clone();
            let event_id = wire.event_id.clone();
            let chat_id = wire.chat_id.clone();
            let payload = wire.payload.clone();
            let is_new = ctx.events.append(&wire).unwrap_or(false);
            if is_new {
                let _ = ctx.bus.send(SyncEvent::EventReceived {
                    chat_id: chat_id.clone(),
                    event_id,
                });
            }
            let ack = WireMessage::Ack { up_to_hlc: hlc };
            if let Err(e) = tx
                .send(WsMessage::text(serde_json::to_string(&ack).unwrap()))
                .await
            {
                tracing::warn!("send ack failed: {e}");
            }

            // Phase 2b: kick off blob fetches for any media references we don't have.
            for (hash, media_type) in extract_media_refs(&payload) {
                if ctx.blobs.exists(&hash, &media_type) {
                    continue;
                }
                if pending_fetches.contains_key(&hash) {
                    continue;
                }
                pending_fetches.insert(
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
                if let Err(e) = tx.send(WsMessage::text(serde_json::to_string(&req).unwrap())).await {
                    tracing::warn!("send blob request failed: {e}");
                    pending_fetches.remove(&hash);
                }
            }
        }
        WireMessage::Ack { up_to_hlc } => {
            // Phone has durably stored every event ≤ this HLC. Advance + persist the
            // outbound watermark so reconnect doesn't replay anything older.
            let advance = match last_sent_hlc.as_ref() {
                Some(cur) => up_to_hlc > *cur,
                None => true,
            };
            if advance {
                *last_sent_hlc = Some(up_to_hlc.clone());
            }
            let mut guard = ctx.pairings.write().await;
            if let Some(p) = guard.pairings.iter_mut().find(|p| p.id == pairing_id) {
                let should_persist = match p.last_sent_to_phone_hlc.as_ref() {
                    Some(cur) => up_to_hlc > *cur,
                    None => true,
                };
                if should_persist {
                    p.last_sent_to_phone_hlc = Some(up_to_hlc);
                    let _ = save_pairings(&ctx.pairings_path, &guard);
                }
            }
        }
        WireMessage::BlobRequest { hash, media_type } => {
            serve_blob(ctx, tx, &hash, &media_type).await;
        }
        WireMessage::BlobChunk {
            hash,
            media_type: _,
            offset,
            total_size,
            data,
            is_last,
        } => {
            let entry = match pending_fetches.get_mut(&hash) {
                Some(e) => e,
                None => return Ok(()), // unsolicited or already finalized
            };
            let bytes = match B64.decode(&data) {
                Ok(b) => b,
                Err(e) => {
                    tracing::warn!("blob {hash}: bad base64 — dropping: {e}");
                    pending_fetches.remove(&hash);
                    return Ok(());
                }
            };
            if offset as usize != entry.buffer.len() {
                tracing::warn!(
                    "blob {hash}: out-of-order chunk (got {offset}, expected {})",
                    entry.buffer.len()
                );
                pending_fetches.remove(&hash);
                return Ok(());
            }
            entry.buffer.extend_from_slice(&bytes);
            if !is_last {
                return Ok(());
            }
            let pending = pending_fetches.remove(&hash).expect("just had it");
            if pending.buffer.len() != total_size as usize {
                tracing::warn!(
                    "blob {hash}: size mismatch (got {} expected {total_size})",
                    pending.buffer.len()
                );
                return Ok(());
            }
            if let Err(e) =
                ctx.blobs
                    .write_verified(&hash, &pending.media_type, &pending.buffer)
            {
                tracing::warn!("blob {hash}: persist failed: {e}");
                return Ok(());
            }
            let _ = ctx.bus.send(SyncEvent::BlobArrived { hash });
        }
        WireMessage::BlobMissing { hash } => {
            pending_fetches.remove(&hash);
            tracing::debug!("peer reports blob {hash} missing");
        }
        _ => {}
    }
    Ok(())
}

async fn serve_blob(ctx: &Arc<ServerContext>, tx: &mut WsSink, hash: &str, media_type: &str) {
    if !ctx.blobs.exists(hash, media_type) {
        let frame = WireMessage::BlobMissing {
            hash: hash.to_string(),
        };
        let _ = tx.send(WsMessage::text(serde_json::to_string(&frame).unwrap())).await;
        return;
    }
    let bytes = match ctx.blobs.read(hash, media_type) {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!("blob {hash}: read failed: {e}");
            let frame = WireMessage::BlobMissing {
                hash: hash.to_string(),
            };
            let _ = tx.send(WsMessage::text(serde_json::to_string(&frame).unwrap())).await;
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
        let _ = tx.send(WsMessage::text(serde_json::to_string(&frame).unwrap())).await;
        return;
    }
    let mut offset = 0usize;
    while offset < total {
        let n = std::cmp::min(BLOB_CHUNK_SIZE, total - offset);
        let slice = &bytes[offset..offset + n];
        let data = B64.encode(slice);
        let is_last = offset + n >= total;
        let frame = WireMessage::BlobChunk {
            hash: hash.to_string(),
            media_type: media_type.to_string(),
            offset: offset as u32,
            total_size: total as u32,
            data,
            is_last,
        };
        if tx.send(WsMessage::text(serde_json::to_string(&frame).unwrap())).await.is_err() {
            return;
        }
        offset += n;
    }
}

fn parse_event(value: &Value) -> Option<ChatEventWire> {
    serde_json::from_value::<ChatEventWire>(value.clone()).ok()
}

async fn send_error(tx: &mut WsSink, code: &str, message: &str) {
    let err = WireMessage::Error {
        code: code.to_string(),
        message: message.to_string(),
    };
    let _ = tx
        .send(WsMessage::text(serde_json::to_string(&err).unwrap()))
        .await;
}
