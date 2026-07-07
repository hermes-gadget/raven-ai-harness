//! Odin Gateway — External API layer for the Raven harness.
//!
//! Provides HTTP, Discord, and WebSocket interfaces for interacting
//! with the Raven agent system.

pub mod discord;
pub mod http;
pub mod ws;

pub use http::{ChatRequest, ChatResponse, GatewayState, TaskHandlerFn, run_http_server};
