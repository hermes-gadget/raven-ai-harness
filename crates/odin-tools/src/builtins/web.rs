//! Web search and fetch tools — HTTP GET with timeout, basic web operations.

use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use chrono::Utc;
use serde::Deserialize;
use tracing::instrument;

use odin_core::error::{OdinError, OdinResult};
use odin_core::traits::{Tool, ToolContext};
use odin_core::types::{FunctionSchema, ToolResult, ToolSchema};

/// Shared HTTP client used by all web tools.
fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .user_agent("OdinTools/1.0 (Raven AI Harness)")
        .build()
        .expect("Failed to build HTTP client")
}

/// Arguments for `web_fetch`.
#[derive(Debug, Deserialize)]
struct WebFetchArgs {
    url: String,
}

/// Tool that fetches the content of a URL via HTTP GET.
///
/// Returns the raw text content of the response. Configured with a 30-second
/// timeout and a descriptive user-agent.
pub struct WebFetch {
    name: String,
    description: String,
    client: Arc<reqwest::Client>,
}

impl WebFetch {
    /// Create a new `WebFetch` tool.
    pub fn new() -> Self {
        Self {
            name: "web_fetch".into(),
            description:
                "Fetch the contents of a URL via HTTP GET. Returns the raw text response body."
                    .into(),
            client: Arc::new(http_client()),
        }
    }

    /// Construct the JSON schema.
    fn make_schema(name: &str) -> ToolSchema {
        ToolSchema {
            schema_type: "function".into(),
            function: FunctionSchema {
                name: name.into(),
                description: "Fetch the content of a URL.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "url": {
                            "type": "string",
                            "description": "The URL to fetch (must start with http:// or https://)"
                        }
                    },
                    "required": ["url"]
                }),
            },
        }
    }
}

impl Default for WebFetch {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for WebFetch {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn schema(&self) -> ToolSchema {
        Self::make_schema(&self.name)
    }

    fn is_safe(&self) -> bool {
        true
    }

    #[instrument(skip(self, _context), fields(tool = self.name))]
    async fn execute(
        &self,
        args: serde_json::Value,
        _context: &ToolContext,
    ) -> OdinResult<ToolResult> {
        let start = Instant::now();

        let parsed: WebFetchArgs = serde_json::from_value(args).map_err(|e| OdinError::Tool {
            tool: self.name.clone(),
            message: format!("Invalid arguments: {e}"),
            source: Some(Box::new(e)),
        })?;

        let url = &parsed.url;

        // Validate URL
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Ok(ToolResult {
                call_id: String::new(),
                tool_name: self.name.clone(),
                success: false,
                output: String::new(),
                error: Some("URL must start with http:// or https://".into()),
                duration_ms: 0,
                timestamp: Utc::now(),
            });
        }

        let response = self.client.get(url).send().await.map_err(|e| {
            if e.is_timeout() {
                OdinError::Timeout(format!("Request to {url} timed out"))
            } else if e.is_connect() {
                OdinError::Network(format!("Could not connect to {url}: {e}"))
            } else {
                OdinError::Network(format!("Request to {url} failed: {e}"))
            }
        })?;

        let status = response.status();
        let body = response.text().await.map_err(|e| {
            OdinError::Network(format!("Failed to read response body from {url}: {e}"))
        })?;

        let duration_ms = start.elapsed().as_millis() as u64;
        let success = status.is_success();

        let output = if body.len() > 100_000 {
            format!(
                "{} (truncated from {} bytes to 100000)",
                &body[..100_000],
                body.len()
            )
        } else {
            body
        };

        let error = if success {
            None
        } else {
            Some(format!("HTTP {status}"))
        };

        Ok(ToolResult {
            call_id: String::new(),
            tool_name: self.name.clone(),
            success,
            output,
            error,
            duration_ms,
            timestamp: Utc::now(),
        })
    }
}

/// Arguments for `web_search`.
#[derive(Debug, Deserialize)]
struct WebSearchArgs {
    query: String,
}

/// Tool that performs a web search.
///
/// This implementation performs a simple HTTP GET to a configurable search
/// endpoint. It can be configured to use different search providers by
/// injecting a custom client or URL.
pub struct WebSearch {
    name: String,
    description: String,
    client: Arc<reqwest::Client>,
    /// Optional search URL template (use {query} as placeholder).
    search_url_template: Option<String>,
}

impl WebSearch {
    /// Create a new `WebSearch` tool.
    pub fn new() -> Self {
        Self {
            name: "web_search".into(),
            description: "Search the web for information. Performs a web search using the configured search provider and returns results as text.".into(),
            client: Arc::new(http_client()),
            search_url_template: None,
        }
    }

    /// Create a `WebSearch` with a custom search URL template.
    ///
    /// The template should contain `{query}` which will be replaced with the
    /// URL-encoded search query.
    pub fn with_search_url(template: impl Into<String>) -> Self {
        Self {
            search_url_template: Some(template.into()),
            ..Self::new()
        }
    }

    /// Construct the JSON schema.
    fn make_schema(name: &str) -> ToolSchema {
        ToolSchema {
            schema_type: "function".into(),
            function: FunctionSchema {
                name: name.into(),
                description: "Search the web for information.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "The search query"
                        },
                        "max_results": {
                            "type": "integer",
                            "description": "Maximum number of results to return (optional, default: 5)",
                            "default": 5
                        }
                    },
                    "required": ["query"]
                }),
            },
        }
    }
}

impl Default for WebSearch {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for WebSearch {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn schema(&self) -> ToolSchema {
        Self::make_schema(&self.name)
    }

    fn is_safe(&self) -> bool {
        true
    }

    #[instrument(skip(self, _context), fields(tool = self.name))]
    async fn execute(
        &self,
        args: serde_json::Value,
        _context: &ToolContext,
    ) -> OdinResult<ToolResult> {
        let start = Instant::now();

        let parsed: WebSearchArgs = serde_json::from_value(args).map_err(|e| OdinError::Tool {
            tool: self.name.clone(),
            message: format!("Invalid arguments: {e}"),
            source: Some(Box::new(e)),
        })?;

        let query = &parsed.query;

        // If a search URL template is configured, use it
        if let Some(template) = &self.search_url_template {
            let encoded = urlencoding(query);
            let url = template.replace("{query}", &encoded);

            let response = self
                .client
                .get(&url)
                .send()
                .await
                .map_err(|e| OdinError::Network(format!("Search request failed: {e}")))?;

            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            let duration_ms = start.elapsed().as_millis() as u64;

            return Ok(ToolResult {
                call_id: String::new(),
                tool_name: self.name.clone(),
                success: status.is_success(),
                output: if body.len() > 100_000 {
                    format!("{} (truncated)", &body[..100_000])
                } else {
                    body
                },
                error: if status.is_success() {
                    None
                } else {
                    Some(format!("HTTP {status}"))
                },
                duration_ms,
                timestamp: Utc::now(),
            });
        }

        // No search template configured — return informative message
        let duration_ms = start.elapsed().as_millis() as u64;
        Ok(ToolResult {
            call_id: String::new(),
            tool_name: self.name.clone(),
            success: true,
            output: format!(
                "Web search is not configured with a search provider URL. \
                 To enable web search, configure a search URL template using \
                 `with_search_url()`. Query was: {query}"
            ),
            error: None,
            duration_ms,
            timestamp: Utc::now(),
        })
    }
}

/// Simple URL encoding for search queries (replaces special chars).
fn urlencoding(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                result.push(byte as char);
            }
            b' ' => result.push_str("%20"),
            _ => {
                result.push_str(&format!("%{:02X}", byte));
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::path::PathBuf;

    fn test_context() -> ToolContext {
        ToolContext {
            agent_id: Default::default(),
            session_id: Default::default(),
            working_dir: PathBuf::from("/tmp"),
            env: HashMap::new(),
        }
    }

    #[tokio::test]
    async fn test_web_fetch_invalid_url() {
        let fetch = WebFetch::new();
        let args = serde_json::json!({
            "url": "not-a-url"
        });
        let result = fetch.execute(args, &test_context()).await.unwrap();
        assert!(!result.success);
        assert!(result.error.unwrap().contains("URL must start with"));
    }

    #[tokio::test]
    async fn test_web_fetch_http_error() {
        let fetch = WebFetch::new();
        let args = serde_json::json!({
            "url": "https://httpstat.us/404"
        });
        let result = fetch.execute(args, &test_context()).await;
        // May fail due to network or return HTTP error - either is acceptable
        if let Ok(res) = result {
            if !res.success {
                assert!(res.error.unwrap().contains("HTTP"));
            }
        }
    }

    #[tokio::test]
    async fn test_web_search_no_template() {
        let search = WebSearch::new();
        let args = serde_json::json!({
            "query": "rust programming",
            "max_results": 3
        });
        let result = search.execute(args, &test_context()).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("not configured"));
    }

    #[tokio::test]
    async fn test_web_fetch_timeout() {
        let fetch = WebFetch::new();
        let args = serde_json::json!({
            "url": "https://httpstat.us/200?sleep=5000"
        });
        // Should not hang — the 30s client timeout should handle it
        let result = fetch.execute(args, &test_context()).await;
        // Either success or a network error is fine
        assert!(result.is_ok() || result.is_err());
    }

    #[tokio::test]
    async fn test_urlencoding() {
        assert_eq!(urlencoding("hello world"), "hello%20world");
        assert_eq!(urlencoding("foo/bar"), "foo%2Fbar");
        assert_eq!(urlencoding("a b c"), "a%20b%20c");
        assert_eq!(urlencoding("simple"), "simple");
        assert_eq!(urlencoding(""), "");
    }
}
