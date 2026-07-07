use serde::{Deserialize, Serialize};

use super::richtext::RichTextBlock;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MessageType {
    #[serde(rename = "TEXT")]
    Text,
    #[serde(rename = "VOICE")]
    Voice,
    #[serde(rename = "IMAGE")]
    Image,
    #[serde(rename = "CHECKLIST")]
    Checklist,
}

impl Default for MessageType {
    fn default() -> Self {
        MessageType::Text
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChecklistItem {
    pub id: String,
    pub text: String,
    #[serde(default)]
    pub checked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    #[serde(rename = "chatId")]
    pub chat_id: String,
    #[serde(default)]
    pub content: Vec<RichTextBlock>,
    #[serde(default, rename = "voiceFile")]
    pub voice_file: Option<String>,
    #[serde(default, rename = "voiceDurationMs")]
    pub voice_duration_ms: Option<i64>,
    #[serde(rename = "createdAt")]
    pub created_at: i64,
    #[serde(default, rename = "editedAt")]
    pub edited_at: Option<i64>,
    #[serde(default, rename = "type")]
    pub message_type: MessageType,
    #[serde(default, rename = "isStarred")]
    pub is_starred: bool,
    #[serde(default, rename = "imageFiles")]
    pub image_files: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default, rename = "reminderAt")]
    pub reminder_at: Option<i64>,
    #[serde(default, rename = "checklistItems")]
    pub checklist_items: Vec<ChecklistItem>,
    /// Emoji reactions attached to this message (ordered, no duplicates,
    /// toggle-on-repeat). Rides inside the sealed body for protected chats
    /// via the same [`MessageEdited`] path as star/reminder.
    #[serde(default)]
    pub reactions: Vec<String>,
    /// Whisper.cpp transcript for voice messages (`None` for other types or
    /// when transcription hasn't run yet). Symmetric sync: any device that
    /// transcribes republishes a `MessageEdited`; peers pick it up without
    /// re-transcribing. Sealed with the body in protected chats.
    #[serde(default)]
    pub transcript: Option<String>,
}
