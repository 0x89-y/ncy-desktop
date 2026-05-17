use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::chat::ChatMetadata;
use super::hlc::HlcTimestamp;
use super::message::Message;

pub const WIRE_VERSION: u32 = 1;

/// Wire/disk shape of a single chat event. Mirrors `ChatEventCodec` in the Kotlin code.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatEventWire {
    pub v: u32,
    #[serde(rename = "type")]
    pub op_type: String,
    #[serde(rename = "eventId")]
    pub event_id: String,
    pub hlc: HlcTimestamp,
    #[serde(rename = "originDevice")]
    pub origin_device: String,
    #[serde(rename = "chatId")]
    pub chat_id: String,
    /// Profile sync: which phone profile this chat belongs to + its display name.
    /// Defaults to empty for pre-profile-sync events (a single "default" bucket).
    #[serde(default, rename = "profileId")]
    pub profile_id: String,
    #[serde(default, rename = "profileName")]
    pub profile_name: String,
    pub payload: Value,
}

/// Logical, parsed event used by the materializer. Discriminant is `op_type`.
#[derive(Debug, Clone)]
pub struct ChatEvent {
    pub event_id: String,
    pub hlc: HlcTimestamp,
    pub origin_device: String,
    pub chat_id: String,
    pub op: ChatEventOp,
}

#[derive(Debug, Clone)]
pub enum ChatEventOp {
    MessageCreated(Message),
    MessageEdited(Message),
    MessageTombstoned(String),
    ChatMetadataChanged(ChatMetadata),
    ChatTombstoned(String),
}

pub mod op_type {
    pub const MESSAGE_CREATED: &str = "MessageCreated";
    pub const MESSAGE_EDITED: &str = "MessageEdited";
    pub const MESSAGE_TOMBSTONED: &str = "MessageTombstoned";
    pub const CHAT_METADATA_CHANGED: &str = "ChatMetadataChanged";
    pub const CHAT_TOMBSTONED: &str = "ChatTombstoned";
}

impl ChatEvent {
    pub fn from_wire(wire: ChatEventWire) -> anyhow::Result<Self> {
        let op = match wire.op_type.as_str() {
            op_type::MESSAGE_CREATED => {
                ChatEventOp::MessageCreated(serde_json::from_value(wire.payload)?)
            }
            op_type::MESSAGE_EDITED => {
                ChatEventOp::MessageEdited(serde_json::from_value(wire.payload)?)
            }
            op_type::MESSAGE_TOMBSTONED => {
                let id = wire
                    .payload
                    .get("messageId")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("MessageTombstoned missing messageId"))?
                    .to_string();
                ChatEventOp::MessageTombstoned(id)
            }
            op_type::CHAT_METADATA_CHANGED => {
                ChatEventOp::ChatMetadataChanged(serde_json::from_value(wire.payload)?)
            }
            op_type::CHAT_TOMBSTONED => {
                let id = wire
                    .payload
                    .get("chatId")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("ChatTombstoned missing chatId"))?
                    .to_string();
                ChatEventOp::ChatTombstoned(id)
            }
            other => anyhow::bail!("unknown op type: {other}"),
        };
        Ok(Self {
            event_id: wire.event_id,
            hlc: wire.hlc,
            origin_device: wire.origin_device,
            chat_id: wire.chat_id,
            op,
        })
    }
}
