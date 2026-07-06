//! HTTP API server using Axum.
//!
//! Provides:
//! - `GET /health` — health check
//! - `POST /chat` — submit a task and receive results
//!
//! The chat endpoint is extensible via a closure handler.

use axum::{
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use odin_core::error::OdinResult;
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio::net::TcpListener;
use tracing;

/// A boxed async handler for processing chat/task requests.
pub type TaskHandlerFn =
    Arc<dyn Fn(ChatRequest) -> Pin<Box<dyn Future<Output = OdinResult<ChatResponse>> + Send>>
        + Send
        + Sync>;

/// Shared state for the HTTP server.
#[derive(Clone)]
pub struct GatewayState {
    /// Optional handler for processing chat/task requests.
    pub task_handler: Option<TaskHandlerFn>,
}

impl Default for GatewayState {
    fn default() -> Self {
        Self { task_handler: None }
    }
}

// ── Request / Response Types ─────────────────────────────────────────

/// Incoming chat or task request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    /// The task goal or user message.
    pub task: String,

    /// Optional context for the task.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,

    /// Optional session ID for continuing a conversation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,

    /// Max iterations for this task.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_iterations: Option<u32>,
}

/// Chat or task response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    /// Whether the task was successful.
    pub success: bool,

    /// Summary of the result.
    pub summary: String,

    /// Number of iterations used.
    pub iterations: u32,

    /// Number of tool calls made.
    pub tool_calls: u32,

    /// Duration in milliseconds.
    pub duration_ms: u64,

    /// Confidence score (0.0 – 1.0).
    pub confidence: f64,

    /// Error message if unsuccessful.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Health check response.
#[derive(Debug, Clone, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub uptime_secs: u64,
}

// ── Route Handlers ───────────────────────────────────────────────────

/// Health check endpoint.
async fn health_handler(start_time: Arc<std::time::Instant>) -> Json<HealthResponse> {
    let uptime = start_time.elapsed().as_secs();
    Json(HealthResponse {
        status: "ok".into(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime_secs: uptime,
    })
}

/// Chat/task endpoint.
async fn chat_handler(
    state: Arc<GatewayState>,
    start_time: Arc<std::time::Instant>,
    Json(request): Json<ChatRequest>,
) -> impl IntoResponse {
    match &state.task_handler {
        Some(handler) => match handler(request).await {
            Ok(response) => (StatusCode::OK, Json(response)).into_response(),
            Err(e) => {
                let error_resp = ChatResponse {
                    success: false,
                    summary: format!("Task execution failed: {e}"),
                    iterations: 0,
                    tool_calls: 0,
                    duration_ms: start_time.elapsed().as_millis() as u64,
                    confidence: 0.0,
                    error: Some(e.to_string()),
                };
                (StatusCode::INTERNAL_SERVER_ERROR, Json(error_resp)).into_response()
            }
        },
        None => {
            let error_resp = ChatResponse {
                success: false,
                summary: "No task handler configured".into(),
                iterations: 0,
                tool_calls: 0,
                duration_ms: 0,
                confidence: 0.0,
                error: Some("No task handler configured".into()),
            };
            (StatusCode::SERVICE_UNAVAILABLE, Json(error_resp)).into_response()
        }
    }
}

// ── Server ───────────────────────────────────────────────────────────

/// Run the HTTP server on the given address.
///
/// The `task_handler` is optional — if provided, it will be called
/// for every `/chat` request. Without one, the endpoint returns 503.
pub async fn run_http_server(
    addr: &str,
    task_handler: Option<TaskHandlerFn>,
) -> OdinResult<()> {
    let state: Arc<GatewayState> = Arc::new(GatewayState { task_handler });
    let start_time = Arc::new(std::time::Instant::now());

    let app = build_router(state, start_time);

    let listener = TcpListener::bind(addr)
        .await
        .map_err(|e| {
            odin_core::error::OdinError::Network(format!("Failed to bind to {addr}: {e}"))
        })?;

    tracing::info!("[GATEWAY] HTTP server listening on {addr}");

    axum::serve(listener, app)
        .await
        .map_err(|e| odin_core::error::OdinError::Network(format!("Server error: {e}")))?;

    Ok(())
}

/// Build the Axum router, useful for embedding in larger apps.
pub fn build_router(
    state: Arc<GatewayState>,
    start_time: Arc<std::time::Instant>,
) -> Router {
    Router::new()
        .route(
            "/health",
            get({
                let st = start_time.clone();
                move || health_handler(st.clone())
            }),
        )
        .route(
            "/chat",
            post({
                let st = state.clone();
                let t0 = start_time.clone();
                move |body| chat_handler(st.clone(), t0.clone(), body)
            }),
        )
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .layer(tower_http::cors::CorsLayer::permissive())
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chat_request_serde() {
        let req = ChatRequest {
            task: "Write a test".into(),
            context: None,
            session_id: None,
            max_iterations: None,
        };

        let json = serde_json::to_string(&req).unwrap();
        let deserialized: ChatRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.task, "Write a test");
    }

    #[test]
    fn test_chat_response_serde() {
        let resp = ChatResponse {
            success: true,
            summary: "Done".into(),
            iterations: 3,
            tool_calls: 5,
            duration_ms: 1000,
            confidence: 0.95,
            error: None,
        };

        let json = serde_json::to_string(&resp).unwrap();
        let deserialized: ChatResponse = serde_json::from_str(&json).unwrap();
        assert!(deserialized.success);
        assert_eq!(deserialized.summary, "Done");
    }

    #[test]
    fn test_health_response_serde() {
        let resp = HealthResponse {
            status: "ok".into(),
            version: "0.1.0".into(),
            uptime_secs: 42,
        };

        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("ok"));
    }

    #[test]
    fn test_gateway_state_default() {
        let state = GatewayState::default();
        assert!(state.task_handler.is_none());
    }

    #[test]
    fn test_build_router_smoke() {
        let state = Arc::new(GatewayState::default());
        let start_time = Arc::new(std::time::Instant::now());
        let _router = build_router(state, start_time);
    }

    #[tokio::test]
    async fn test_handler_function() {
        let handler: TaskHandlerFn = Arc::new(|req: ChatRequest| {
            Box::pin(async move {
                Ok(ChatResponse {
                    success: true,
                    summary: format!("Handled: {}", req.task),
                    iterations: 1,
                    tool_calls: 0,
                    duration_ms: 0,
                    confidence: 1.0,
                    error: None,
                })
            })
        });

        let request = ChatRequest {
            task: "hello".into(),
            context: None,
            session_id: None,
            max_iterations: None,
        };

        let response = handler(request).await.unwrap();
        assert!(response.success);
        assert_eq!(response.summary, "Handled: hello");
    }
}
