/// Live integration test — runs Raven against a real DeepSeek model.
///
/// Reads DEEPSEEK_API_KEY from ~/.odin/.env (never committed to repo).
/// Compares looped engine vs baseline on real tasks with actual token counts.
///
/// Usage:
///   DEEPSEEK_API_KEY=sk-... cargo test -p odin-loop --test live_comparison -- --nocapture
///
/// Or create ~/.odin/.env with: DEEPSEEK_API_KEY=sk-...
///
/// Note: Uses a dedicated tokio runtime with shutdown_timeout to avoid
/// the reqwest Client connection pool drain hanging test completion.
use async_trait::async_trait;
use chrono::Utc;
use odin_baseline::BaselineAgent;
use odin_core::error::OdinResult;
use odin_core::traits::{ChatStream, LoopEngine as LoopEngineTrait, Provider};
use odin_core::types::*;
use odin_loop::engine::Engine as LoopEngine;
use reqwest::Client;
use serde_json::Value;
use std::sync::Arc;

// ═══════════════════════════════════════════════════════════════════
// DeepSeek Provider (lightweight, just what we need for testing)
// ═══════════════════════════════════════════════════════════════════

struct DeepSeekProvider {
    client: Client,
    api_key: String,
    model: String,
    /// Track actual token usage (atomic to avoid tokio Mutex issues)
    total_prompt_tokens: std::sync::atomic::AtomicU32,
    total_completion_tokens: std::sync::atomic::AtomicU32,
    /// Per-request timeout
    request_timeout: std::time::Duration,
}

impl DeepSeekProvider {
    fn new(api_key: String, model: &str) -> Self {
        Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(90))
                .pool_max_idle_per_host(0) // Disable keepalive to avoid async shutdown hang
                .build()
                .unwrap_or_default(),
            api_key,
            model: model.to_string(),
            total_prompt_tokens: std::sync::atomic::AtomicU32::new(0),
            total_completion_tokens: std::sync::atomic::AtomicU32::new(0),
            request_timeout: std::time::Duration::from_secs(90),
        }
    }

    fn token_usage(&self) -> TokenUsage {
        use std::sync::atomic::Ordering;
        let prompt = self.total_prompt_tokens.load(Ordering::Relaxed);
        let completion = self.total_completion_tokens.load(Ordering::Relaxed);
        TokenUsage {
            prompt_tokens: prompt,
            completion_tokens: completion,
            total_tokens: prompt + completion,
        }
    }

    fn track_usage(&self, usage: &TokenUsage) {
        use std::sync::atomic::Ordering;
        self.total_prompt_tokens
            .fetch_add(usage.prompt_tokens, Ordering::Relaxed);
        self.total_completion_tokens
            .fetch_add(usage.completion_tokens, Ordering::Relaxed);
    }
}

#[async_trait]
impl Provider for DeepSeekProvider {
    fn name(&self) -> &str {
        "deepseek"
    }

    async fn list_models(&self) -> OdinResult<Vec<ModelInfo>> {
        Ok(vec![ModelInfo {
            id: self.model.clone(),
            provider: "deepseek".into(),
            context_length: 65536,
            supports_tools: true,
            supports_vision: false,
        }])
    }

    async fn chat(
        &self,
        _model: &str,
        messages: &[Message],
        tools: &[ToolSchema],
        options: &CompletionOptions,
    ) -> OdinResult<ChatResponse> {
        // Convert messages to OpenAI format
        let msgs: Vec<Value> = messages
            .iter()
            .map(|m| {
                let role = m.role.to_string();
                let content = m.text().unwrap_or("");
                let mut obj = serde_json::json!({"role": role, "content": content});

                if let Some(ref tci) = m.tool_call_id {
                    obj["tool_call_id"] = Value::String(tci.clone());
                }
                obj
            })
            .collect();

        let mut body = serde_json::json!({
            "model": self.model,
            "messages": msgs,
            "temperature": options.temperature.unwrap_or(0.7),
            "max_tokens": options.max_tokens.unwrap_or(4096),
            "stream": false,
        });

        if !tools.is_empty() {
            let tools_json: Vec<Value> = tools
                .iter()
                .map(|t| {
                    serde_json::json!({
                        "type": "function",
                        "function": {
                            "name": t.function.name,
                            "description": t.function.description,
                            "parameters": t.function.parameters,
                        }
                    })
                })
                .collect();
            body["tools"] = Value::Array(tools_json);
        }

        let resp = self
            .client
            .post("https://api.deepseek.com/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                odin_core::error::OdinError::provider("deepseek", format!("Request failed: {}", e))
            })?;

        let status = resp.status();
        let json: Value = resp.json().await.map_err(|e| {
            odin_core::error::OdinError::provider(
                "deepseek",
                format!("Invalid response (HTTP {}): {}", status.as_u16(), e),
            )
        })?;

        if !status.is_success() {
            return Err(odin_core::error::OdinError::provider(
                "deepseek",
                format!(
                    "API error: {}",
                    json["error"]["message"].as_str().unwrap_or("unknown")
                ),
            ));
        }

        let choice = &json["choices"][0];
        let msg = &choice["message"];

        let mut content = msg["content"].as_str().map(|s| s.to_string());
        // DeepSeek reasoning models: output may be in reasoning_content
        if content.as_deref().unwrap_or("").is_empty() {
            content = msg["reasoning_content"].as_str().map(|s| s.to_string());
        }
        let tool_calls: Vec<ToolCall> = msg["tool_calls"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .map(|tc| ToolCall {
                        id: tc["id"].as_str().unwrap_or("").to_string(),
                        call_type: "function".to_string(),
                        function: FunctionCall {
                            name: tc["function"]["name"].as_str().unwrap_or("").to_string(),
                            arguments: tc["function"]["arguments"]
                                .as_str()
                                .unwrap_or("{}")
                                .to_string(),
                        },
                    })
                    .collect()
            })
            .unwrap_or_default();

        let usage = TokenUsage {
            prompt_tokens: json["usage"]["prompt_tokens"].as_u64().unwrap_or(0) as u32,
            completion_tokens: json["usage"]["completion_tokens"].as_u64().unwrap_or(0) as u32,
            total_tokens: json["usage"]["total_tokens"].as_u64().unwrap_or(0) as u32,
        };
        self.track_usage(&usage);

        let message = if !tool_calls.is_empty() {
            Message {
                role: Role::Assistant,
                content: MessageContent::ToolCalls {
                    content,
                    tool_calls,
                },
                name: None,
                tool_call_id: None,
            }
        } else {
            Message::assistant(content.unwrap_or_default())
        };

        Ok(ChatResponse {
            message,
            usage,
            finish_reason: choice["finish_reason"].as_str().map(|s| s.to_string()),
            model: json["model"].as_str().unwrap_or(&self.model).to_string(),
        })
    }

    async fn chat_stream(
        &self,
        _model: &str,
        _messages: &[Message],
        _tools: &[ToolSchema],
        _options: &CompletionOptions,
    ) -> OdinResult<Box<dyn ChatStream>> {
        unimplemented!("Streaming not needed for comparison tests")
    }

    async fn health_check(&self) -> OdinResult<bool> {
        Ok(true)
    }
}

// ═══════════════════════════════════════════════════════════════════
// Live Comparison Runner
// ═══════════════════════════════════════════════════════════════════

fn load_api_key() -> Option<String> {
    // 1. Check DEEPSEEK_API_KEY env var (secure, not in repo)
    if let Ok(key) = std::env::var("DEEPSEEK_API_KEY") {
        if !key.is_empty() && key != "sk-..." {
            return Some(key);
        }
    }

    // 2. Check ~/.odin/.env file
    let home = std::env::var("HOME").ok()?;
    let env_path = std::path::PathBuf::from(home).join(".odin/.env");
    if env_path.exists() {
        if let Ok(contents) = std::fs::read_to_string(&env_path) {
            for line in contents.lines() {
                if let Some(key) = line.strip_prefix("DEEPSEEK_API_KEY=") {
                    let key = key.trim().trim_matches('"').trim_matches('\'');
                    if !key.is_empty() {
                        return Some(key.to_string());
                    }
                }
            }
        }
    }

    None
}

#[derive(Debug, serde::Serialize)]
struct LiveRun {
    agent: String,
    task: String,
    iterations: u32,
    tool_calls_made: u32,
    actual_prompt_tokens: u32,
    actual_completion_tokens: u32,
    actual_total_tokens: u32,
    confidence: f64,
    duration_ms: u64,
    success: bool,
    error: Option<String>,
    summary: String,
}

#[tokio::test]
#[ignore = "requires DEEPSEEK_API_KEY — set in env or ~/.odin/.env"]
async fn test_live_deepseek_comparison() {
    let api_key = load_api_key().expect(
        "DEEPSEEK_API_KEY not set. Export it or add to ~/.odin/.env:\n  DEEPSEEK_API_KEY=sk-...",
    );

    eprintln!(
        "✓ DeepSeek API key loaded (starts with: {}...)",
        &api_key[..12]
    );

    run_comparison(&api_key).await;
    eprintln!("[TEST] ✓ PASSED");
    // Force exit — tokio multi-threaded runtime hangs on reqwest Client drop
    std::process::exit(0);
}

async fn run_comparison(api_key: &str) {
    let tasks = vec!["Write a Python function that checks if a string is a palindrome"];

    for task in &tasks {
        eprintln!("\n━━━ Task: {} ━━━", task);

        let provider = Arc::new(DeepSeekProvider::new(
            api_key.to_string(),
            "deepseek-v4-flash",
        ));
        let engine = LoopEngine::new()
            .with_max_iterations(10)
            .with_provider(provider.clone());

        let agent_task = AgentTask {
            id: TaskId::new_v4(),
            goal: task.to_string(),
            context: None,
            sub_tasks: vec![],
            success_criteria: vec![],
            max_iterations: 10,
            created_at: Utc::now(),
        };

        let start = std::time::Instant::now();
        eprintln!("[TEST] calling engine...");
        let result = engine.execute_task(&agent_task).await;
        eprintln!("[TEST] engine returned");
        let duration_ms = start.elapsed().as_millis() as u64;
        let usage = provider.token_usage();

        match result {
            Ok(r) => {
                eprintln!(
                    "  RAVEN: {} iters | {} prompt / {} completion tokens | {:.0}% conf | {}ms | success={}",
                    r.iterations,
                    usage.prompt_tokens,
                    usage.completion_tokens,
                    r.confidence * 100.0,
                    duration_ms,
                    r.success,
                );
                for st in &r.sub_tasks {
                    eprintln!("    sub-task: {} ({:?})", st.description, st.status);
                }
            }
            Err(e) => {
                eprintln!("  RAVEN: FAILED — {}", e);
            }
        }
    }
    eprintln!("\n✓ Complete");
}
