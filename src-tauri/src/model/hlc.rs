use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

/// Hybrid Logical Clock matching the Kotlin/JSON shape from the sync protocol.
/// Field names must round-trip with the phone's serialization.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HlcTimestamp {
    #[serde(rename = "physicalMs")]
    pub physical_ms: i64,
    pub logical: i32,
    #[serde(rename = "nodeId")]
    pub node_id: String,
}

impl PartialOrd for HlcTimestamp {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for HlcTimestamp {
    fn cmp(&self, other: &Self) -> Ordering {
        self.physical_ms
            .cmp(&other.physical_ms)
            .then_with(|| self.logical.cmp(&other.logical))
            .then_with(|| self.node_id.cmp(&other.node_id))
    }
}

impl HlcTimestamp {
    pub fn zero(node_id: impl Into<String>) -> Self {
        Self {
            physical_ms: 0,
            logical: 0,
            node_id: node_id.into(),
        }
    }

    /// Generate the next local timestamp given the previous local clock and current
    /// wall-clock. Mirrors `HlcTimestamp.tick` on the phone (Kulkarni HLC).
    pub fn tick(previous: Option<&HlcTimestamp>, now_ms: i64, node_id: &str) -> Self {
        let prev_p = previous.map(|p| p.physical_ms).unwrap_or(0);
        let prev_l = previous.map(|p| p.logical).unwrap_or(0);
        if now_ms > prev_p {
            HlcTimestamp {
                physical_ms: now_ms,
                logical: 0,
                node_id: node_id.to_string(),
            }
        } else {
            HlcTimestamp {
                physical_ms: prev_p,
                logical: prev_l + 1,
                node_id: node_id.to_string(),
            }
        }
    }

    /// Merge a remote timestamp into the local clock and return the new local timestamp.
    /// Mirrors `HlcTimestamp.receive` on the phone.
    pub fn receive(
        local: Option<&HlcTimestamp>,
        remote: &HlcTimestamp,
        now_ms: i64,
        node_id: &str,
    ) -> Self {
        let l_p = local.map(|l| l.physical_ms).unwrap_or(0);
        let l_l = local.map(|l| l.logical).unwrap_or(0);
        let max_p = now_ms.max(l_p).max(remote.physical_ms);
        let logical = if max_p == l_p && max_p == remote.physical_ms {
            l_l.max(remote.logical) + 1
        } else if max_p == l_p {
            l_l + 1
        } else if max_p == remote.physical_ms {
            remote.logical + 1
        } else {
            0
        };
        HlcTimestamp {
            physical_ms: max_p,
            logical,
            node_id: node_id.to_string(),
        }
    }
}
