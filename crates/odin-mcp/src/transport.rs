//! Transport layer for MCP communication.
//!
//! Provides a `McpTransport` trait and a `StdioTransport` implementation
//! that communicates with an MCP server subprocess via stdin/stdout
//! using JSON-RPC 2.0 messages.

use crate::error::{McpError, McpResult};
use crate::types::{JsonRpcRequest, JsonRpcResponse};
use async_trait::async_trait;
use serde_json::Value;
use std::io::{BufReader, Read, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::Mutex;

/// Transport abstraction for MCP communication.
#[async_trait]
pub trait McpTransport: Send + Sync {
    /// Send a JSON-RPC request and await the response.
    async fn send_request(&self, method: &str, params: Option<Value>)
    -> McpResult<JsonRpcResponse>;

    /// Close the transport.
    async fn close(&mut self) -> McpResult<()>;
}

/// STDIO transport for MCP servers launched as child processes.
///
/// Spawns a process (e.g., `npx @modelcontextprotocol/server-filesystem`),
/// sends JSON-RPC 2.0 messages over stdin, and reads responses from stdout.
pub struct StdioTransport {
    /// The child process handle.
    child: Mutex<Option<Child>>,
    /// Writer to the child's stdin.
    stdin: Mutex<Option<ChildStdin>>,
    /// Unique request ID counter.
    request_id: AtomicU64,
    /// The command used to spawn the child.
    command: String,
    /// Arguments to the command.
    args: Vec<String>,
    /// Environment variables added to the child process.
    env: std::collections::HashMap<String, String>,
}

impl StdioTransport {
    /// Create a new StdioTransport configuration.
    ///
    /// The process is not spawned until [`connect`](#method.connect) is called.
    pub fn new(command: impl Into<String>, args: Vec<String>) -> Self {
        Self {
            child: Mutex::new(None),
            stdin: Mutex::new(None),
            request_id: AtomicU64::new(1),
            command: command.into(),
            args,
            env: std::collections::HashMap::new(),
        }
    }

    /// Add configured environment variables to the MCP child process.
    pub fn with_env(mut self, env: std::collections::HashMap<String, String>) -> Self {
        self.env = env;
        self
    }

    /// Spawn the child process and open communication channels.
    pub async fn connect(&self) -> McpResult<()> {
        let mut child = Command::new(&self.command)
            .args(&self.args)
            .envs(&self.env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| {
                McpError::Connection(format!(
                    "Failed to spawn MCP server '{}': {}",
                    self.command, e
                ))
            })?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| McpError::Connection("Failed to capture child stdin".into()))?;

        *self.child.lock().await = Some(child);
        *self.stdin.lock().await = Some(stdin);

        Ok(())
    }

    /// Read a single JSON-RPC response from the child's stdout.
    ///
    /// This reads one full line (newline-delimited JSON) from stdout
    /// and parses it as a `JsonRpcResponse`.
    async fn read_response(&self) -> McpResult<JsonRpcResponse> {
        let mut child_guard = self.child.lock().await;
        let child = child_guard
            .as_mut()
            .ok_or_else(|| McpError::Connection("Not connected".into()))?;

        // Take ownership of stdout temporarily to create a buffered reader
        let mut owned_stdout = child
            .stdout
            .take()
            .ok_or_else(|| McpError::Connection("No stdout available".into()))?;

        // Read one line (the MCP protocol uses newline-delimited JSON)
        use std::io::BufRead;
        let mut reader = BufReader::new(&mut owned_stdout);
        let mut line = String::new();
        reader
            .read_line(&mut line)
            .map_err(|e| McpError::Transport(format!("Failed to read from child stdout: {e}")))?;

        // Put stdout back
        child.stdout = Some(owned_stdout);

        if line.is_empty() {
            // Check stderr for error messages
            if let Some(stderr) = child.stderr.as_mut() {
                let mut err_buf = String::new();
                let mut err_reader = BufReader::new(stderr.by_ref());
                let _ = err_reader.read_line(&mut err_buf);
                if !err_buf.is_empty() {
                    return Err(McpError::Transport(format!(
                        "MCP server stderr: {}",
                        err_buf.trim()
                    )));
                }
            }
            return Err(McpError::Transport("MCP server closed connection".into()));
        }

        serde_json::from_str(&line).map_err(|e| {
            McpError::InvalidResponse(format!("Failed to parse JSON-RPC response: {e}"))
        })
    }
}

#[async_trait]
impl McpTransport for StdioTransport {
    async fn send_request(
        &self,
        method: &str,
        params: Option<Value>,
    ) -> McpResult<JsonRpcResponse> {
        let id = self.request_id.fetch_add(1, Ordering::SeqCst);
        let request = JsonRpcRequest::new(id, method, params);

        let request_str = serde_json::to_string(&request).map_err(McpError::Serialization)?;

        {
            let mut stdin_guard = self.stdin.lock().await;
            let stdin = stdin_guard
                .as_mut()
                .ok_or_else(|| McpError::Connection("Not connected".into()))?;

            // Write the JSON-RPC request as a single line (required by MCP transport)
            writeln!(stdin, "{}", request_str)
                .map_err(|e| McpError::Transport(format!("Failed to write to child stdin: {e}")))?;
            stdin
                .flush()
                .map_err(|e| McpError::Transport(format!("Failed to flush child stdin: {e}")))?;
        }

        // Read the response
        self.read_response().await
    }

    async fn close(&mut self) -> McpResult<()> {
        let mut child_guard = self.child.lock().await;
        *self.stdin.lock().await = None;

        if let Some(mut child) = child_guard.take() {
            // Try graceful shutdown, then kill
            let _ = child.kill();
            let _ = child.wait();
        }
        Ok(())
    }
}

impl Drop for StdioTransport {
    fn drop(&mut self) {
        // Best-effort cleanup in synchronous context
        if let Ok(mut child_guard) = self.child.try_lock()
            && let Some(mut child) = child_guard.take()
        {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

// ── Mock Transport (for testing) ─────────────────────────────────────

/// A mock transport for unit testing McpClient without a real subprocess.
pub struct MockTransport {
    /// Pre-configured responses keyed by method name.
    responses: std::collections::HashMap<String, McpResult<JsonRpcResponse>>,
    /// Record of sent requests for assertion.
    sent_requests: std::sync::Mutex<Vec<(String, Option<Value>)>>,
    /// Whether the transport is "connected".
    connected: std::sync::atomic::AtomicBool,
}

impl MockTransport {
    /// Create a new mock transport with the given method→response map.
    pub fn new(responses: std::collections::HashMap<String, McpResult<JsonRpcResponse>>) -> Self {
        Self {
            responses,
            sent_requests: std::sync::Mutex::new(Vec::new()),
            connected: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Mark the transport as connected.
    pub fn set_connected(&self, connected: bool) {
        self.connected
            .store(connected, std::sync::atomic::Ordering::SeqCst);
    }

    /// Get the list of requests that were sent through this transport.
    pub fn sent_requests(&self) -> Vec<(String, Option<Value>)> {
        self.sent_requests.lock().unwrap().clone()
    }
}

#[async_trait]
impl McpTransport for MockTransport {
    async fn send_request(
        &self,
        method: &str,
        params: Option<Value>,
    ) -> McpResult<JsonRpcResponse> {
        self.sent_requests
            .lock()
            .unwrap()
            .push((method.to_string(), params));

        if let Some(response) = self.responses.get(method) {
            match response {
                Ok(r) => Ok(r.clone()),
                Err(e) => Err(match e {
                    McpError::Protocol { code, message } => McpError::Protocol {
                        code: *code,
                        message: message.clone(),
                    },
                    _ => McpError::Transport(e.to_string()),
                }),
            }
        } else {
            Err(McpError::Protocol {
                code: -32601,
                message: format!("Method not found: {method}"),
            })
        }
    }

    async fn close(&mut self) -> McpResult<()> {
        self.connected
            .store(false, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }
}
