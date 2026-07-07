//! Core types used across all Odin crates.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Unique identifier for an agent instance.
pub type AgentId = Uuid;

/// Unique identifier for a session.
pub type SessionId = Uuid;

/// Unique identifier for a task.
pub type TaskId = Uuid;

/// Unique identifier for a tool call.
pub type CallId = Uuid;

// ── Message Types ──────────────────────────────────────────────────

/// Role in a conversation.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, strum_macros::Display,
)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

/// A single message in a conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: MessageContent,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

/// Content of a message — text, tool calls, or tool results.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    Text {
        content: String,
    },
    ToolCalls {
        content: Option<String>,
        tool_calls: Vec<ToolCall>,
    },
    /// OpenAI-style assistant message with text + tool_calls
    AssistantWithTools {
        content: Option<String>,
        tool_calls: Vec<ToolCall>,
    },
}

impl Message {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: Role::System,
            content: MessageContent::Text {
                content: content.into(),
            },
            name: None,
            tool_call_id: None,
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: MessageContent::Text {
                content: content.into(),
            },
            name: None,
            tool_call_id: None,
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: MessageContent::Text {
                content: content.into(),
            },
            name: None,
            tool_call_id: None,
        }
    }

    pub fn tool_result(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: Role::Tool,
            content: MessageContent::Text {
                content: content.into(),
            },
            name: None,
            tool_call_id: Some(tool_call_id.into()),
        }
    }

    pub fn text(&self) -> Option<&str> {
        match &self.content {
            MessageContent::Text { content } => Some(content.as_str()),
            MessageContent::ToolCalls { content, .. }
            | MessageContent::AssistantWithTools { content, .. } => content.as_deref(),
        }
    }

    pub fn tool_calls(&self) -> &[ToolCall] {
        match &self.content {
            MessageContent::ToolCalls { tool_calls, .. }
            | MessageContent::AssistantWithTools { tool_calls, .. } => tool_calls.as_slice(),
            MessageContent::Text { .. } => &[],
        }
    }
}

// ── Tool Types ─────────────────────────────────────────────────────

/// A tool call requested by the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: FunctionCall,
}

/// The function name and arguments within a tool call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    /// JSON-encoded arguments string
    pub arguments: String,
}

/// Schema definition for a tool the model can use.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSchema {
    #[serde(rename = "type")]
    pub schema_type: String,
    pub function: FunctionSchema,
}

/// Function schema within a tool definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionSchema {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// Result of executing a tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub call_id: String,
    pub tool_name: String,
    pub success: bool,
    pub output: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub duration_ms: u64,
    pub timestamp: DateTime<Utc>,
}

// ── Agent / Task Types ─────────────────────────────────────────────

/// A task for an agent to execute.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTask {
    pub id: TaskId,
    pub goal: String,
    pub context: Option<String>,
    #[serde(default)]
    pub sub_tasks: Vec<SubTask>,
    #[serde(default)]
    pub success_criteria: Vec<String>,
    pub max_iterations: u32,
    pub created_at: DateTime<Utc>,
}

/// A decomposed sub-task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubTask {
    pub id: String,
    pub description: String,
    pub status: SubTaskStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum_macros::Display)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum SubTaskStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
    Skipped,
}

/// Result of an agent task execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    pub task_id: TaskId,
    pub success: bool,
    pub summary: String,
    pub iterations: u32,
    pub tool_calls: u32,
    pub duration_ms: u64,
    pub sub_tasks: Vec<SubTask>,
    pub confidence: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

// ── Loop Engine Types ──────────────────────────────────────────────

/// The phases of the agent loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum_macros::Display)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum LoopPhase {
    Plan,
    Act,
    Inspect,
    Critique,
    Revise,
    Verify,
    Decide,
}

/// The decision at the end of a loop iteration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoopDecision {
    Continue,
    Stop,
    Escalate,
    Retry,
}

/// Confidence score for a model's output (0.0–1.0).
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct ConfidenceScore(pub f64);

impl ConfidenceScore {
    pub fn new(score: f64) -> Self {
        Self(score.clamp(0.0, 1.0))
    }

    pub fn is_high(&self) -> bool {
        self.0 >= 0.8
    }

    pub fn is_low(&self) -> bool {
        self.0 < 0.5
    }

    pub fn value(&self) -> f64 {
        self.0
    }
}

/// A compact summary of the agent's state for small context windows.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateSummary {
    pub goal: String,
    pub current_phase: LoopPhase,
    pub completed_steps: Vec<String>,
    pub pending_steps: Vec<String>,
    pub last_action: Option<String>,
    pub last_result: Option<String>,
    pub errors: Vec<String>,
    pub confidence: f64,
    pub token_usage: TokenUsage,
}

// ── Model / Provider Types ─────────────────────────────────────────

/// Information about an available model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub provider: String,
    pub context_length: u32,
    pub supports_tools: bool,
    pub supports_vision: bool,
}

/// Options for a completion request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
}

impl Default for CompletionOptions {
    fn default() -> Self {
        Self {
            temperature: Some(0.7),
            max_tokens: Some(4096),
            top_p: None,
            stop: None,
            stream: None,
        }
    }
}

/// Response from a chat completion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub message: Message,
    pub usage: TokenUsage,
    pub finish_reason: Option<String>,
    pub model: String,
}

/// Token usage statistics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

impl TokenUsage {
    pub fn add(&mut self, other: &TokenUsage) {
        self.prompt_tokens += other.prompt_tokens;
        self.completion_tokens += other.completion_tokens;
        self.total_tokens += other.total_tokens;
    }
}

// ── Memory Types ────────────────────────────────────────────────────

/// A memory entry in persistent storage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: String,
    pub content: String,
    pub category: MemoryCategory,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub importance: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum_macros::Display)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum MemoryCategory {
    Preference,
    Entity,
    Event,
    Fact,
    Pattern,
}

// ── Audit Types ─────────────────────────────────────────────────────

/// An audit log entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub agent_id: AgentId,
    pub session_id: SessionId,
    pub event_type: AuditEventType,
    pub action: String,
    pub details: serde_json::Value,
    pub result: AuditResult,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum_macros::Display)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum AuditEventType {
    ToolCall,
    ModelCall,
    Decision,
    PermissionCheck,
    ConfigChange,
    Error,
    SessionStart,
    SessionEnd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum_macros::Display)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum AuditResult {
    Success,
    Failure,
    Denied,
    Pending,
}

// ── Permission Types ────────────────────────────────────────────────

/// A permission rule for a tool or action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRule {
    pub tool_name: String,
    pub action: PermissionAction,
    pub require_approval: bool,
    pub max_rate_per_minute: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum_macros::Display)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum PermissionAction {
    Allow,
    Deny,
    AskUser,
}

// ── Utility Types ───────────────────────────────────────────────────

/// The severity level of an event.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    strum_macros::Display,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum Severity {
    Debug,
    Info,
    Warning,
    Error,
    Critical,
}

/// A file path boundary for sandboxing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathBoundary {
    pub allowed_read: Vec<String>,
    pub allowed_write: Vec<String>,
    pub denied: Vec<String>,
}

impl Default for PathBoundary {
    fn default() -> Self {
        Self {
            allowed_read: vec![".".to_string()],
            allowed_write: vec![".".to_string()],
            denied: vec![
                "/etc/passwd".to_string(),
                "/etc/shadow".to_string(),
                "~/.ssh".to_string(),
            ],
        }
    }
}
