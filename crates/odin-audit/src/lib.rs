//! odin-audit — Audit logging for the Odin harness.
//!
//! Provides an [`AuditLogger`] implementation that writes structured
//! audit entries to file and/or SQLite databases, supporting queries
//! by agent, session, and event type.

pub mod logger;

pub use logger::AuditLoggerImpl;
