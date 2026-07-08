//! MCP protocol types — JSON-RPC 2.0 structures and MCP-specific tool definitions.

use serde::{Deserialize, Serialize};

// ── JSON-RPC 2.0 Types ───────────────────────────────────────────────

/// A JSON-RPC 2.0 request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: u64,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

impl JsonRpcRequest {
    /// Create a new JSON-RPC request.
    pub fn new(id: u64, method: impl Into<String>, params: Option<serde_json::Value>) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            method: method.into(),
            params,
        }
    }
}

/// A JSON-RPC 2.0 response (success).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: u64,
    pub result: Option<serde_json::Value>,
    pub error: Option<JsonRpcError>,
}

/// A JSON-RPC 2.0 error object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

// ── MCP Protocol Types ───────────────────────────────────────────────

/// A tool definition discovered from an MCP server (returned by tools/list).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolDef {
    /// The tool's unique name.
    pub name: String,
    /// Human-readable description.
    pub description: String,
    /// JSON Schema for the tool's input parameters.
    #[serde(default, rename = "inputSchema")]
    pub input_schema: serde_json::Value,
}

/// Result of calling an MCP tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolResult {
    /// The content returned by the tool (list of content items).
    #[serde(default)]
    pub content: Vec<McpContentItem>,
    /// Whether the tool execution resulted in an error.
    #[serde(default, rename = "isError")]
    pub is_error: bool,
}

/// A single content item in an MCP tool result.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum McpContentItem {
    /// Text content.
    #[serde(rename = "text")]
    Text {
        /// The text value.
        text: String,
    },
    /// Resource content (e.g., file contents).
    #[serde(rename = "resource")]
    Resource {
        /// The resource URI.
        uri: String,
        /// Optional MIME type.
        #[serde(skip_serializing_if = "Option::is_none", rename = "mimeType")]
        mime_type: Option<String>,
        /// The resource content.
        text: Option<String>,
        /// Optional binary content (base64).
        #[serde(skip_serializing_if = "Option::is_none")]
        blob: Option<String>,
    },
}

// ── MCP Initialize / Server Info ─────────────────────────────────────

/// MCP server capabilities (from initialize response).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerCapabilities {
    /// Whether the server supports tool listing.
    #[serde(default)]
    pub tools: Option<McpToolsCapability>,
    /// Whether the server supports resource discovery.
    #[serde(default)]
    pub resources: Option<McpResourcesCapability>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolsCapability {
    /// Whether the server supports tool list notifications.
    #[serde(default)]
    pub list_changed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResourcesCapability {
    /// Whether the server supports resource subscription.
    #[serde(default)]
    pub subscribe: bool,
    /// Whether the server supports resource list notifications.
    #[serde(default)]
    pub list_changed: bool,
}

/// The initialize result from an MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpInitializeResult {
    /// Protocol version the server uses.
    #[serde(rename = "protocolVersion")]
    pub protocol_version: String,
    /// Server capabilities.
    pub capabilities: McpServerCapabilities,
    /// Server implementation info.
    #[serde(rename = "serverInfo")]
    pub server_info: McpServerInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerInfo {
    pub name: String,
    pub version: String,
}

// ── Tool List Result ─────────────────────────────────────────────────

/// The result of a tools/list call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolListResult {
    pub tools: Vec<McpToolDef>,
}

// ── Tool Call Request ────────────────────────────────────────────────

/// Parameters for a tools/call request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolCallParams {
    pub name: String,
    pub arguments: serde_json::Value,
}

impl McpToolCallParams {
    pub fn new(name: impl Into<String>, arguments: serde_json::Value) -> Self {
        Self {
            name: name.into(),
            arguments,
        }
    }
}
