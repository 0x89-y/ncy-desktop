//! Phase 3 — in-memory cache of derived AES-256 keys for protected chats, keyed by
//! chatId. Mirror of `ProtectionKeyStore.kt` on the phone. Keys are derived from the
//! user's PIN + the chat's `protectionSalt` when the user unlocks a chat and are
//! cleared on lock — never persisted to disk.

use std::collections::HashMap;
use std::sync::Mutex;

/// 32-byte AES key. Boxed in a fixed array so callers can pass `&[u8; 32]` to
/// `protection::seal`/`open`.
pub type ChatKey = [u8; 32];

#[derive(Default)]
pub struct ProtectionKeyStore {
    keys: Mutex<HashMap<String, ChatKey>>,
}

impl ProtectionKeyStore {
    pub fn new() -> Self {
        Self {
            keys: Mutex::new(HashMap::new()),
        }
    }

    /// Cache `key` for `chat_id`, replacing any existing entry.
    pub fn unlock(&self, chat_id: &str, key: ChatKey) {
        self.keys.lock().unwrap().insert(chat_id.to_string(), key);
    }

    pub fn key_for(&self, chat_id: &str) -> Option<ChatKey> {
        self.keys.lock().unwrap().get(chat_id).copied()
    }

    pub fn is_unlocked(&self, chat_id: &str) -> bool {
        self.keys.lock().unwrap().contains_key(chat_id)
    }

    pub fn lock(&self, chat_id: &str) {
        if let Some(mut k) = self.keys.lock().unwrap().remove(chat_id) {
            k.fill(0);
        }
    }

    #[allow(dead_code)]
    pub fn lock_all(&self) {
        let mut map = self.keys.lock().unwrap();
        for (_, k) in map.iter_mut() {
            k.fill(0);
        }
        map.clear();
    }
}
