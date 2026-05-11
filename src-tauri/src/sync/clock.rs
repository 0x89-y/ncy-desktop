use std::sync::Mutex;

use crate::model::HlcTimestamp;

/// Desktop's own HLC clock state. Used both to author outbound events (`tick`) and to
/// absorb the HLCs of inbound events (`receive`) so we always stay causally ahead.
///
/// Initialized at startup from the max HLC across all stored events — that way on a
/// fresh process the clock can't accidentally produce an HLC that's older than
/// something already on disk.
pub struct DesktopClock {
    inner: Mutex<Option<HlcTimestamp>>,
    node_id: String,
}

impl DesktopClock {
    pub fn new(node_id: String, initial: Option<HlcTimestamp>) -> Self {
        Self {
            inner: Mutex::new(initial),
            node_id,
        }
    }

    pub fn tick(&self, now_ms: i64) -> HlcTimestamp {
        let mut g = self.inner.lock().unwrap();
        let next = HlcTimestamp::tick(g.as_ref(), now_ms, &self.node_id);
        *g = Some(next.clone());
        next
    }

    pub fn receive(&self, remote: &HlcTimestamp, now_ms: i64) {
        let mut g = self.inner.lock().unwrap();
        let next = HlcTimestamp::receive(g.as_ref(), remote, now_ms, &self.node_id);
        *g = Some(next);
    }
}
