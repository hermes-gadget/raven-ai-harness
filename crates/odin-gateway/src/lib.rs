//! Odin Gateway — External API layer for the Raven harness.
//!
//! Provides HTTP, Discord, and WebSocket interfaces for interacting
//! with the Raven agent system.

pub mod discord;
pub mod http;
pub mod ws;

pub use discord::DiscordConfig;
pub use discord::DiscordGateway;
pub use http::{
    ChatRequest, ChatResponse, DoctorReportResponse, GatewayState, HealthDependencies,
    HealthResponse, LockSummary, MetricsResponse, OrchestrateRequest, OrchestrateResponse,
    OrchestrateStatusResponse, OrchestrateTaskInfo, TaskHandlerFn, ToolInfo,
    ToolsListResponse, ValidationReportResponse, build_router, run_http_server,
};
