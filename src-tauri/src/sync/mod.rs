pub mod auth;
pub mod blob;
pub mod client;
pub mod clock;
pub mod mdns;
pub mod outbound;
pub mod protection;
pub mod protection_keystore;
pub mod server;
pub mod wire;

pub use blob::BlobStore;
pub use client::{run_peer_supervisor, PeerSyncDeps};
pub use clock::DesktopClock;
pub use mdns::MdnsRegistration;
pub use outbound::{author_event, OutboundEvent, OutboundTx};
pub use protection_keystore::ProtectionKeyStore;
pub use server::{run as run_server, ServerContext, SyncEvent};

use crate::model::event::ChatEventWire;

/// Protected-chat catch-up: gather the `supersedes` pointers carried by encrypted
/// events (the eventIds of plaintext message events they replace) and return the
/// log with those superseded events removed, or `None` if nothing is superseded so
/// the caller can skip the rewrite. The pointer lives in the cleartext envelope, so
/// this needs no key — a locked desktop still scrubs its plaintext at rest. Shared
/// by both the inbound server path and the outbound client path.
pub fn scrub_superseded(events: &[ChatEventWire]) -> Option<Vec<ChatEventWire>> {
    let mut superseded: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for e in events {
        if e.payload.get("enc").is_some() {
            if let Some(id) = e.payload.get("supersedes").and_then(|v| v.as_str()) {
                superseded.insert(id);
            }
        }
    }
    if superseded.is_empty() {
        return None;
    }
    let kept: Vec<ChatEventWire> = events
        .iter()
        .filter(|e| !superseded.contains(e.event_id.as_str()))
        .cloned()
        .collect();
    if kept.len() == events.len() {
        None
    } else {
        Some(kept)
    }
}
