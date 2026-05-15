use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMetadata {
    pub id: String,
    pub name: String,
    #[serde(default, rename = "avatarFile")]
    pub avatar_file: Option<String>,
    #[serde(default, rename = "backgroundFile")]
    pub background_file: Option<String>,
    #[serde(rename = "createdAt")]
    pub created_at: i64,
    #[serde(default, rename = "isPinned")]
    pub is_pinned: bool,
    #[serde(default, rename = "bubbleColor")]
    pub bubble_color: Option<String>,
    #[serde(default, rename = "bubbleGradient")]
    pub bubble_gradient: bool,
    #[serde(default, rename = "isArchived")]
    pub is_archived: bool,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default, rename = "isProtected")]
    pub is_protected: bool,
    #[serde(default, rename = "hideName")]
    pub hide_name: bool,
    /// Phase 3: per-chat random salt for PBKDF2-derived encryption key. Plaintext on
    /// the wire (salts aren't secrets); only the PIN is. `None` for non-protected
    /// chats and for pre-phase-3 chats authored before the field existed.
    #[serde(default, rename = "protectionSalt")]
    pub protection_salt: Option<String>,
}
