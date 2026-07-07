//! Anthropic provider (Claude models).

use async_trait::async_trait;
use odin_core::error::{OdinError, OdinResult};
use odin_core::traits::{ChatStream, Provider};
use odin_core::types::*;
use reqwest::Client;
use serde_json::Value;

pub struct AnthropicProvider {
    client: Client,
    api_key: String,
    base_url: String,
}

impl AnthropicProvider {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            client: Client::new(),
            api_key: api_key.into(),
            base_url: "https://api.anthropic.com/v1".into(),
        }
    }

    fn convert_messages(messages: &[Message]) -> (Option<String>, Vec<Value>) {
        let system = messages
            .iter()
            .filter(|m| m.role == Role::System)
            .map(|m| m.text().unwrap_or("").to_string())
            .collect::<Vec<_>>()
            .join("\n");

        let system_msg = if system.is_empty() {
            None
        } else {
            Some(system)
        };

        let anthropic_msgs: Vec<Value> = messages
            .iter()
            .filter(|m| m.role != Role::System)
            .map(|m| {
                let role = match m.role {
                    Role::User => "user",
                    Role::Assistant => "assistant",
                    Role::Tool => "user", // Anthropic uses user role for tool results
                    Role::System => "user",
                };
                let content = m.text().unwrap_or("");
                serde_json::json!({"role": role, "content": content})
            })
            .collect();

        (system_msg, anthropic_msgs)
    }
}

#[async_trait]
impl Provider for AnthropicProvider {
    fn name(&self) -> &str {
        "anthropic"
    }

    async fn list_models(&self) -> OdinResult<Vec<ModelInfo>> {
        Ok(vec![
            ModelInfo {
                id: "claude-sonnet-4-20250514".into(),
                provider: "anthropic".into(),
                context_length: 200000,
                supports_tools: true,
                supports_vision: true,
            },
            ModelInfo {
                id: "claude-haiku-3-5-20241022".into(),
                provider: "anthropic".into(),
                context_length: 200000,
                supports_tools: true,
                supports_vision: false,
            },
        ])
    }

    async fn chat(
        &self,
        model: &str,
        messages: &[Message],
        _tools: &[ToolSchema],
        _options: &CompletionOptions,
    ) -> OdinResult<ChatResponse> {
        let (_system, anthropic_msgs) = Self::convert_messages(messages);

        let body = serde_json::json!({
            "model": model,
            "max_tokens": 4096,
            "messages": anthropic_msgs,
        });

        let resp = self
            .client
            .post(&format!("{}/messages", self.base_url))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .await
            .map_err(|e| OdinError::provider("anthropic", format!("Request failed: {}", e)))?;

        let json: Value = resp
            .json()
            .await
            .map_err(|e| OdinError::provider("anthropic", format!("Invalid response: {}", e)))?;

        let content = json["content"][0]["text"]
            .as_str()
            .unwrap_or("")
            .to_string();

        Ok(ChatResponse {
            message: Message::assistant(content),
            usage: TokenUsage::default(),
            finish_reason: json["stop_reason"].as_str().map(|s| s.to_string()),
            model: model.to_string(),
        })
    }

    async fn chat_stream(
        &self,
        _model: &str,
        _messages: &[Message],
        _tools: &[ToolSchema],
        _options: &CompletionOptions,
    ) -> OdinResult<Box<dyn ChatStream>> {
        Err(OdinError::provider(
            "anthropic",
            "Streaming not yet implemented",
        ))
    }

    async fn health_check(&self) -> OdinResult<bool> {
        Ok(true)
    }
}
