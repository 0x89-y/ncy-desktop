use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::model::HlcTimestamp;

/// Top-level frames over the wire. Discriminated by `type` to match the Kotlin/Python
/// reference encoders.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WireMessage {
    Challenge {
        nonce: String,
    },
    Hello {
        #[serde(rename = "deviceId")]
        device_id: String,
        auth: String,
        #[serde(default, rename = "sinceHlc")]
        since_hlc: Option<HlcTimestamp>,
    },
    Welcome {
        #[serde(rename = "deviceId")]
        device_id: String,
        name: String,
    },
    /// Inbound or outbound event frame. The inner `event` is left as raw JSON so we
    /// can persist it byte-identically to disk and ack its `hlc` field.
    Event {
        event: Value,
    },
    Ack {
        #[serde(rename = "upToHlc")]
        up_to_hlc: HlcTimestamp,
    },
    Error {
        code: String,
        message: String,
    },

    // Phase 2b — content-addressed blob transfer.
    BlobRequest {
        hash: String,
        #[serde(rename = "mediaType")]
        media_type: String,
    },
    BlobChunk {
        hash: String,
        #[serde(rename = "mediaType")]
        media_type: String,
        offset: u32,
        #[serde(rename = "totalSize")]
        total_size: u32,
        /// Base64-encoded payload bytes.
        data: String,
        #[serde(rename = "isLast")]
        is_last: bool,
    },
    BlobMissing {
        hash: String,
    },
}
