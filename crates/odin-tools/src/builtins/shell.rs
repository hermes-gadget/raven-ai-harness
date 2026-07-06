//! Shell command execution tool with dangerous-command detection.

use std::time::Instant;

use async_trait::async_trait;
use chrono::Utc;
use regex::Regex;
use serde::Deserialize;
use tokio::process::Command;
use tracing::instrument;

use odin_core::error::{OdinError, OdinResult};
use odin_core::traits::{Tool, ToolContext};
use odin_core::types::{FunctionSchema, ToolResult, ToolSchema};

/// Arguments for the `shell` tool.
#[derive(Debug, Deserialize)]
struct ShellArgs {
    command: String,
    #[serde(default)]
    workdir: Option<String>,
    #[serde(default = "default_timeout")]
    timeout_secs: u64,
}

fn default_timeout() -> u64 {
    60
}

/// Tool that executes shell commands.
///
/// The shell tool runs commands via `/bin/sh -c`. It checks the command
/// against a list of dangerous patterns and marks itself as requiring
/// approval if a match is found.
pub struct Shell {
    name: String,
    description: String,
    dangerous_patterns: Vec<Regex>,
}

impl Shell {
    /// Create a new `Shell` tool with default dangerous-command patterns.
    pub fn new() -> Self {
        let patterns = [
            r"rm\s+-rf",
            r"rm\s+-r\s+/",
            r"git\s+reset\s+--hard",
            r"git\s+push\s+--force",
            r"sudo\s+",
            r"chmod\s+777",
            r">\s*/dev/",
            r"mkfs\.",
            r"dd\s+if=",
            r":\(\)\s*\{",
            r">\s*/dev/sda",
            r"mv\s+/\s+",
            r"shutdown\s",
            r"reboot\s",
            r"init\s+0",
            r"init\s+6",
            r"poweroff",
        ];

        let re_patterns: Vec<Regex> = patterns
            .iter()
            .map(|p| Regex::new(p).expect("Invalid dangerous command regex"))
            .collect();

        Self {
            name: "shell".into(),
            description: "Execute a shell command and return its stdout and stderr. Use for running terminal commands, scripts, or any system interaction.".into(),
            dangerous_patterns: re_patterns,
        }
    }

    /// Create a `Shell` with custom dangerous-command patterns.
    pub fn with_patterns(patterns: Vec<Regex>) -> Self {
        Self {
            name: "shell".into(),
            description: "Execute a shell command and return its stdout and stderr.".into(),
            dangerous_patterns: patterns,
        }
    }

    /// Check whether a command matches any dangerous pattern.
    pub fn is_dangerous(&self, command: &str) -> bool {
        self.dangerous_patterns.iter().any(|re| re.is_match(command))
    }

    /// Construct the JSON schema for this tool.
    fn make_schema(name: &str) -> ToolSchema {
        ToolSchema {
            schema_type: "function".into(),
            function: FunctionSchema {
                name: name.into(),
                description: "Execute a shell command.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "command": {
                            "type": "string",
                            "description": "Shell command to execute (passed to /bin/sh -c)"
                        },
                        "workdir": {
                            "type": "string",
                            "description": "Working directory for the command (optional, defaults to agent working dir)"
                        },
                        "timeout_secs": {
                            "type": "integer",
                            "description": "Timeout in seconds (optional, defaults to 60)",
                            "default": 60
                        }
                    },
                    "required": ["command"]
                }),
            },
        }
    }
}

impl Default for Shell {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for Shell {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn schema(&self) -> ToolSchema {
        Self::make_schema(&self.name)
    }

    fn requires_approval(&self) -> bool {
        true  // shell commands always require approval for safety
    }

    fn is_safe(&self) -> bool {
        false  // shell is not inherently safe
    }

    #[instrument(skip(self, _context), fields(tool = self.name))]
    async fn execute(
        &self,
        args: serde_json::Value,
        _context: &ToolContext,
    ) -> OdinResult<ToolResult> {
        let start = Instant::now();

        let parsed: ShellArgs = serde_json::from_value(args).map_err(|e| {
            OdinError::Tool {
                tool: self.name.clone(),
                message: format!("Invalid arguments: {e}"),
                source: Some(Box::new(e)),
            }
        })?;

        let command_str = &parsed.command;

        // Check for dangerous commands
        if self.is_dangerous(command_str) {
            return Ok(ToolResult {
                call_id: String::new(),
                tool_name: self.name.clone(),
                success: false,
                output: String::new(),
                error: Some(format!(
                    "Command matches dangerous pattern and was blocked: {command_str}"
                )),
                duration_ms: 0,
                timestamp: Utc::now(),
            });
        }

        // Build the command
        let mut cmd = Command::new("/bin/sh");
        cmd.args(["-c", command_str]);

        // Set working directory if provided
        if let Some(workdir) = &parsed.workdir {
            cmd.current_dir(workdir);
        } else {
            cmd.current_dir(&_context.working_dir);
        }

        // Set timeout
        let timeout = std::time::Duration::from_secs(parsed.timeout_secs.max(1));

        // Spawn and wait with timeout
        let output = tokio::time::timeout(timeout, cmd.output()).await.map_err(|_| {
            OdinError::Timeout(format!(
                "Shell command timed out after {}s: {command_str}",
                parsed.timeout_secs
            ))
        })?;

        let output = output.map_err(|e| {
            OdinError::Tool {
                tool: self.name.clone(),
                message: format!("Failed to execute command: {e}"),
                source: Some(Box::new(e)),
            }
        })?;

        let duration_ms = start.elapsed().as_millis() as u64;

        // Combine stdout and stderr
        let mut result_output = String::new();
        if !output.stdout.is_empty() {
            result_output.push_str(&String::from_utf8_lossy(&output.stdout));
        }
        if !output.stderr.is_empty() {
            if !result_output.is_empty() {
                result_output.push('\n');
            }
            result_output.push_str("STDERR:\n");
            result_output.push_str(&String::from_utf8_lossy(&output.stderr));
        }

        let success = output.status.success();
        let error = if success {
            None
        } else {
            Some(format!("Exit code: {}", output.status.code().unwrap_or(-1)))
        };

        Ok(ToolResult {
            call_id: String::new(),
            tool_name: self.name.clone(),
            success,
            output: result_output,
            error,
            duration_ms,
            timestamp: Utc::now(),
        })
    }
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
    async fn test_shell_echo() {
        let shell = Shell::new();
        let args = serde_json::json!({
            "command": "echo 'hello odin'",
            "timeout_secs": 10
        });
        let result = shell.execute(args, &test_context()).await.unwrap();
        assert!(result.success, "STDERR: {:?}", result.error);
        assert!(result.output.contains("hello odin"), "output: {}", result.output);
    }

    #[tokio::test]
    async fn test_shell_pwd() {
        let shell = Shell::new();
        let args = serde_json::json!({
            "command": "pwd",
            "workdir": "/tmp",
            "timeout_secs": 10
        });
        let result = shell.execute(args, &test_context()).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("/tmp"), "output: {}", result.output);
    }

    #[tokio::test]
    async fn test_shell_dangerous_blocked() {
        let shell = Shell::new();
        let args = serde_json::json!({
            "command": "rm -rf /important",
            "timeout_secs": 5
        });
        let result = shell.execute(args, &test_context()).await.unwrap();
        assert!(!result.success);
        assert!(result.error.as_deref().unwrap_or("").contains("dangerous"));
    }

    #[tokio::test]
    async fn test_is_dangerous() {
        let shell = Shell::new();
        assert!(shell.is_dangerous("rm -rf /"));
        assert!(shell.is_dangerous("sudo rm -rf /"));
        assert!(shell.is_dangerous("git push --force origin main"));
        assert!(!shell.is_dangerous("ls -la"));
        assert!(!shell.is_dangerous("echo hello"));
        assert!(!shell.is_dangerous("cat /etc/hostname"));
    }

    #[tokio::test]
    async fn test_shell_timeout() {
        let shell = Shell::new();
        let args = serde_json::json!({
            "command": "sleep 30",
            "timeout_secs": 1
        });
        let result = shell.execute(args, &test_context()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_shell_invalid_command() {
        let shell = Shell::new();
        let args = serde_json::json!({
            "command": "nonexistent_command_xyz123",
            "timeout_secs": 5
        });
        let result = shell.execute(args, &test_context()).await.unwrap();
        assert!(!result.success);
    }
}
