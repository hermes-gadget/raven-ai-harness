//! Session — tracks a single conversation session with its messages and metadata.

use chrono::{DateTime, Utc};
use odin_core::types::{Message, SessionId};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A conversation session.
///
/// Sessions group messages belonging to one logical conversation.
/// Each session has a unique ID and tracks when it was created and
/// last updated.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// Unique session identifier.
    pub id: SessionId,

    /// Human-readable label (e.g., "code-review-42").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,

    /// The messages in this session.
    #[serde(default)]
    pub messages: Vec<Message>,

    /// When the session was created.
    pub created_at: DateTime<Utc>,

    /// When the session was last updated.
    pub updated_at: DateTime<Utc>,

    /// Arbitrary metadata key-value pairs.
    #[serde(default)]
    pub metadata: std::collections::HashMap<String, String>,
}

impl Session {
    /// Create a new empty session.
    pub fn new() -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            label: None,
            messages: Vec::new(),
            created_at: now,
            updated_at: now,
            metadata: std::collections::HashMap::new(),
        }
    }

    /// Create a new session with a label.
    pub fn with_label(label: impl Into<String>) -> Self {
        let mut session = Self::new();
        session.label = Some(label.into());
        session
    }

    /// Add a message to this session.
    pub fn add_message(&mut self, message: Message) {
        self.messages.push(message);
        self.updated_at = Utc::now();
    }

    /// Get the number of messages in this session.
    pub fn message_count(&self) -> usize {
        self.messages.len()
    }

    /// Get an iterator over the messages.
    pub fn iter_messages(&self) -> impl Iterator<Item = &Message> {
        self.messages.iter()
    }

    /// Clear all messages (keeps the session).
    pub fn clear_messages(&mut self) {
        self.messages.clear();
        self.updated_at = Utc::now();
    }

    /// Get messages since a given index.
    pub fn messages_since(&self, index: usize) -> &[Message] {
        if index >= self.messages.len() {
            &[]
        } else {
            &self.messages[index..]
        }
    }

    /// Set a metadata key-value pair.
    pub fn set_metadata(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.metadata.insert(key.into(), value.into());
    }

    /// Get a metadata value by key.
    pub fn get_metadata(&self, key: &str) -> Option<&str> {
        self.metadata.get(key).map(|s| s.as_str())
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use odin_core::types::Role;

    #[test]
    fn test_session_creation() {
        let session = Session::new();
        assert!(session.messages.is_empty());
        assert_eq!(session.message_count(), 0);
        // Verify UUID format
        assert_ne!(session.id.to_string(), "");
    }

    #[test]
    fn test_session_with_label() {
        let session = Session::with_label("test-session");
        assert_eq!(session.label.as_deref(), Some("test-session"));
    }

    #[test]
    fn test_add_message() {
        let mut session = Session::new();
        let msg = Message::user("Hello");
        session.add_message(msg);
        assert_eq!(session.message_count(), 1);
        assert!(session.updated_at >= session.created_at);
    }

    #[test]
    fn test_clear_messages() {
        let mut session = Session::new();
        session.add_message(Message::user("Hello"));
        session.add_message(Message::assistant("Hi there"));
        assert_eq!(session.message_count(), 2);
        session.clear_messages();
        assert_eq!(session.message_count(), 0);
    }

    #[test]
    fn test_messages_since() {
        let mut session = Session::new();
        session.add_message(Message::user("A"));
        session.add_message(Message::user("B"));
        session.add_message(Message::user("C"));

        let since = session.messages_since(1);
        assert_eq!(since.len(), 2);
        assert_eq!(since[0].text(), Some("B"));
        assert_eq!(since[1].text(), Some("C"));

        // Out of bounds
        let since = session.messages_since(10);
        assert!(since.is_empty());
    }

    #[test]
    fn test_metadata() {
        let mut session = Session::new();
        session.set_metadata("key1", "value1");
        assert_eq!(session.get_metadata("key1"), Some("value1"));
        assert_eq!(session.get_metadata("nonexistent"), None);
    }

    #[test]
    fn test_role_serde() {
        use serde_json;
        let json = serde_json::to_string(&Role::User).unwrap();
        assert_eq!(json, "\"user\"");
    }
}
