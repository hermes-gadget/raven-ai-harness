//! Odin Memory — Persistent memory storage for AI agents.
//!
//! This crate provides a SQLite-backed implementation of the
//! [`MemoryStore`] trait defined in `odin-core`. Agents can use
//! this to remember facts, preferences, entities, and patterns
//! across sessions.
//!
//! # Quick Start
//!
//! ```rust,no_run
//! use odin_core::{traits::MemoryStore, MemoryCategory, MemoryEntry};
//! use odin_memory::SqliteMemoryStore;
//! use chrono::Utc;
//! use uuid::Uuid;
//!
//! # async fn example() {
//! let store = SqliteMemoryStore::new("agent_memory.db").unwrap();
//!
//! let entry = MemoryEntry {
//!     id: Uuid::new_v4().to_string(),
//!     content: "Alice prefers dark mode.".to_string(),
//!     category: MemoryCategory::Preference,
//!     created_at: Utc::now(),
//!     updated_at: Utc::now(),
//!     tags: vec!["alice".to_string(), "ui".to_string()],
//!     importance: 0.8,
//! };
//!
//! store.store(entry).await.unwrap();
//! # }
//! ```

pub mod models;
pub mod store;

pub use store::SqliteMemoryStore;
