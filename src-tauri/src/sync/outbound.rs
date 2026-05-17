use std::sync::Arc;

use anyhow::Result;
use tokio::sync::broadcast;

use crate::model::event::{ChatEventWire, WIRE_VERSION};
use crate::storage::{now_ms, EventStore};

use super::clock::DesktopClock;
use super::server::SyncEvent;

/// Wrapper around `ChatEventWire` so the broadcast channel type is unambiguous and we
/// can grow the envelope later (e.g. add ack-required flags) without touching every
/// subscriber.
#[derive(Debug, Clone)]
pub struct OutboundEvent(pub ChatEventWire);

pub type OutboundTx = broadcast::Sender<OutboundEvent>;

/// Author a desktop-originated event end-to-end:
/// 1. tick the desktop's HLC,
/// 2. build a fully-formed `ChatEventWire` with our `deviceId` as `originDevice`,
/// 3. persist to the local event log (idempotent dedup by `eventId`),
/// 4. broadcast to every live phone connection,
/// 5. emit a frontend-visible bus event so the UI refreshes.
///
/// Returns the appended event so callers can hand the new state to the frontend
/// without a separate round-trip.
pub fn author_event(
    clock: &DesktopClock,
    store: &Arc<EventStore>,
    outbound: &OutboundTx,
    bus: &broadcast::Sender<SyncEvent>,
    origin_device: &str,
    chat_id: &str,
    op_type: &str,
    payload: serde_json::Value,
) -> Result<ChatEventWire> {
    let hlc = clock.tick(now_ms());
    let event_id = uuid::Uuid::new_v4().to_string();
    // Profile sync: a desktop-authored event must carry the same profile as the chat
    // it targets, so it groups correctly and survives a round-trip to the phone.
    // Inherit it from the chat's existing events (latest non-empty profile stamp).
    let (profile_id, profile_name) = chat_profile(store, chat_id);
    let wire = ChatEventWire {
        v: WIRE_VERSION,
        op_type: op_type.to_string(),
        event_id: event_id.clone(),
        hlc,
        origin_device: origin_device.to_string(),
        chat_id: chat_id.to_string(),
        profile_id,
        profile_name,
        payload,
    };
    store.append(&wire)?;
    let _ = outbound.send(OutboundEvent(wire.clone()));
    let _ = bus.send(SyncEvent::EventAuthored {
        chat_id: chat_id.to_string(),
        event_id,
    });
    Ok(wire)
}

/// The chat's profile (id, name) per its highest-HLC event that carries a non-empty
/// profile stamp. `("", "")` if the chat has no events yet or none are stamped
/// (pre-profile-sync) — those land in the receiver's default bucket.
fn chat_profile(store: &Arc<EventStore>, chat_id: &str) -> (String, String) {
    let events = match store.read_chat(chat_id) {
        Ok(e) => e,
        Err(_) => return (String::new(), String::new()),
    };
    events
        .into_iter()
        .filter(|e| !e.profile_id.is_empty())
        .max_by(|a, b| a.hlc.cmp(&b.hlc))
        .map(|e| (e.profile_id, e.profile_name))
        .unwrap_or_default()
}
