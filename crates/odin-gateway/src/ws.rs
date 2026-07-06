//! WebSocket integration stub.
//!
//! This module provides scaffolding for real-time bidirectional
//! communication via WebSockets. The full implementation will handle:
//! - Persistent connections for streaming responses
//! - JSON message protocol for task submission and results
//! - Ping/pong keepalive
//! - Connection pooling

use odin_core::error::OdinResult;

/// Configuration for the WebSocket gateway.
#[derive(Debug, Clone)]
pub struct WsConfig {
    /// Whether the WebSocket gateway is enabled.
    pub enabled: bool,

    /// Listen address.
    pub addr: String,

    /// Max connections.
    pub max_connections: usize,
}

impl Default for WsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            addr: "127.0.0.1:9178".into(),
            max_connections: 100,
        }
    }
}

/// WebSocket message types.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WsMessage {
    /// Message type (e.g., "task", "result", "ping", "error").
    pub msg_type: String,

    /// Payload as a JSON value.
    #[serde(default)]
    pub payload: serde_json::Value,

    /// Optional correlation ID for request-response.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,
}

/// A stub for the WebSocket gateway.
#[derive(Debug, Clone)]
pub struct WsGateway {
    /// Configuration.
    pub config: WsConfig,
}

impl WsGateway {
    /// Create a new WebSocket gateway.
    pub fn new(config: WsConfig) -> Self {
        Self { config }
    }

    /// Start the WebSocket server (stub).
    ///
    /// In production, this would bind to the configured address and
    /// accept WebSocket connections, handling the message protocol.
    pub async fn start(&self) -> OdinResult<()> {
        if !self.config.enabled {
            tracing::info!("[WS] Gateway disabled");
            return Ok(());
        }

        tracing::info!(
            "[WS] Gateway would start on {} (max {} connections)",
            self.config.addr,
            self.config.max_connections
        );

        // TODO: Start WebSocket server
        // - Bind to configured address
        // - Accept connections with AXUM WS support
        // - Manage connection pool
        // - Route messages to runtime

        Ok(())
    }

    /// Stop the WebSocket gateway (stub).
    pub async fn stop(&self) -> OdinResult<()> {
        tracing::info!("[WS] Gateway would stop");
        // TODO: Gracefully close all connections
        Ok(())
    }

    /// Broadcast a message to all connected clients (stub).
    pub async fn broadcast(&self, _message: &WsMessage) -> OdinResult<()> {
        tracing::info!("[WS] Would broadcast message type '{}'", _message.msg_type);
        // TODO: Send to all connected clients
        Ok(())
    }

    /// Get the number of connected clients (stub).
    pub fn connection_count(&self) -> usize {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ws_config_default() {
        let config = WsConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.addr, "127.0.0.1:9178");
    }

    #[test]
    fn test_ws_gateway_creation() {
        let gateway = WsGateway::new(WsConfig::default());
        assert_eq!(gateway.connection_count(), 0);
    }

    #[tokio::test]
    async fn test_start_disabled_gateway() {
        let gateway = WsGateway::new(WsConfig::default());
        let result = gateway.start().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_stop_gateway() {
        let gateway = WsGateway::new(WsConfig::default());
        let result = gateway.stop().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_broadcast() {
        let gateway = WsGateway::new(WsConfig::default());
        let msg = WsMessage {
            msg_type: "ping".into(),
            payload: serde_json::json!({"ts": 12345}),
            correlation_id: None,
        };
        let result = gateway.broadcast(&msg).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_ws_message_serde() {
        let msg = WsMessage {
            msg_type: "task".into(),
            payload: serde_json::json!({"goal": "write code"}),
            correlation_id: Some("req-1".into()),
        };

        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: WsMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.msg_type, "task");
        assert_eq!(
            deserialized.payload["goal"],
            serde_json::json!("write code")
        );
        assert_eq!(deserialized.correlation_id, Some("req-1".into()));
    }
}
