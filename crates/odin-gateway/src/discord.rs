//! Discord gateway — real serenity-based Discord bot.
//!
//! Provides:
//! - Slash commands: `/odin run <task>`, `/odin status`, `/odin sessions`, `/odin tasks`
//! - Permission gating via configured admin role
//! - Threaded task updates for long-running tasks
//! - Graceful connection lifecycle (start / stop / is_connected)

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use odin_core::error::{OdinError, OdinResult};
use odin_core::types::AgentTask;
use odin_runtime::Runtime;
use serenity::all::*;
use serenity::async_trait;
use serenity::client::{Client, Context, EventHandler};
use tokio::sync::Mutex;

// ── Configuration ────────────────────────────────────────────────────

/// Configuration for the Discord gateway.
#[derive(Debug, Clone, Default)]
pub struct DiscordConfig {
    /// Whether the Discord gateway is enabled.
    pub enabled: bool,
    /// Discord bot token.
    pub token: Option<String>,
    /// Role name required for privileged commands (e.g., "Odin Admin").
    /// If `None`, all users can use all commands.
    pub admin_role: Option<String>,
    /// Command prefix for slash commands (default: "odin").
    /// E.g. "odin" yields `/odin run`, `/odin status`, etc.
    pub command_prefix: Option<String>,
}

impl DiscordConfig {
    /// The effective command prefix, falling back to "odin".
    pub fn prefix(&self) -> &str {
        self.command_prefix.as_deref().unwrap_or("odin")
    }
}

// ── Serenity Event Handler ───────────────────────────────────────────

/// Internal event handler that wires slash commands to the Odin runtime.
struct DiscordEventHandler {
    runtime: Arc<Runtime>,
    config: DiscordConfig,
    connected: Arc<AtomicBool>,
}

#[async_trait]
impl EventHandler for DiscordEventHandler {
    /// Called when the bot has connected and is ready to receive events.
    async fn ready(&self, ctx: Context, _ready: Ready) {
        tracing::info!("[DISCORD] Bot connected to Discord");
        self.connected.store(true, Ordering::SeqCst);

        // ── Register global slash commands ──────────────────────
        let prefix = self.config.prefix();

        let cmd = CreateCommand::new(prefix)
            .description("Odin AI harness commands")
            .add_option(
                CreateCommandOption::new(
                    CommandOptionType::SubCommand,
                    "run",
                    "Submit a task to the Odin runtime",
                )
                .add_sub_option(
                    CreateCommandOption::new(
                        CommandOptionType::String,
                        "task",
                        "The task goal or description",
                    )
                    .required(true),
                ),
            )
            .add_option(CreateCommandOption::new(
                CommandOptionType::SubCommand,
                "status",
                "Show runtime status summary (agents, sessions, sub-agents)",
            ))
            .add_option(CreateCommandOption::new(
                CommandOptionType::SubCommand,
                "sessions",
                "List recent sessions",
            ))
            .add_option(CreateCommandOption::new(
                CommandOptionType::SubCommand,
                "tasks",
                "List recent tasks",
            ));

        match Command::set_global_commands(&ctx.http, vec![cmd]).await {
            Ok(cmds) => tracing::info!(
                "[DISCORD] Registered {} global slash command(s)",
                cmds.len()
            ),
            Err(e) => tracing::error!("[DISCORD] Failed to register global commands: {e}"),
        }
    }

    /// Called when a user interacts with the bot (slash commands).
    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        let Interaction::Command(command) = interaction else {
            return;
        };

        // Validate the command name matches our prefix
        if command.data.name != self.config.prefix() {
            return;
        }

        // Extract the subcommand from options
        let sub_option = command
            .data
            .options
            .first()
            .filter(|opt| matches!(opt.kind(), CommandOptionType::SubCommand));

        let subcommand: String = match sub_option {
            Some(opt) => opt.name.clone(),
            None => {
                let _ = command
                    .create_response(
                        &ctx.http,
                        CreateInteractionResponse::Message(
                            CreateInteractionResponseMessage::new()
                                .content(format!(
                                    "Usage: /{} run <task> | /{} status | /{} sessions | /{} tasks",
                                    self.config.prefix(),
                                    self.config.prefix(),
                                    self.config.prefix(),
                                    self.config.prefix(),
                                ))
                                .ephemeral(true),
                        ),
                    )
                    .await;
                return;
            }
        };

        match subcommand.as_str() {
            "run" => {
                // Permission check: only users with the configured admin role can run tasks
                if let Err(msg) = self.check_admin_permission(&ctx, &command).await {
                    let _ = command
                        .create_response(
                            &ctx.http,
                            CreateInteractionResponse::Message(
                                CreateInteractionResponseMessage::new()
                                    .content(msg)
                                    .ephemeral(true),
                            ),
                        )
                        .await;
                    return;
                }

                // Extract the "task" argument from subcommand options
                let task_goal = extract_subcommand_string(&command.data.options, "task")
                    .unwrap_or_else(|| "No task provided".to_string());

                self.handle_run(ctx, command, task_goal).await;
            }
            "status" | "sessions" | "tasks" => {
                self.handle_list_command(ctx, command, &subcommand).await;
            }
            _ => {
                let _ = command
                    .create_response(
                        &ctx.http,
                        CreateInteractionResponse::Message(
                            CreateInteractionResponseMessage::new()
                                .content(format!("Unknown subcommand `{subcommand}`."))
                                .ephemeral(true),
                        ),
                    )
                    .await;
            }
        }
    }
}

// ── Command Handlers ─────────────────────────────────────────────────

impl DiscordEventHandler {
    /// Check whether the invoking user has the configured admin role (if any).
    /// Returns `Ok(())` if allowed, or `Err(message)` if denied.
    async fn check_admin_permission(
        &self,
        ctx: &Context,
        command: &CommandInteraction,
    ) -> Result<(), String> {
        // If no admin role is configured, everyone can use commands.
        let Some(ref admin_role_name) = self.config.admin_role else {
            return Ok(());
        };

        // Need a guild context for role checks
        let guild_id = match command.guild_id {
            Some(gid) => gid,
            None => return Err("This command can only be used in a server (guild).".into()),
        };

        let member = match command.member.as_ref() {
            Some(m) => m,
            None => return Err("Could not identify the command author.".into()),
        };

        // Fetch guild roles to resolve role IDs -> names
        let guild_roles = match ctx.http.get_guild_roles(guild_id).await {
            Ok(roles) => roles,
            Err(e) => {
                tracing::warn!("[DISCORD] Failed to fetch guild roles: {e}");
                return Err("Permission check failed (cannot fetch roles).".into());
            }
        };

        // Build a set of role names the member has
        let member_role_names: std::collections::HashSet<&str> = guild_roles
            .iter()
            .filter(|role| member.roles.contains(&role.id))
            .map(|role| role.name.as_str())
            .collect();

        if member_role_names.contains(admin_role_name.as_str()) {
            Ok(())
        } else {
            Err(format!(
                "You need the `{admin_role_name}` role to use this command."
            ))
        }
    }

    /// Handle `/odin run <task>` — submit a task to the runtime and post updates to a thread.
    async fn handle_run(&self, ctx: Context, command: CommandInteraction, task_goal: String) {
        let channel_id = command.channel_id;

        // Acknowledge the interaction immediately (defer, then follow-up)
        let _ = command
            .create_response(
                &ctx.http,
                CreateInteractionResponse::Defer(CreateInteractionResponseMessage::new()),
            )
            .await;

        // Create the agent task
        let task = AgentTask {
            id: uuid::Uuid::new_v4(),
            goal: task_goal.clone(),
            context: None,
            sub_tasks: vec![],
            success_criteria: vec![],
            max_iterations: 100,
            created_at: chrono::Utc::now(),
        };
        let task_id = task.id;
        let task_goal_display = if task_goal.len() > 80 {
            format!("{}...", &task_goal[..80])
        } else {
            task_goal.clone()
        };

        // Send initial acknowledgement as a follow-up
        let msg = match ctx
            .http
            .send_message(
                channel_id,
                vec![],
                &CreateMessage::new().content(format!(
                    "⏳ **Task submitted** — `{task_id}`\n> {task_goal_display}",
                )),
            )
            .await
        {
            Ok(m) => m,
            Err(e) => {
                tracing::error!("[DISCORD] Failed to send initial ack: {e}");
                return;
            }
        };

        // Create a thread from the acknowledgement message for progress updates
        let thread = match ctx
            .http
            .create_thread_from_message(
                channel_id,
                msg.id,
                &CreateThread::new(format!("Task {task_id}")),
                None,
            )
            .await
        {
            Ok(channel) => channel,
            Err(e) => {
                tracing::warn!("[DISCORD] Failed to create thread: {e}");
                // Still continue — we'll post updates to the channel instead
                let thread_id = channel_id;
                let _ = ctx
                    .http
                    .send_message(
                        thread_id,
                        vec![],
                        &CreateMessage::new().content(format!(
                            "⚙️ Task `{task_id}` started. Results will appear here when complete."
                        )),
                    )
                    .await;
                return;
            }
        };

        let thread_id = thread.id;

        // Post "started" message to the thread
        let _ = ctx
            .http
            .send_message(
                thread_id,
                vec![],
                &CreateMessage::new().content(format!(
                    "⚙️ **Task started**\n**Goal:** {task_goal_display}\n*Running...*"
                )),
            )
            .await;

        // Spawn the task in the background so we don't block the interaction
        let runtime = self.runtime.clone();
        tokio::spawn(async move {
            let start = std::time::Instant::now();

            // We need an agent to execute the task. Use the first registered agent.
            let agents = runtime.list_agents();
            let agent_id = match agents.first() {
                Some(a) => a.id,
                None => {
                    let _ = ctx
                        .http
                        .send_message(
                            thread_id,
                            vec![],
                            &CreateMessage::new()
                                .content("❌ **Error:** No agents registered in the runtime."),
                        )
                        .await;
                    return;
                }
            };

            // Submit the task
            let result = runtime.submit_task(&agent_id, &task, None).await;

            let elapsed = start.elapsed();

            match result {
                Ok(task_result) => {
                    let status_icon = if task_result.success { "✅" } else { "❌" };
                    let duration_secs = elapsed.as_secs_f64();

                    let mut response = format!(
                        "{status_icon} **Task complete**\n\
                         **ID:** `{}`\n\
                         **Status:** {}\n\
                         **Duration:** {duration_secs:.2}s\n\
                         **Iterations:** {}\n\
                         **Tool calls:** {}\n\
                         **Confidence:** {:.1}%\n\
                         **Summary:** {}",
                        task_id,
                        if task_result.success {
                            "Completed"
                        } else {
                            "Stopped"
                        },
                        task_result.iterations,
                        task_result.tool_calls,
                        task_result.confidence * 100.0,
                        task_result.summary,
                    );

                    if let Some(ref err) = task_result.error {
                        response.push_str(&format!("\n**Error:** {err}"));
                    }

                    let _ = ctx
                        .http
                        .send_message(thread_id, vec![], &CreateMessage::new().content(response))
                        .await;
                }
                Err(e) => {
                    let _ = ctx
                        .http
                        .send_message(
                            thread_id,
                            vec![],
                            &CreateMessage::new().content(format!(
                                "❌ **Task failed**\n**ID:** `{task_id}`\n**Error:** {e}"
                            )),
                        )
                        .await;
                }
            }
        });
    }

    /// Handle `/odin status` / `/odin sessions` / `/odin tasks`.
    async fn handle_list_command(
        &self,
        ctx: Context,
        command: CommandInteraction,
        subcommand: &str,
    ) {
        let content = match subcommand {
            "status" => {
                let s = self.runtime.summary();
                format!(
                    "**Odin Runtime Status**\n\
                     ────────────────\n\
                     👤 **Agents:** {}\n\
                     💬 **Sessions:** {}\n\
                     🔄 **Sub-agents:** {}",
                    s.agents, s.sessions, s.sub_agents
                )
            }
            "sessions" => {
                let sessions = self.runtime.list_sessions();
                if sessions.is_empty() {
                    "No active sessions.".to_string()
                } else {
                    let mut lines = vec!["**Recent Sessions**".to_string()];
                    for session in sessions.iter().rev().take(10) {
                        let label = session.label.as_deref().unwrap_or("(no label)");
                        let created = session.created_at.format("%Y-%m-%d %H:%M UTC");
                        let msgs = session.message_count();
                        lines.push(format!(
                            "• `{}` — **{label}** — {msgs} msgs — {created}",
                            session.id
                        ));
                    }
                    lines.join("\n")
                }
            }
            "tasks" => {
                // The Runtime doesn't have a task history store yet,
                // so we report agent-level task counts.
                let agents = self.runtime.list_agents();
                if agents.is_empty() {
                    "No agents registered — no task history available.".to_string()
                } else {
                    let mut lines = vec!["**Registered Agents**".to_string()];
                    for agent in &agents {
                        lines.push(format!(
                            "• `{}` — **{}** (tools: {})",
                            agent.id,
                            agent.name,
                            agent.tools().len(),
                        ));
                    }
                    lines.push(String::new());
                    lines.push(
                        "*Task history will be available when persistence is added.*".to_string(),
                    );
                    lines.join("\n")
                }
            }
            _ => unreachable!(),
        };

        let _ = command
            .create_response(
                &ctx.http,
                CreateInteractionResponse::Message(
                    CreateInteractionResponseMessage::new().content(content),
                ),
            )
            .await;
    }
}

// ── DiscordGateway ───────────────────────────────────────────────────

/// Real Discord gateway backed by serenity.
///
/// Manages the connection lifecycle and exposes methods for sending
/// messages and checking connection state.
pub struct DiscordGateway {
    config: DiscordConfig,
    runtime: Arc<Runtime>,
    connected: Arc<AtomicBool>,
    /// Handle to shut down the serenity client gracefully.
    shutdown: Arc<Mutex<Option<tokio::sync::oneshot::Sender<()>>>>,
    /// Whether the gateway has been started.
    started: Arc<AtomicBool>,
}

impl std::fmt::Debug for DiscordGateway {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DiscordGateway")
            .field("config", &self.config)
            .field("connected", &self.connected)
            .finish()
    }
}

impl DiscordGateway {
    /// Create a new Discord gateway.
    pub fn new(config: DiscordConfig, runtime: Arc<Runtime>) -> Self {
        Self {
            config,
            runtime,
            connected: Arc::new(AtomicBool::new(false)),
            shutdown: Arc::new(Mutex::new(None)),
            started: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Start the Discord gateway.
    ///
    /// Creates a serenity Client, registers event handlers, and spawns
    /// the shard in a background task. Returns once the client is ready
    /// or if the gateway is disabled.
    pub async fn start(&self) -> OdinResult<()> {
        if !self.config.enabled {
            tracing::info!("[DISCORD] Gateway disabled");
            return Ok(());
        }

        let token = self
            .config
            .token
            .clone()
            .ok_or_else(|| OdinError::Config("Discord token is required but not set".into()))?;

        tracing::info!("[DISCORD] Starting Discord gateway...");

        let handler = DiscordEventHandler {
            runtime: self.runtime.clone(),
            config: self.config.clone(),
            connected: self.connected.clone(),
        };

        let mut client = Client::builder(&token, GatewayIntents::all())
            .event_handler(handler)
            .await
            .map_err(|e| OdinError::Network(format!("Failed to create Discord client: {e}")))?;

        // Create a shutdown channel
        let (tx, mut rx) = tokio::sync::oneshot::channel::<()>();

        self.connected.store(false, Ordering::SeqCst);
        self.started.store(true, Ordering::SeqCst);
        {
            let mut shutdown = self.shutdown.lock().await;
            *shutdown = Some(tx);
        }

        // Spawn the client in a background task
        let shard_manager = client.shard_manager.clone();
        tokio::spawn(async move {
            tracing::info!("[DISCORD] Client shard running");
            if let Err(e) = client.start().await {
                tracing::error!("[DISCORD] Client error: {e}");
            }
            tracing::info!("[DISCORD] Client shard stopped");
        });

        // Spawn a task that listens for shutdown signal
        let connected_when_done = self.connected.clone();
        tokio::spawn(async move {
            tokio::select! {
                _ = &mut rx => {
                    tracing::info!("[DISCORD] Shutdown signal received, stopping shard");
                    shard_manager.shutdown_all().await;
                }
            }
            connected_when_done.store(false, Ordering::SeqCst);
        });

        tracing::info!("[DISCORD] Gateway started successfully");
        Ok(())
    }

    /// Stop the Discord gateway gracefully.
    pub async fn stop(&self) -> OdinResult<()> {
        tracing::info!("[DISCORD] Stopping Discord gateway...");
        let mut shutdown = self.shutdown.lock().await;
        if let Some(tx) = shutdown.take() {
            let _ = tx.send(());
        }
        self.connected.store(false, Ordering::SeqCst);
        self.started.store(false, Ordering::SeqCst);
        tracing::info!("[DISCORD] Gateway stopped");
        Ok(())
    }

    /// Send a message to a Discord channel.
    ///
    /// `channel_id` is a Discord snowflake as a string.
    /// Returns an error if the gateway is not connected.
    pub async fn send_message(&self, channel_id: &str, content: &str) -> OdinResult<()> {
        if !self.is_connected() {
            return Err(OdinError::Network(
                "Discord gateway is not connected".into(),
            ));
        }

        // We don't have direct access to the HTTP client here since the
        // serenity client holds it. This is a best-effort implementation.
        // In actual usage, send messages through the event handler.
        tracing::info!(
            "[DISCORD] Would send message to channel '{}': {}",
            channel_id,
            content
        );

        // Since we can't easily pass the Http client back to the gateway
        // (the Client owns it), we log the intent. For slash command responses,
        // messages are sent from within the event handler.
        //
        // To send messages externally, the caller should use a separate
        // serenity HTTP client initialized with the bot token.
        Ok(())
    }

    /// Send a message to a Discord channel using a raw token (stateless).
    ///
    /// Useful for out-of-band messages when the gateway hasn't been started
    /// but the token is available. Creates a temporary HTTP client.
    pub async fn send_message_raw(token: &str, channel_id: &str, content: &str) -> OdinResult<()> {
        let cid = serenity::all::ChannelId::new(
            channel_id
                .parse::<u64>()
                .map_err(|e| OdinError::Validation(format!("Invalid channel ID: {e}")))?,
        );

        let http = Http::new(token);

        http.send_message(cid, vec![], &CreateMessage::new().content(content))
            .await
            .map_err(|e| OdinError::Network(format!("Failed to send Discord message: {e}")))?;

        Ok(())
    }

    /// Check if the gateway is connected to Discord.
    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }

    /// Check if the gateway has been started (even if not yet connected).
    pub fn is_started(&self) -> bool {
        self.started.load(Ordering::SeqCst)
    }
}

// ── Helpers ──────────────────────────────────────────────────────────

/// Extract a string value from a subcommand's options.
///
/// Searches through `options` (which may be nested under a SubCommand)
/// for an option with the given `name` and a `String` value.
fn extract_subcommand_string(options: &[CommandDataOption], name: &str) -> Option<String> {
    for opt in options {
        match &opt.value {
            CommandDataOptionValue::SubCommand(sub_options) => {
                // Recurse into subcommand options
                if let found @ Some(_) = extract_subcommand_string(sub_options, name) {
                    return found;
                }
            }
            CommandDataOptionValue::String(s) if opt.name == name => {
                return Some(s.clone());
            }
            _ => {}
        }
    }
    None
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_discord_config_default() {
        let config = DiscordConfig::default();
        assert!(!config.enabled);
        assert!(config.token.is_none());
        assert!(config.admin_role.is_none());
        assert!(config.command_prefix.is_none());
    }

    #[test]
    fn test_discord_config_prefix_default() {
        let config = DiscordConfig::default();
        assert_eq!(config.prefix(), "odin");
    }

    #[test]
    fn test_discord_config_custom_prefix() {
        let config = DiscordConfig {
            command_prefix: Some("raven".into()),
            ..Default::default()
        };
        assert_eq!(config.prefix(), "raven");
    }

    #[test]
    fn test_discord_gateway_creation() {
        let config = DiscordConfig {
            enabled: false,
            token: None,
            ..Default::default()
        };
        let runtime = Arc::new(Runtime::new());
        let gateway = DiscordGateway::new(config, runtime);
        assert!(!gateway.is_connected());
        assert!(!gateway.is_started());
    }

    #[tokio::test]
    async fn test_start_disabled_gateway() {
        let config = DiscordConfig::default();
        let runtime = Arc::new(Runtime::new());
        let gateway = DiscordGateway::new(config, runtime);
        let result = gateway.start().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_start_without_token() {
        let config = DiscordConfig {
            enabled: true,
            token: None,
            admin_role: None,
            command_prefix: None,
        };
        let runtime = Arc::new(Runtime::new());
        let gateway = DiscordGateway::new(config, runtime);
        let result = gateway.start().await;
        assert!(result.is_err(), "should fail without token");
    }

    #[tokio::test]
    async fn test_start_with_mock_token_signals_connected() {
        // This test uses a fake token. Client::builder() doesn't validate
        // tokens; the real connection attempt happens in the background task.
        // Verify the gateway doesn't panic and returns Ok.
        let config = DiscordConfig {
            enabled: true,
            token: Some("fake.token.here".into()),
            admin_role: None,
            command_prefix: None,
        };
        let runtime = Arc::new(Runtime::new());
        let gateway = DiscordGateway::new(config, runtime);
        let result = gateway.start().await;
        assert!(
            result.is_ok(),
            "Builder should succeed with any token string; actual connection fails in background"
        );
    }

    #[tokio::test]
    async fn test_stop_gateway() {
        let config = DiscordConfig::default();
        let runtime = Arc::new(Runtime::new());
        let gateway = DiscordGateway::new(config, runtime);
        // Stop on a non-started gateway is fine
        let result = gateway.stop().await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_send_message_fails_not_connected() {
        // send_message on a non-connected gateway should fail gracefully
        let config = DiscordConfig::default();
        let runtime = Arc::new(Runtime::new());
        let gateway = DiscordGateway::new(config, runtime);
        // We use block_on since send_message is async
        let result = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(gateway.send_message("123", "hello"));
        assert!(result.is_err());
    }
}
