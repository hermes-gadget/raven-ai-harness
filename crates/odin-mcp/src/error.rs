//! Error types specific to MCP operations.

use thiserror::Error;

/// Errors that can occur during MCP client operations.
#[derive(Debug, Error)]
pub enum McpError {
    /// Transport-level IO error.
    #[error("MCP transport error: {0}")]
    Transport(String),

    /// JSON serialization/deserialization error.
    #[error("MCP serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Protocol-level error (JSON-RPC error response).
    #[error("MCP protocol error (code {code}): {message}")]
    Protocol {
        /// JSON-RPC error code.
        code: i64,
        /// Error message.
        message: String,
    },

    /// Connection error.
    #[error("MCP connection error: {0}")]
    Connection(String),

    /// Timeout error.
    #[error("MCP timeout: {0}")]
    Timeout(String),

    /// Tool execution error.
    #[error("MCP tool error ({tool}): {message}")]
    Tool {
        /// Name of the tool that failed.
        tool: String,
        /// Error message.
        message: String,
    },

    /// Invalid response from server.
    #[error("MCP invalid response: {0}")]
    InvalidResponse(String),

    /// Server already initialized.
    #[error("MCP server already initialized")]
    AlreadyInitialized,

    /// Server not initialized.
    #[error("MCP server not initialized")]
    NotInitialized,
}

/// Convenience result type for MCP operations.
pub type McpResult<T> = Result<T, McpError>;
