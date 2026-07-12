//! Raven Agent CLI — Multi-agent AI orchestration platform.
//!
//! Usage:
//!   odin orchestrate <goal> — Execute with hidden sub-agent orchestration (default)
//!   odin run --direct <task> — Single-agent execution (legacy mode)
//!   odin serve [--addr]      — Start the HTTP API server
//!   odin task submit|status|inspect|cancel|pause|resume — Manage running tasks
//!   odin agents list           — List active sub-agents
//!   odin locks list            — List file locks held by agents
//!   odin config [--show]       — Show or edit configuration
//!   odin version               — Show version information
//!   odin schedule <action>     — Manage scheduled cron jobs
//!   odin providers list        — List configured providers
//!   odin skills list|tools     — List skills and tool associations
//!   odin tools list|inspect|... — Tool ecosystem management
//!   odin audit replay <id>     — Replay audit log entries
//!   odin status                — Show runtime status

use std::path::PathBuf;
use std::sync::Arc;

use clap::{Parser, Subcommand};
use odin_core::config::OdinConfig;
use odin_core::traits::{AuditLogger, LoopEngine, Tool};
use odin_core::types::AgentTask;
use odin_orchestrator::Composer;
use odin_orchestrator::persistence::OrchestrationStore;
use odin_runtime::{Agent, Runtime};
use tracing_subscriber::EnvFilter;

// Scheduler types
use odin_scheduler::{JobId, Scheduler};

// ── CLI Definition ───────────────────────────────────────────────────

/// Raven Agent — multi-agent AI orchestration platform.
#[derive(Parser, Debug)]
#[command(
    name = "odin",
    version,
    about = "Raven Agent — multi-agent AI orchestration platform",
    long_about = "Raven Agent is a multi-agent AI orchestration platform.\\n\\\
                  It decomposes user goals, spawns hidden sub-agents with\\n\\\
                  scoped tools/files/permissions, manages file locks, and\\n\\\
                  merges parallel results into one coherent response.\\n\\\
                  Default behavior: multi-agent orchestration.",
    author = "Raven Agent Contributors"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Execute a task through the agent loop (orchestrated by default).
    Run {
        /// The task goal to execute.
        task: String,

        /// Optional config file path.
        #[arg(short, long, global = true, env = "ODIN_CONFIG")]
        config: Option<PathBuf>,

        /// Max iterations for the task.
        #[arg(short = 'n', long, default_value_t = 100)]
        max_iterations: u32,

        /// Use direct single-agent execution (skip orchestration).
        #[arg(long)]
        direct: bool,
    },

    /// Orchestrate a goal with hidden sub-agents (default mode).
    Orchestrate {
        #[command(subcommand)]
        action: OrchestrateAction,
    },

    /// Start the HTTP API server.
    Serve {
        /// Address to listen on.
        #[arg(short, long, default_value = "127.0.0.1:9177", env = "ODIN_HTTP_ADDR")]
        addr: String,

        /// Optional config file path.
        #[arg(short, long, global = true, env = "ODIN_CONFIG")]
        config: Option<PathBuf>,
    },

    /// Show or edit configuration.
    Config {
        /// Path to the config file.
        #[arg(default_value = "~/.odin/config.yaml", env = "ODIN_CONFIG")]
        path: PathBuf,

        /// Edit the config in $EDITOR.
        #[arg(short, long)]
        edit: bool,
    },

    /// Show version information.
    Version,

    /// Manage scheduled cron jobs.
    Schedule {
        #[command(subcommand)]
        action: ScheduleAction,
    },

    /// List configured providers with health status.
    Providers {
        #[command(subcommand)]
        action: ProvidersAction,
        #[arg(short, long, global = true, env = "ODIN_CONFIG")]
        config: Option<PathBuf>,
    },

    /// Manage and list skills.
    Skills {
        #[command(subcommand)]
        action: SkillsAction,
    },

    /// Manage and list tasks.
    Tasks {
        #[command(subcommand)]
        action: TasksAction,
    },

    /// List and inspect sessions.
    Sessions {
        #[command(subcommand)]
        action: SessionsAction,
    },

    /// List registered tools.
    Tools {
        #[command(subcommand)]
        action: ToolsAction,
    },

    /// Audit log operations.
    Audit {
        #[command(subcommand)]
        action: AuditAction,
    },

    /// Show runtime status summary.
    Status,
}

#[derive(Subcommand, Debug)]
enum ScheduleAction {
    /// Add a new scheduled job.
    Add {
        /// Human-readable name for this job.
        name: String,
        /// Cron expression (e.g., "0 */6 * * *").
        schedule: String,
        /// Task goal to execute when the job fires.
        task: String,
    },
    /// List all scheduled jobs.
    List,
    /// Remove a job by ID.
    Remove {
        /// Job ID (UUID) to remove.
        job_id: String,
    },
    /// Enable a disabled job by ID.
    Enable {
        /// Job ID (UUID) to enable.
        job_id: String,
    },
    /// Disable an enabled job by ID.
    Disable {
        /// Job ID (UUID) to disable.
        job_id: String,
    },
}

#[derive(Subcommand, Debug)]
enum OrchestrateAction {
    /// Submit a new task for orchestration.
    Submit {
        /// The goal to orchestrate.
        goal: String,

        /// Optional config file path.
        #[arg(short, long, global = true, env = "ODIN_CONFIG")]
        config: Option<PathBuf>,

        /// Max iterations per sub-agent.
        #[arg(short = 'n', long, default_value_t = 100)]
        max_iterations: u32,
    },
    /// Show status of the current orchestration run.
    Status,
    /// Inspect a specific task by ID.
    Inspect {
        /// Task ID.
        id: String,
    },
    /// Cancel a running task.
    Cancel {
        /// Task ID.
        id: String,
    },
    /// Pause all running sub-agents.
    Pause,
    /// Resume all paused sub-agents.
    Resume,
    /// List all active sub-agents.
    Agents,
    /// List all file locks currently held.
    Locks,
    /// List the file write queue.
    Queue,
}

#[derive(Subcommand, Debug)]
enum ProvidersAction {
    /// List all configured providers.
    List,
}

#[derive(Subcommand, Debug)]
enum SkillsAction {
    /// List available skills.
    List {
        #[arg(short, long)]
        dir: Option<String>,
    },
    /// Show required and recommended tools for a skill.
    Tools {
        /// Skill name.
        name: String,
    },
}

#[derive(Subcommand, Debug)]
enum TasksAction {
    /// List recent tasks.
    List {
        #[arg(short, long, default_value_t = 20)]
        limit: usize,
        #[arg(short, long)]
        status: Option<String>,
    },
    /// Inspect a task by ID.
    Inspect { id: String },
}

#[derive(Subcommand, Debug)]
enum SessionsAction {
    /// List active sessions.
    List,
    /// Inspect a session by ID.
    Inspect { id: String },
}

/// Manage and inspect registered tools via the `odin tools` subcommand.
#[derive(Subcommand, Debug)]
enum ToolsAction {
    /// List all registered tools.
    List {
        /// Filter by capability tag (can be specified multiple times).
        #[arg(short, long)]
        tag: Vec<String>,
    },
    /// Inspect a tool by name.
    Inspect { name: String },
    /// Validate the tool registry.
    Validate,
    /// Test a specific tool.
    Test {
        name: String,
        #[arg(short, long)]
        args: Option<String>,
        /// Run in dry-run mode — validate args but return mock result.
        #[arg(long)]
        dry_run: bool,
    },
    /// Run a comprehensive doctor check on all registered tools.
    Doctor,
    /// Show the tool catalog grouped by category.
    Catalog {
        /// Output format: table (default), json, yaml.
        #[arg(short, long, default_value = "table")]
        format: String,
        /// Filter by category (e.g., "filesystem", "shell", "web").
        #[arg(short, long)]
        category: Option<String>,
        /// Filter by tag (e.g., "safe", "dangerous", "read").
        #[arg(short, long)]
        tag: Option<String>,
    },
    /// Show reliability scores for all tools.
    Reliability,
}

#[derive(Subcommand, Debug)]
enum AuditAction {
    /// Replay audit entries for a task ID.
    Replay { task_id: String },
}

// ── Entrypoint ───────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing with env-filter
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .compact()
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Run {
            task,
            config,
            max_iterations,
            direct,
        } => cmd_run(task, config, max_iterations, direct).await,
        Commands::Orchestrate { action } => cmd_orchestrate(action).await,
        Commands::Serve { addr, config } => cmd_serve(addr, config).await,
        Commands::Config { path, edit } => cmd_config(path, edit),
        Commands::Schedule { action } => cmd_schedule(action).await,
        Commands::Providers { action, config } => cmd_providers(action, config).await,
        Commands::Skills { action } => cmd_skills(action).await,
        Commands::Tasks { action } => cmd_tasks(action).await,
        Commands::Sessions { action } => cmd_sessions(action).await,
        Commands::Tools { action } => cmd_tools(action).await,
        Commands::Audit { action } => cmd_audit(action).await,
        Commands::Version => cmd_version(),
        Commands::Status => cmd_status(),
    }
}

// ── Command Implementations ──────────────────────────────────────────

/// `odin run <task>` — Execute a task (orchestrated by default, --direct for legacy).
async fn cmd_run(
    task: String,
    config_path: Option<PathBuf>,
    max_iterations: u32,
    direct: bool,
) -> anyhow::Result<()> {
    tracing::info!("[CLI] Running task: {} (direct={})", task, direct);

    // Load configuration
    let config = load_config(config_path.as_deref())?;
    tracing::debug!("[CLI] Config loaded");

    // Find the default provider config
    let provider_name = &config.models.default_provider;
    let provider_cfg = config
        .models
        .providers
        .get(provider_name)
        .cloned()
        .unwrap_or_else(|| {
            // Fall back to a default openai_compat provider config
            odin_core::config::ProviderConfig {
                provider_type: "openai_compat".into(),
                base_url: Some("http://localhost:11434/v1".into()),
                api_key: None,
                api_key_env: None,
                default_model: None,
                headers: Default::default(),
                timeout_secs: 120,
                max_retries: 3,
                fallback_chain: None,
                health_check_interval_secs: 0,
                circuit_breaker_threshold: 0,
            }
        });

    tracing::info!(
        "[CLI] Creating provider '{}' (type: {})",
        provider_name,
        provider_cfg.provider_type
    );

    // Create the provider via the factory
    let provider: Arc<dyn odin_core::traits::Provider> =
        odin_providers::create_provider(&provider_cfg)?;

    // Create the policy engine from safety config
    let policy_engine = Arc::new(odin_permissions::PolicyEngine::new(
        config.safety.permissions.clone(),
        &config.safety.dangerous_commands,
        config.tools.path_boundary.clone(),
        config.safety.max_rate_per_minute,
        config.safety.require_approval,
    ));
    tracing::info!("[CLI] Policy engine initialized");

    // ── Shared resources (used by both orchestrated and direct paths) ──
    let tool_registry = Arc::new(odin_tools::ToolRegistry::new());
    let sandbox = Arc::new(odin_tools::Sandbox::new(
        odin_core::types::PathBoundary::default(),
    ));

    // Register built-in tools
    macro_rules! try_register_tool {
        ($registry:expr, $tool:expr) => {
            if let Err(e) = $registry.register($tool) {
                tracing::warn!("[CLI] Failed to register tool: {}", e);
            }
        };
    }

    try_register_tool!(
        tool_registry,
        Box::new(odin_tools::builtins::file::FileRead::new(sandbox.clone()))
    );
    try_register_tool!(
        tool_registry,
        Box::new(odin_tools::builtins::file::FileWrite::new(sandbox.clone()))
    );
    try_register_tool!(
        tool_registry,
        Box::new(odin_tools::builtins::shell::Shell::new())
    );

    let memory = Arc::new(
        odin_memory::SqliteMemoryStore::new(&config.general.instance_name).unwrap_or_else(|_| {
            tracing::warn!("[CLI] Failed to create memory store, using in-memory fallback");
            odin_memory::SqliteMemoryStore::in_memory().expect("in-memory store should never fail")
        }),
    );
    let audit_logger = Arc::new(odin_audit::AuditLoggerImpl::with_file(format!(
        "{}.audit.jsonl",
        config.general.instance_name
    )));
    tracing::info!("[CLI] Memory store and audit logger initialized");

    // Branch: orchestrated (default) or direct single-agent
    if !direct {
        return run_orchestrated(
            &task,
            max_iterations,
            provider,
            policy_engine,
            tool_registry,
        )
        .await;
    }

    // ── Direct single-agent mode (legacy) ──────────────────────────
    // Create the loop engine with the provider attached
    let engine = odin_loop::LoopEngine::new()
        .with_provider(provider.clone())
        .with_policy_engine(policy_engine.clone())
        .with_max_iterations(max_iterations);

    // Get available tools as Vec<Arc<dyn Tool>>
    let tools: Vec<Arc<dyn odin_core::traits::Tool>> = tool_registry
        .list_schemas()
        .iter()
        .filter_map(|s| tool_registry.get(&s.function.name))
        .collect();

    // Create the agent
    let agent = Agent::new("default-agent", Arc::new(engine), provider, tools);
    let agent_id = agent.id;

    // Register agent in runtime with memory store
    let runtime = Runtime::new()
        .with_memory(memory)
        .with_default_max_iterations(max_iterations);
    runtime.register_agent(agent);
    let session = runtime.create_session_with_label("cli-run-direct");

    // Create the task
    let agent_task = AgentTask {
        id: uuid::Uuid::new_v4(),
        goal: task.clone(),
        context: None,
        sub_tasks: vec![],
        success_criteria: vec![],
        max_iterations,
        created_at: chrono::Utc::now(),
    };

    // Log task start
    let start_entry = odin_core::types::AuditEntry {
        id: uuid::Uuid::new_v4(),
        timestamp: chrono::Utc::now(),
        agent_id,
        session_id: session.id,
        event_type: odin_core::types::AuditEventType::SessionStart,
        action: "cli_run".to_string(),
        details: serde_json::json!({
            "task": task,
            "max_iterations": max_iterations,
        }),
        result: odin_core::types::AuditResult::Success,
    };
    if let Err(e) = audit_logger.log(start_entry).await {
        tracing::warn!("[CLI] Failed to log audit start: {e}");
    }

    // Submit the task
    tracing::info!("[CLI] Submitting task '{}' to agent 'default-agent'", task);
    let start = std::time::Instant::now();
    let result = runtime
        .submit_task(&agent_id, &agent_task, Some(session.id))
        .await?;
    let elapsed = start.elapsed();

    // Log task end
    let end_entry = odin_core::types::AuditEntry {
        id: uuid::Uuid::new_v4(),
        timestamp: chrono::Utc::now(),
        agent_id,
        session_id: session.id,
        event_type: odin_core::types::AuditEventType::SessionEnd,
        action: "cli_run_complete".to_string(),
        details: serde_json::json!({
            "success": result.success,
            "iterations": result.iterations,
            "duration_ms": elapsed.as_millis(),
            "tool_calls": result.tool_calls,
            "confidence": result.confidence,
        }),
        result: if result.success {
            odin_core::types::AuditResult::Success
        } else {
            odin_core::types::AuditResult::Failure
        },
    };
    if let Err(e) = audit_logger.log(end_entry).await {
        tracing::warn!("[CLI] Failed to log audit end: {e}");
    }

    // Print the result
    println!();
    println!("╔══════════════════════════════════════════╗");
    println!("║          Task Result                     ║");
    println!("╠══════════════════════════════════════════╣");
    println!("║  Goal:    {:32} ║", &task[..task.len().min(32)]);
    println!(
        "║  Status:  {:32} ║",
        if result.success {
            "✓ COMPLETED"
        } else {
            "✗ STOPPED"
        }
    );
    println!(
        "║  Duration: {:31} ║",
        format!("{}.{:03}s", elapsed.as_secs(), elapsed.subsec_millis())
    );
    println!("║  Iterations: {:26} ║", result.iterations);
    println!("║  Tool calls: {:25} ║", result.tool_calls);
    println!(
        "║  Confidence: {:25}% ║",
        (result.confidence * 100.0) as u32
    );
    println!("╚══════════════════════════════════════════╝");
    println!();
    println!("Summary: {}", result.summary);

    if !result.sub_tasks.is_empty() {
        println!();
        println!("Sub-tasks:");
        for st in &result.sub_tasks {
            let icon = match st.status {
                odin_core::types::SubTaskStatus::Completed => "✓",
                odin_core::types::SubTaskStatus::Failed => "✗",
                odin_core::types::SubTaskStatus::InProgress => "◷",
                odin_core::types::SubTaskStatus::Pending => "○",
                odin_core::types::SubTaskStatus::Skipped => "−",
            };
            println!("  {} {} — {}", icon, st.id, st.description);
            if let Some(ref res) = st.result {
                println!("        Result: {}", res);
            }
        }
    }

    if let Some(ref err) = result.error {
        println!();
        println!("Error: {}", err);
    }

    Ok(())
}

/// Orchestrated execution — decompose goal, spawn parallel sub-agents, merge results.
async fn run_orchestrated(
    goal: &str,
    max_iterations: u32,
    provider: Arc<dyn odin_core::traits::Provider>,
    policy_engine: Arc<odin_permissions::PolicyEngine>,
    tool_registry: Arc<odin_tools::ToolRegistry>,
) -> anyhow::Result<()> {
    use odin_orchestrator::composer::ComposerConfig;
    use odin_orchestrator::merge::{MergeStrategy, SubAgentResult};
    use odin_orchestrator::sub_agent::SubAgentConfigBuilder;

    let mut composer = Composer::new(ComposerConfig {
        max_parallel: 10,
        default_max_iterations: max_iterations,
        auto_merge: true,
        merge_strategy: MergeStrategy::Concatenate,
        workspace_root: ".".to_string(),
        persist_state: true,
    });

    // Decompose the goal into a task graph
    composer.intake(goal);
    let node_count = {
        let graph = composer.get_graph(goal).unwrap();
        graph.nodes.len()
    };

    println!("╔══════════════════════════════════════════╗");
    println!("║     Raven Agent — Orchestrated Run      ║");
    println!("╠══════════════════════════════════════════╣");
    println!("║  Goal:  {:<32} ║", &goal[..goal.len().min(32)]);
    println!("║  Tasks: {:<32} ║", format!("{} sub-task(s)", node_count));
    println!("╚══════════════════════════════════════════╝");
    println!();

    // For each workstream group, spawn sub-agents.
    // Collect node data first to avoid holding an immutable borrow of composer.
    let graph = composer.get_graph(goal).unwrap();
    let groups = composer.detect_workstreams(graph);

    // Extract all node data before mutating composer
    type NodeExecutionSpec = (uuid::Uuid, String, String, Vec<String>, Vec<String>, u32);
    let mut node_tasks: Vec<(usize, Vec<NodeExecutionSpec>)> = Vec::new();
    for (group_idx, group) in groups.iter().enumerate() {
        let mut tasks = Vec::new();
        for &node_id in group {
            let node = &graph.nodes[&node_id];
            tasks.push((
                node.id,
                node.label.clone(),
                node.goal.clone(),
                node.read_files.clone(),
                node.write_files.clone(),
                node.priority,
            ));
        }
        node_tasks.push((group_idx, tasks));
    }

    let mut agent_handles = Vec::new();

    for (group_idx, tasks) in &node_tasks {
        println!(
            "🔄 Group {}: {} agent(s) dispatch...",
            group_idx + 1,
            tasks.len()
        );

        for (node_id, label, goal, read_files, write_files, priority) in tasks {
            // Create sub-agent config
            let agent_config = SubAgentConfigBuilder::new(label, goal)
                .read_files(read_files.clone())
                .write_files(write_files.clone())
                .max_iterations(max_iterations)
                .priority(*priority)
                .task_node(*node_id)
                .build();

            let agent_id = composer.register_agent(agent_config);

            // Try to start (acquires file locks or queues)
            match composer.start_agent(agent_id) {
                Ok(()) => tracing::info!("[ORCH] Agent '{}' started", label),
                Err(msg) => tracing::info!("[ORCH] Agent '{}' queued: {}", label, msg),
            }

            // Spawn async task for execution
            let provider = provider.clone();
            let policy_engine = policy_engine.clone();
            let tool_registry = tool_registry.clone();
            let goal = goal.clone();
            let label = label.clone();
            let label_for_result = label.clone();

            let handle = tokio::spawn(async move {
                let engine = odin_loop::LoopEngine::new()
                    .with_provider(provider.clone())
                    .with_policy_engine(policy_engine)
                    .with_max_iterations(max_iterations)
                    .with_tool_registry(tool_registry);

                let task = AgentTask {
                    id: uuid::Uuid::new_v4(),
                    goal,
                    context: None,
                    sub_tasks: vec![],
                    success_criteria: vec![],
                    max_iterations,
                    created_at: chrono::Utc::now(),
                };

                let start = std::time::Instant::now();
                let result = engine.execute_task(&task).await;
                let elapsed = start.elapsed();

                (agent_id, label_for_result, result, elapsed)
            });

            agent_handles.push(handle);
        }
    }

    println!();
    println!(
        "⏳ Waiting for {} sub-agent(s) to complete...",
        agent_handles.len()
    );

    let mut total_success = 0;
    let mut total_fail = 0;

    for handle in agent_handles {
        match handle.await {
            Ok((agent_id, label, result, elapsed)) => match result {
                Ok(task_result) => {
                    println!(
                        "  {} {} — {} ({}ms, {} iters)",
                        if task_result.success { "✅" } else { "⚠️" },
                        label,
                        if task_result.success {
                            "success"
                        } else {
                            "failed"
                        },
                        elapsed.as_millis(),
                        task_result.iterations,
                    );
                    composer.complete_agent(
                        agent_id,
                        SubAgentResult {
                            agent_id,
                            name: label.clone(),
                            summary: task_result.summary.clone(),
                            output: Some(task_result.summary),
                            modified_files: vec![],
                            success: task_result.success,
                            error: task_result.error.clone(),
                            duration_ms: elapsed.as_millis() as u64,
                        },
                    );
                    if task_result.success {
                        total_success += 1;
                    } else {
                        total_fail += 1;
                    }
                }
                Err(e) => {
                    println!("  ❌ {} — error: {}", label, e);
                    composer.fail_agent(agent_id, e.to_string());
                    total_fail += 1;
                }
            },
            Err(e) => {
                tracing::error!("[ORCH] Sub-agent panicked: {}", e);
                total_fail += 1;
            }
        }
    }

    // Merge and print final result
    let results = composer.collect_results();
    let merged = composer.merge_results(results);

    println!();
    println!("╔══════════════════════════════════════════╗");
    println!("║          Orchestrated Result            ║");
    println!("╠══════════════════════════════════════════╣");
    println!(
        "║  Status:  {:32} ║",
        if total_fail == 0 {
            "✓ ALL COMPLETED"
        } else {
            "✗ PARTIAL FAILURE"
        }
    );
    println!(
        "║  Success: {}/{} agents passed           ║",
        total_success,
        total_success + total_fail
    );
    println!("╚══════════════════════════════════════════╝");
    println!();
    println!("{}", merged.summary);

    if !merged.conflicts.is_empty() {
        println!();
        println!("⚠️  Merge conflicts in files:");
        for c in &merged.conflicts {
            println!("  - {} (agents: {})", c.file, c.agents.join(", "));
        }
    }

    Ok(())
}

/// Get a path in the Raven Agent state directory.
fn dirs_state_path(filename: &str) -> std::path::PathBuf {
    let base = dirs_state_dir();
    std::fs::create_dir_all(&base).ok();
    base.join(filename)
}

/// Get the Raven Agent state directory.
fn dirs_state_dir() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    std::path::PathBuf::from(home).join(".raven-agent")
}

/// `odin orchestrate` — Multi-agent orchestration commands.
async fn cmd_orchestrate(action: OrchestrateAction) -> anyhow::Result<()> {
    match action {
        OrchestrateAction::Submit {
            goal,
            config: _config,
            max_iterations: _max_iterations,
        } => {
            tracing::info!("[ORCHESTRATE] Submitting goal: {}", goal);

            // Create a persistent run ID
            let run_id = uuid::Uuid::new_v4();

            // Initialize the orchestration store
            let store = odin_orchestrator::persistence::SqliteOrchestrationStore::new(
                dirs_state_path("orchestration.db"),
            )
            .await
            .map_err(|e| anyhow::anyhow!("Failed to initialize orchestration store: {e}"))?;
            store
                .initialize()
                .await
                .map_err(|e| anyhow::anyhow!("Store init: {e}"))?;

            let mut composer = Composer::default();
            composer.intake(&goal);

            let node_count = {
                let graph = composer.get_graph(&goal).unwrap();
                graph.nodes.len()
            };

            // Persist the task graph
            if let Some(graph) = composer.get_graph(&goal) {
                let _ = store.save_task_graph(graph).await;
            }

            println!("╔══════════════════════════════════════════╗");
            println!("║     Raven Agent — Orchestration         ║");
            println!("╠══════════════════════════════════════════╣");
            println!("║  Run ID: {:<32} ║", run_id.to_string());
            println!("║  Goal:  {:<32} ║", &goal[..goal.len().min(32)]);
            println!("║  Tasks: {:<32} ║", format!("{} sub-task(s)", node_count));
            println!("╚══════════════════════════════════════════╝");
            println!();
            println!("📋 Use 'odin orchestrate status' to check progress.");
            println!("📋 Use 'odin orchestrate inspect {}' for details.", run_id);

            if node_count <= 1 {
                println!("   Single task — no parallelization needed.");
                println!("   Goal: {}", goal);
            } else {
                println!("   Decomposed into {} parallel sub-tasks:", node_count);
                if let Some(graph) = composer.get_graph(&goal) {
                    for (i, node) in graph.nodes.values().enumerate() {
                        let mut info = format!("     {}. {}", i + 1, node.label);
                        if !node.write_files.is_empty() {
                            info.push_str(&format!(" [writes: {}]", node.write_files.join(", ")));
                        }
                        println!("{}", info);
                    }
                }
            }

            // Show what workstreams were detected
            if let Some(graph) = composer.get_graph(&goal) {
                let workstreams = composer.detect_workstreams(graph);
                println!();
                println!("   Workstreams: {} parallel group(s)", workstreams.len());
                for (i, ws) in workstreams.iter().enumerate() {
                    println!("     Group {}: {} agent(s)", i + 1, ws.len());
                }
            }
            println!();
        }
        OrchestrateAction::Status => {
            let store = odin_orchestrator::persistence::SqliteOrchestrationStore::new(
                dirs_state_path("orchestration.db"),
            )
            .await
            .map_err(|e| anyhow::anyhow!("Store: {e}"))?;
            store.initialize().await.ok();

            let graphs = store.list_task_graphs().await.unwrap_or_default();
            let lifecycles = store.list_agent_lifecycles().await.unwrap_or_default();

            println!("📊 Orchestration Status");
            println!("   Stored task graphs: {}", graphs.len());
            for g in &graphs {
                println!(
                    "     - '{}' ({} nodes, {})",
                    g.root_goal, g.node_count, g.status
                );
            }
            println!("   Stored agent lifecycles: {}", lifecycles.len());
            for lc in &lifecycles {
                println!("     - {} ({})", lc.agent_id, lc.phase);
            }
            if graphs.is_empty() && lifecycles.is_empty() {
                println!(
                    "   No stored orchestration state. Use 'odin orchestrate submit' to start a run."
                );
            }
        }
        OrchestrateAction::Inspect { id } => {
            let store = odin_orchestrator::persistence::SqliteOrchestrationStore::new(
                dirs_state_path("orchestration.db"),
            )
            .await
            .map_err(|e| anyhow::anyhow!("Store: {e}"))?;
            store.initialize().await.ok();

            // Try to load as a graph by root goal
            if let Ok(graph) = store.load_task_graph(&id).await {
                println!("🔍 Task Graph: {}", graph.root_goal);
                println!("   Status: {:?}", graph.status);
                println!("   Nodes: {}", graph.nodes.len());
                for (i, node) in graph.nodes.values().enumerate() {
                    println!("     {}. {} [{:?}]", i + 1, node.label, node.status);
                    println!("        Goal: {}", node.goal);
                    if !node.write_files.is_empty() {
                        println!("        Writes: {}", node.write_files.join(", "));
                    }
                }
                return Ok(());
            }

            // Try to load as an agent lifecycle
            if let Ok(run_id) = uuid::Uuid::parse_str(&id)
                && let Ok(lc) = store.load_agent_lifecycle(run_id).await
            {
                println!("🔍 Agent Lifecycle: {}", lc.agent_id);
                println!("   Phase: {:?}", lc.phase);
                println!("   Created: {}", lc.created_at);
                if let Some(finished) = lc.finished_at {
                    println!("   Finished: {}", finished);
                }
                if let Some(err) = &lc.error {
                    println!("   Error: {}", err);
                }
                println!("   History: {} transition(s)", lc.history.len());
                return Ok(());
            }

            println!(
                "🔍 Not found: '{}' — no task graph or agent lifecycle with that ID.",
                id
            );
            println!("   Use 'odin orchestrate status' to list stored items.");
        }
        OrchestrateAction::Cancel { id } => {
            println!("🛑 Cancel requested for task: {}", id);
            println!(
                "   (Task cancellation requires active runtime — CLI orchestration is stateless)"
            );
        }
        OrchestrateAction::Pause => {
            println!("⏸️  Pause all sub-agents");
            println!("   (Pause requires active runtime — CLI orchestration is stateless)");
        }
        OrchestrateAction::Resume => {
            println!("▶️  Resume all sub-agents");
            println!("   (Resume requires active runtime — CLI orchestration is stateless)");
        }
        OrchestrateAction::Agents => {
            println!("🤖 Active Sub-Agents");
            println!("   No active sub-agents (stateless CLI mode)");
            println!();
            println!("   In a long-running server context, this would show:");
            println!("   - Agent ID, name, lifecycle phase, held locks, duration");
        }
        OrchestrateAction::Locks => {
            let composer = Composer::default();
            let summary = composer.lock_summary();
            println!("🔒 File Lock Manager");
            println!("   Total locked files: {}", summary.total_locked_files);
            println!("   Write-locked files: {}", summary.write_locked_files);
            println!("   Queued writers:     {}", summary.queued_writers);
        }
        OrchestrateAction::Queue => {
            let composer = Composer::default();
            println!("📝 Write Queue");
            println!("   No files queued (stateless CLI mode)");
            println!();
            println!("   In a multi-agent run, this would show:");
            println!("   - File path, queued agents, queue position, wait time");
            // Print lock summary for reference
            let summary = composer.lock_summary();
            println!();
            println!("   Lock summary:");
            println!("     Total locked: {}", summary.total_locked_files);
            println!("     Write-locked: {}", summary.write_locked_files);
        }
    }
    Ok(())
}

/// `odin serve` — Start the HTTP API server with a real task handler.
async fn cmd_serve(addr: String, config_path: Option<PathBuf>) -> anyhow::Result<()> {
    tracing::info!("[CLI] Starting HTTP server on {addr}");

    let config = load_config(config_path.as_deref())?;

    // Build the provider from config
    let provider_name = &config.models.default_provider;
    let provider_cfg = config
        .models
        .providers
        .get(provider_name)
        .cloned()
        .unwrap_or_else(|| odin_core::config::ProviderConfig {
            provider_type: "openai_compat".into(),
            base_url: Some("http://localhost:11434/v1".into()),
            api_key: None,
            api_key_env: None,
            default_model: None,
            headers: Default::default(),
            timeout_secs: 120,
            max_retries: 3,
            fallback_chain: None,
            health_check_interval_secs: 0,
            circuit_breaker_threshold: 0,
        });

    let provider = odin_providers::create_provider(&provider_cfg)?;
    let tool_registry = Arc::new(odin_tools::ToolRegistry::new());
    let sandbox = Arc::new(odin_tools::Sandbox::new(
        odin_core::types::PathBoundary::default(),
    ));

    // Create the policy engine from safety config
    let policy_engine = Arc::new(odin_permissions::PolicyEngine::new(
        config.safety.permissions.clone(),
        &config.safety.dangerous_commands,
        config.tools.path_boundary.clone(),
        config.safety.max_rate_per_minute,
        config.safety.require_approval,
    ));
    tracing::info!("[CLI/serve] Policy engine initialized");

    // Register built-in tools
    let _ = tool_registry.register(Box::new(odin_tools::builtins::file::FileRead::new(
        sandbox.clone(),
    )));
    let _ = tool_registry.register(Box::new(odin_tools::builtins::file::FileWrite::new(
        sandbox.clone(),
    )));
    let _ = tool_registry.register(Box::new(odin_tools::builtins::shell::Shell::new()));

    let tools: Vec<Arc<dyn odin_core::traits::Tool>> = tool_registry
        .list_schemas()
        .iter()
        .filter_map(|s| tool_registry.get(&s.function.name))
        .collect();

    // Wire persistent memory store
    let memory = Arc::new(
        odin_memory::SqliteMemoryStore::new(&config.general.instance_name).unwrap_or_else(|_| {
            tracing::warn!("[CLI/serve] Failed to create memory store, using in-memory fallback");
            odin_memory::SqliteMemoryStore::in_memory().expect("in-memory store should never fail")
        }),
    );
    tracing::info!("[CLI/serve] Memory store initialized");

    // Wire audit logger
    let audit_logger = Arc::new(odin_audit::AuditLoggerImpl::with_file(format!(
        "{}.audit.jsonl",
        config.general.instance_name
    )));
    tracing::info!("[CLI/serve] Audit logger initialized");

    // Build the task handler closure
    let handler: odin_gateway::TaskHandlerFn = {
        let memory = memory;
        let audit_logger = audit_logger;
        Arc::new(move |req: odin_gateway::ChatRequest| {
            let provider = provider.clone();
            let tool_registry = tool_registry.clone();
            let policy_engine = policy_engine.clone();
            let tools = tools.clone();
            let memory = memory.clone();
            let audit_logger = audit_logger.clone();
            Box::pin(async move {
                let start = std::time::Instant::now();

                let engine = odin_loop::LoopEngine::new()
                    .with_provider(provider.clone())
                    .with_policy_engine(policy_engine.clone())
                    .with_max_iterations(req.max_iterations.unwrap_or(100))
                    .with_tool_registry(tool_registry.clone());

                let agent = Agent::new(
                    "serve-agent",
                    Arc::new(engine),
                    provider.clone(),
                    tools.clone(),
                );
                let agent_id = agent.id;

                let runtime = Runtime::new().with_memory(memory.clone());
                runtime.register_agent(agent);

                // Parse session_id if provided
                let session_id = req
                    .session_id
                    .clone()
                    .and_then(|sid| uuid::Uuid::parse_str(&sid).ok());

                let runtime_task = AgentTask {
                    id: uuid::Uuid::new_v4(),
                    goal: req.task.clone(),
                    context: req.context.clone(),
                    sub_tasks: vec![],
                    success_criteria: vec![],
                    max_iterations: req.max_iterations.unwrap_or(100),
                    created_at: chrono::Utc::now(),
                };

                // Log task start
                let start_entry = odin_core::types::AuditEntry {
                    id: uuid::Uuid::new_v4(),
                    timestamp: chrono::Utc::now(),
                    agent_id,
                    session_id: session_id.unwrap_or_default(),
                    event_type: odin_core::types::AuditEventType::SessionStart,
                    action: "serve_run".to_string(),
                    details: serde_json::json!({
                        "task": req.task,
                        "max_iterations": req.max_iterations.unwrap_or(100),
                    }),
                    result: odin_core::types::AuditResult::Success,
                };
                if let Err(e) = audit_logger.log(start_entry).await {
                    tracing::warn!("[CLI/serve] Failed to log audit start: {e}");
                }

                let result = runtime
                    .submit_task(&agent_id, &runtime_task, session_id)
                    .await
                    .map_err(|e| odin_core::error::OdinError::Internal(e.to_string()))?;

                let elapsed = start.elapsed().as_millis() as u64;

                // Log task end
                let end_entry = odin_core::types::AuditEntry {
                    id: uuid::Uuid::new_v4(),
                    timestamp: chrono::Utc::now(),
                    agent_id,
                    session_id: result.task_id,
                    event_type: odin_core::types::AuditEventType::SessionEnd,
                    action: "serve_run_complete".to_string(),
                    details: serde_json::json!({
                        "success": result.success,
                        "iterations": result.iterations,
                        "duration_ms": elapsed,
                        "tool_calls": result.tool_calls,
                        "confidence": result.confidence,
                    }),
                    result: if result.success {
                        odin_core::types::AuditResult::Success
                    } else {
                        odin_core::types::AuditResult::Failure
                    },
                };
                if let Err(e) = audit_logger.log(end_entry).await {
                    tracing::warn!("[CLI/serve] Failed to log audit end: {e}");
                }

                Ok(odin_gateway::ChatResponse {
                    success: result.success,
                    summary: result.summary,
                    iterations: result.iterations,
                    tool_calls: result.tool_calls,
                    duration_ms: elapsed.max(result.duration_ms),
                    confidence: result.confidence,
                    error: result.error,
                })
            })
        })
    };

    println!("╔══════════════════════════════════════════╗");
    println!("║     Raven Agent — API Gateway           ║");
    println!("╠══════════════════════════════════════════╣");
    println!("║  HTTP API: http://{addr:<15}  ║", addr = addr);
    println!("║  Health:   http://{addr:<15}/health  ║", addr = addr);
    println!("║  Chat:     POST http://{addr:<15}/chat  ║", addr = addr);
    println!("╚══════════════════════════════════════════╝");
    println!();

    odin_gateway::run_http_server(&addr, Some(handler))
        .await
        .map_err(|e| anyhow::anyhow!("Server error: {e}"))?;

    Ok(())
}

/// `odin config` — Show or edit configuration.
fn cmd_config(path: PathBuf, edit: bool) -> anyhow::Result<()> {
    let expanded_path = shellexpand::tilde(&path.to_string_lossy()).to_string();
    let config_path = PathBuf::from(&expanded_path);

    if edit {
        // Open the config in the user's editor
        let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vim".into());
        let status = std::process::Command::new(&editor)
            .arg(&config_path)
            .status()
            .map_err(|e| anyhow::anyhow!("Failed to open editor '{}': {e}", editor))?;

        if !status.success() {
            anyhow::bail!("Editor exited with non-zero status");
        }

        println!("Config saved to: {}", config_path.display());
    } else {
        // Show config
        if config_path.exists() {
            let contents = std::fs::read_to_string(&config_path).map_err(|e| {
                anyhow::anyhow!("Failed to read config {}: {e}", config_path.display())
            })?;
            println!("Configuration file: {}", config_path.display());
            println!("---");
            println!("{}", contents);
        } else {
            println!("No config file found at {}", config_path.display());
            println!("Creating with defaults...");

            let config = OdinConfig::default();
            if let Some(parent) = config_path.parent() {
                std::fs::create_dir_all(parent).ok();
            }
            config
                .save(&config_path)
                .map_err(|e| anyhow::anyhow!("Failed to save config: {e}"))?;

            println!("Created default config at {}", config_path.display());
        }
    }

    Ok(())
}

/// `odin version` — Show version information.
fn cmd_version() -> anyhow::Result<()> {
    println!("Odin CLI (Raven Agent)");
    println!("Version:    {}", env!("CARGO_PKG_VERSION"));
    println!("Repository: https://github.com/hermes-gadget/raven-agent");
    println!("License:    {}", env!("CARGO_PKG_LICENSE"));
    println!();

    println!("Crate versions:");
    println!(
        "  odin-core:    {}",
        odin_core::OdinConfig::default().general.instance_name
    );
    println!("  odin-runtime: {}", env!("CARGO_PKG_VERSION"));
    println!("  odin-gateway: {}", env!("CARGO_PKG_VERSION"));
    println!("  odin-skills:  {}", env!("CARGO_PKG_VERSION"));
    println!("  odin-loop:    {}", env!("CARGO_PKG_VERSION"));
    println!();

    println!("Built with:");
    println!("  Rust: {}", rustc_version());
    println!("  Profile: {}", build_profile());
    println!("  Target:  {}", std::env::consts::ARCH);

    Ok(())
}

/// `odin schedule` — Manage scheduled cron jobs.
async fn cmd_schedule(action: ScheduleAction) -> anyhow::Result<()> {
    let scheduler = Scheduler::default();

    match action {
        ScheduleAction::Add {
            name,
            schedule,
            task,
        } => {
            // Parse the schedule to validate it before adding
            if let Err(e) = odin_scheduler::Schedule::parse(&schedule) {
                anyhow::bail!("Invalid cron expression '{}': {}", schedule, e);
            }

            let goal = task.clone();
            let job_task: odin_scheduler::JobTask = std::sync::Arc::new(move || {
                let g = goal.clone();
                Box::pin(async move {
                    tracing::info!("[Scheduler] Running task '{}'", g);
                    println!("[Scheduler] Running task: {}", g);
                })
            });

            let job_id = scheduler
                .add_job(&name, &schedule, job_task)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to add job: {}", e))?;
            println!("Job '{}' added with ID: {}", name, job_id);
        }
        ScheduleAction::List => {
            let jobs = scheduler.list_jobs().await;
            if jobs.is_empty() {
                println!("No scheduled jobs.");
            } else {
                println!("Scheduled jobs:");
                println!();
                for job in &jobs {
                    let status = if job.enabled { "enabled" } else { "disabled" };
                    println!(
                        "  {} — {} ({}) [{}]",
                        job.id, job.name, job.schedule, status
                    );
                    println!(
                        "         Last run: {}",
                        job.last_run.map_or("never".into(), |t| t.to_rfc3339())
                    );
                    println!(
                        "         Next run: {}",
                        job.next_run.map_or("none".into(), |t| t.to_rfc3339())
                    );
                    println!("         Runs: {}", job.run_count);
                }
            }
        }
        ScheduleAction::Remove { job_id } => {
            let id: JobId = job_id
                .parse()
                .map_err(|e| anyhow::anyhow!("Invalid job ID '{}': {}", job_id, e))?;
            match scheduler.remove_job(id).await {
                Ok(true) => println!("Job '{}' removed.", job_id),
                Ok(false) => println!("Job '{}' not found.", job_id),
                Err(e) => anyhow::bail!("Failed to remove job: {}", e),
            }
        }
        ScheduleAction::Enable { job_id } => {
            let id: JobId = job_id
                .parse()
                .map_err(|e| anyhow::anyhow!("Invalid job ID '{}': {}", job_id, e))?;
            match scheduler.set_job_enabled(id, true).await {
                Ok(true) => println!("Job '{}' enabled.", job_id),
                Ok(false) => println!("Job '{}' not found.", job_id),
                Err(e) => anyhow::bail!("Failed to enable job: {}", e),
            }
        }
        ScheduleAction::Disable { job_id } => {
            let id: JobId = job_id
                .parse()
                .map_err(|e| anyhow::anyhow!("Invalid job ID '{}': {}", job_id, e))?;
            match scheduler.set_job_enabled(id, false).await {
                Ok(true) => println!("Job '{}' disabled.", job_id),
                Ok(false) => println!("Job '{}' not found.", job_id),
                Err(e) => anyhow::bail!("Failed to disable job: {}", e),
            }
        }
    }

    Ok(())
}

/// `odin providers list` — List configured providers.
async fn cmd_providers(
    action: ProvidersAction,
    config_path: Option<PathBuf>,
) -> anyhow::Result<()> {
    match action {
        ProvidersAction::List => {
            let config = load_config(config_path.as_deref())?;
            let providers = &config.models.providers;
            let default = &config.models.default_provider;

            if providers.is_empty() {
                println!("No providers configured.");
                return Ok(());
            }

            println!("Configured providers:");
            println!();
            for (name, cfg) in providers {
                let is_default = if name == default { " (default)" } else { "" };
                println!(
                    "  {name}{is_default}:",
                    name = name,
                    is_default = is_default
                );
                println!("    Type:      {}", cfg.provider_type);
                if let Some(ref url) = cfg.base_url {
                    println!("    Base URL:  {}", url);
                }
                if let Some(ref model) = cfg.default_model {
                    println!("    Model:     {}", model);
                }
                println!();
            }
        }
    }

    Ok(())
}

/// `odin skills list` — List available skills.
async fn cmd_skills(action: SkillsAction) -> anyhow::Result<()> {
    match action {
        SkillsAction::List { dir } => {
            let skills_dir = dir
                .map(PathBuf::from)
                .or_else(|| {
                    let default_path = shellexpand::tilde("~/.odin/skills");
                    Some(PathBuf::from(default_path.to_string()))
                })
                .unwrap();

            if !skills_dir.exists() {
                println!("Skills directory not found: {}", skills_dir.display());
                println!("Create it with: mkdir -p {}", skills_dir.display());
                return Ok(());
            }

            let registry = odin_skills::SkillRegistry::load_from_dir(&skills_dir)
                .map_err(|e| anyhow::anyhow!("Failed to load skills: {e}"))?;
            let skills = registry.all();
            let enabled = registry.enabled();

            if skills.is_empty() {
                println!("No skills found in {}", skills_dir.display());
                return Ok(());
            }

            // Build available tool names from the tool registry for validation
            let tool_registry = build_tool_registry();
            let available: Vec<String> = tool_registry
                .all_tools()
                .iter()
                .map(|t| t.name().to_string())
                .collect();
            let validations = registry.validate_tools(&available);

            println!("Skills ({}/{} enabled):", enabled.len(), skills.len());
            println!();
            for skill in &skills {
                let status = if skill.enabled { "✓" } else { " " };
                let req_count = skill.required_tools.len();
                let rec_count = skill.recommended_tools.len();
                let tool_info = if req_count > 0 || rec_count > 0 {
                    format!(" [{} req, {} rec]", req_count, rec_count)
                } else {
                    String::new()
                };
                println!(
                    "  {status} {name:<30} {desc}{tool_info}",
                    name = skill.name,
                    desc = skill.description
                );
            }

            // Show validation warnings
            if !validations.is_empty() {
                println!();
                println!("⚠  Tool availability warnings:");
                for v in &validations {
                    if v.has_errors {
                        println!(
                            "  ✗ {} — missing required: {}",
                            v.skill_name,
                            v.missing_required.join(", ")
                        );
                    } else if !v.missing_required.is_empty() {
                        println!(
                            "  ⚠ {} — missing required: {}",
                            v.skill_name,
                            v.missing_required.join(", ")
                        );
                    }
                    if !v.missing_recommended.is_empty() {
                        println!(
                            "  ℹ {} — missing recommended: {}",
                            v.skill_name,
                            v.missing_recommended.join(", ")
                        );
                    }
                }
            }

            println!();
            println!("Skills directory: {}", skills_dir.display());
        }
        SkillsAction::Tools { name } => {
            let default_dir = shellexpand::tilde("~/.odin/skills");
            let skills_dir = PathBuf::from(default_dir.to_string());

            let registry = if skills_dir.exists() {
                odin_skills::SkillRegistry::load_from_dir(&skills_dir)
                    .map_err(|e| anyhow::anyhow!("Failed to load skills: {e}"))?
            } else {
                println!("Skills directory not found: {}", skills_dir.display());
                return Ok(());
            };

            match registry.tools_for_skill(&name) {
                Some(tools) => {
                    println!("Skill: {}", tools.skill_name);
                    println!();
                    if tools.required.is_empty() && tools.recommended.is_empty() {
                        println!("  No tool dependencies declared.");
                    } else {
                        if !tools.required.is_empty() {
                            println!("  Required tools:");
                            for t in &tools.required {
                                println!("    - {t}");
                            }
                        }
                        if !tools.recommended.is_empty() {
                            println!("  Recommended tools:");
                            for t in &tools.recommended {
                                println!("    - {t}");
                            }
                        }
                    }

                    // Cross-check with tool registry
                    let tool_registry = build_tool_registry();
                    let available: Vec<String> = tool_registry
                        .all_tools()
                        .iter()
                        .map(|t| t.name().to_string())
                        .collect();

                    let missing_req: Vec<_> = tools
                        .required
                        .iter()
                        .filter(|t| !available.contains(t))
                        .collect();
                    let missing_rec: Vec<_> = tools
                        .recommended
                        .iter()
                        .filter(|t| !available.contains(t))
                        .collect();

                    if !missing_req.is_empty() {
                        println!();
                        println!("  ⚠  Missing required tools (skill may not work):");
                        for t in &missing_req {
                            println!("    - {t}");
                        }
                    }
                    if !missing_rec.is_empty() {
                        println!();
                        println!("  ℹ  Unavailable recommended tools:");
                        for t in &missing_rec {
                            println!("    - {t}");
                        }
                    }
                    if missing_req.is_empty() && missing_rec.is_empty() {
                        println!();
                        println!("  ✓ All tool dependencies satisfied.");
                    }
                }
                None => {
                    println!("Skill '{name}' not found.");
                    println!();
                    println!("Available skills:");
                    for skill in registry.all() {
                        println!("  - {}", skill.name);
                    }
                }
            }
        }
    }

    Ok(())
}

/// `odin tasks` — List and inspect tasks.
async fn cmd_tasks(action: TasksAction) -> anyhow::Result<()> {
    match action {
        TasksAction::List { limit, status } => {
            let config = load_config(None)?;
            let audit_path = format!("{}.audit.jsonl", config.general.instance_name);
            let path = std::path::Path::new(&audit_path);

            if !path.exists() {
                println!("No audit log found at {}", audit_path);
                return Ok(());
            }

            let contents = std::fs::read_to_string(path)
                .map_err(|e| anyhow::anyhow!("Failed to read audit log: {e}"))?;

            let mut entries = Vec::new();
            for line in contents.lines().rev() {
                if entries.len() >= limit {
                    break;
                }
                if let Ok(entry) = serde_json::from_str::<odin_core::types::AuditEntry>(line) {
                    // Filter by status if provided
                    if let Some(ref s) = status {
                        let status_match = match s.as_str() {
                            "success" => entry.result == odin_core::types::AuditResult::Success,
                            "failure" | "fail" => {
                                entry.result == odin_core::types::AuditResult::Failure
                            }
                            _ => true,
                        };
                        if !status_match {
                            continue;
                        }
                    }
                    entries.push(entry);
                }
            }

            if entries.is_empty() {
                println!("No tasks found in audit log.");
                return Ok(());
            }

            println!("Recent tasks (last {}):", entries.len());
            println!();
            for entry in &entries {
                let icon = match entry.result {
                    odin_core::types::AuditResult::Success => "✓",
                    odin_core::types::AuditResult::Failure => "✗",
                    odin_core::types::AuditResult::Denied => "−",
                    odin_core::types::AuditResult::Pending => "○",
                };
                println!(
                    "  {icon} {id} [{event}] {action} — {ts}",
                    id = entry.id,
                    event = entry.event_type,
                    action = entry.action,
                    ts = entry.timestamp.to_rfc3339()
                );
            }
        }
        TasksAction::Inspect { id } => {
            let config = load_config(None)?;
            let audit_path = format!("{}.audit.jsonl", config.general.instance_name);
            let path = std::path::Path::new(&audit_path);

            if !path.exists() {
                println!("No audit log found at {}", audit_path);
                return Ok(());
            }

            let contents = std::fs::read_to_string(path)
                .map_err(|e| anyhow::anyhow!("Failed to read audit log: {e}"))?;

            let target_id = uuid::Uuid::parse_str(&id)
                .map_err(|e| anyhow::anyhow!("Invalid task ID '{id}': {e}"))?;

            for line in contents.lines() {
                if let Ok(entry) = serde_json::from_str::<odin_core::types::AuditEntry>(line)
                    && entry.id == target_id
                {
                    println!("Task: {}", entry.id);
                    println!("  Event:    {}", entry.event_type);
                    println!("  Action:   {}", entry.action);
                    println!("  Agent:    {}", entry.agent_id);
                    println!("  Session:  {}", entry.session_id);
                    println!("  Time:     {}", entry.timestamp.to_rfc3339());
                    println!("  Result:   {:?}", entry.result);
                    println!(
                        "  Details:  {}",
                        serde_json::to_string_pretty(&entry.details)?
                    );
                    return Ok(());
                }
            }

            println!("Task '{id}' not found in audit log.");
        }
    }

    Ok(())
}

/// `odin sessions` — List and inspect sessions.
async fn cmd_sessions(action: SessionsAction) -> anyhow::Result<()> {
    match action {
        SessionsAction::List => {
            let config = load_config(None)?;
            let audit_path = format!("{}.audit.jsonl", config.general.instance_name);
            let path = std::path::Path::new(&audit_path);

            if !path.exists() {
                println!("No audit log found at {}", audit_path);
                return Ok(());
            }

            let contents = std::fs::read_to_string(path)
                .map_err(|e| anyhow::anyhow!("Failed to read audit log: {e}"))?;

            let mut sessions: std::collections::BTreeMap<
                uuid::Uuid,
                Vec<odin_core::types::AuditEntry>,
            > = std::collections::BTreeMap::new();

            for line in contents.lines() {
                if let Ok(entry) = serde_json::from_str::<odin_core::types::AuditEntry>(line) {
                    sessions.entry(entry.session_id).or_default().push(entry);
                }
            }

            if sessions.is_empty() {
                println!("No sessions found.");
                return Ok(());
            }

            println!("Sessions ({}):", sessions.len());
            println!();
            for (session_id, entries) in &sessions {
                let first_ts = entries.first().map(|e| e.timestamp.to_rfc3339());
                let last_ts = entries.last().map(|e| e.timestamp.to_rfc3339());
                println!("  Session: {session_id}");
                println!("    Entries: {}", entries.len());
                if let Some(ref ts) = first_ts {
                    println!("    First:   {ts}");
                }
                if let Some(ref ts) = last_ts {
                    println!("    Last:    {ts}");
                }
                let has_error = entries
                    .iter()
                    .any(|e| e.result == odin_core::types::AuditResult::Failure);
                if has_error {
                    println!("    Status:  ⚠ has failures");
                } else {
                    println!("    Status:  ✓ all ok");
                }
                println!();
            }
        }
        SessionsAction::Inspect { id } => {
            let config = load_config(None)?;
            let audit_path = format!("{}.audit.jsonl", config.general.instance_name);
            let path = std::path::Path::new(&audit_path);

            if !path.exists() {
                println!("No audit log found at {}", audit_path);
                return Ok(());
            }

            let contents = std::fs::read_to_string(path)
                .map_err(|e| anyhow::anyhow!("Failed to read audit log: {e}"))?;

            let session_id = uuid::Uuid::parse_str(&id)
                .map_err(|e| anyhow::anyhow!("Invalid session ID '{id}': {e}"))?;

            let mut entries = Vec::new();
            for line in contents.lines() {
                if let Ok(entry) = serde_json::from_str::<odin_core::types::AuditEntry>(line)
                    && entry.session_id == session_id
                {
                    entries.push(entry);
                }
            }

            if entries.is_empty() {
                println!("Session '{id}' not found.");
                return Ok(());
            }

            println!("Session: {id}");
            println!("  Entries: {}", entries.len());
            println!();
            for entry in &entries {
                let icon = match entry.result {
                    odin_core::types::AuditResult::Success => "✓",
                    odin_core::types::AuditResult::Failure => "✗",
                    odin_core::types::AuditResult::Denied => "−",
                    odin_core::types::AuditResult::Pending => "○",
                };
                println!(
                    "  {icon} {action:<30} [{event:<15}] {ts}",
                    action = entry.action,
                    event = format!("{:?}", entry.event_type),
                    ts = entry.timestamp.to_rfc3339()
                );
            }
        }
    }

    Ok(())
}

/// `odin tools` — Manage and inspect registered tools.
async fn cmd_tools(action: ToolsAction) -> anyhow::Result<()> {
    match action {
        ToolsAction::List { tag } => {
            let registry = build_tool_registry();
            let all_tools = registry.all_tools();

            // Filter by tags if specified
            let tools: Vec<_> = if tag.is_empty() {
                all_tools
            } else {
                all_tools
                    .into_iter()
                    .filter(|t| {
                        let tt = t.capability_tags();
                        tag.iter()
                            .all(|filter_tag| tt.contains(&filter_tag.as_str()))
                    })
                    .collect()
            };

            if tools.is_empty() {
                println!("No tools registered.");
                if !tag.is_empty() {
                    println!("(Filtered by tags: {:?})", tag);
                }
                return Ok(());
            }

            let filter_note = if tag.is_empty() {
                String::new()
            } else {
                format!(" [filtered by tags: {:?}]", tag)
            };

            println!("╔══════════════════════════════════════════╗");
            println!(
                "║         Registered Tools ({:>2}){:>12} ║",
                tools.len(),
                filter_note
            );
            println!("╠══════════════════════════════════════════╣");

            let mut sorted = tools.clone();
            sorted.sort_by(|a, b| a.name().cmp(b.name()));
            for tool in &sorted {
                let dangerous = if tool.is_dangerous() { " ⚠" } else { "  " };
                let name = tool.name();
                let truncated = if name.len() > 28 {
                    format!("{}…", &name[..27])
                } else {
                    name.to_string()
                };
                println!("║ {dangerous} {truncated:<30} ║",);
            }
            println!("╚══════════════════════════════════════════╝");
        }
        ToolsAction::Inspect { name } => {
            let registry = build_tool_registry();
            match registry.get(&name) {
                Some(tool) => {
                    let schema = tool.schema();
                    println!("Tool: {}", tool.name());
                    println!("  Description:  {}", tool.description());
                    println!("  Dangerous:    {}", tool.is_dangerous());
                    println!("  Safe:         {}", tool.is_safe());
                    println!("  Requires approval: {}", tool.requires_approval());
                    println!("  Capabilities: {:?}", tool.capability_tags());
                    println!();
                    println!("  Schema:");
                    println!(
                        "    {}",
                        serde_json::to_string_pretty(&schema).unwrap_or_else(|_| "{}".into())
                    );
                }
                None => {
                    println!("Tool '{name}' not found in registry.");
                    println!();
                    println!("Available tools:");
                    let registry = build_tool_registry();
                    let tools = registry.all_tools();
                    let mut names: Vec<String> =
                        tools.iter().map(|t| t.name().to_string()).collect();
                    names.sort();
                    for n in &names {
                        println!("  - {n}");
                    }
                }
            }
        }
        ToolsAction::Validate => {
            let registry = build_tool_registry();
            let tools = registry.all_tools();
            let mut valid = true;

            println!("Validating tool registry ({} tools)...", tools.len());
            println!();

            for tool in &tools {
                let name = tool.name();
                let mut issues = Vec::new();

                if name.is_empty() {
                    issues.push("empty name");
                }
                if tool.description().is_empty() {
                    issues.push("no description");
                }

                if issues.is_empty() {
                    println!("  ✓ {name}");
                } else {
                    println!("  ✗ {name} — {}", issues.join(", "));
                    valid = false;
                }
            }

            if valid {
                println!();
                println!("All tools valid.");
            } else {
                println!();
                println!("Validation failed — some tools have issues.");
                std::process::exit(1);
            }
        }
        ToolsAction::Test {
            name,
            args,
            dry_run,
        } => {
            let registry = build_tool_registry();
            match registry.get(&name) {
                Some(tool) => {
                    let json_args: serde_json::Value = match args {
                        Some(ref inline) => serde_json::from_str(inline)
                            .map_err(|e| anyhow::anyhow!("Invalid JSON args: {e}"))?,
                        None => serde_json::json!({}),
                    };

                    if dry_run {
                        // Wrap in DryRunTool for safe execution
                        let dry_tool = odin_tools::DryRunTool::new(tool);
                        println!("[DRY RUN] Testing tool: {}", dry_tool.name());
                        let context = odin_core::traits::ToolContext {
                            agent_id: uuid::Uuid::new_v4(),
                            session_id: uuid::Uuid::new_v4(),
                            working_dir: std::env::current_dir().unwrap_or_default(),
                            env: std::env::vars().collect(),
                        };
                        let start = std::time::Instant::now();
                        match dry_tool.execute(json_args, &context).await {
                            Ok(result) => {
                                let elapsed = start.elapsed();
                                println!(
                                    "  Status:   {}",
                                    if result.success {
                                        "✓ PASS"
                                    } else {
                                        "✗ FAIL"
                                    }
                                );
                                println!("  Duration: {:.3}s", elapsed.as_secs_f64());
                                println!("  Output:   {}", result.output);
                                if let Some(ref err) = result.error {
                                    println!("  Error:    {err}");
                                }
                            }
                            Err(e) => {
                                println!("  Error: {e}");
                                std::process::exit(1);
                            }
                        }
                    } else {
                        // Real execution
                        println!("Testing tool: {}", tool.name());
                        println!("  Description: {}", tool.description());
                        println!("  Args:        {}", serde_json::to_string(&json_args)?);
                        println!();
                        let context = odin_core::traits::ToolContext {
                            agent_id: uuid::Uuid::new_v4(),
                            session_id: uuid::Uuid::new_v4(),
                            working_dir: std::env::current_dir().unwrap_or_default(),
                            env: std::env::vars().collect(),
                        };
                        let start = std::time::Instant::now();
                        match tool.execute(json_args, &context).await {
                            Ok(result) => {
                                let elapsed = start.elapsed();
                                println!(
                                    "  Status:   {}",
                                    if result.success {
                                        "✓ PASS"
                                    } else {
                                        "✗ FAIL"
                                    }
                                );
                                println!("  Duration: {:.3}s", elapsed.as_secs_f64());
                                println!("  Output:   {}", result.output);
                                if let Some(ref err) = result.error {
                                    println!("  Error:    {err}");
                                }
                            }
                            Err(e) => {
                                println!("  Error: {e}");
                                std::process::exit(1);
                            }
                        }
                    }
                }
                None => {
                    println!("Tool '{name}' not found.");
                    std::process::exit(1);
                }
            }
        }
        ToolsAction::Doctor => {
            let registry = build_tool_registry();
            let report = odin_tools::ToolDoctor::check(&registry);
            println!("Tool Doctor: {} tools checked", report.summary.total_tools);
            println!(
                "  Passed: {}, Failed: {}, Warnings: {}",
                report.summary.passed, report.summary.failed, report.summary.warnings
            );
            for tc in &report.tool_checks {
                let icon = if tc
                    .checks
                    .iter()
                    .any(|c| c.status == odin_tools::DoctorCheckStatus::Fail)
                {
                    "✗"
                } else {
                    "✓"
                };
                println!("  {icon} {}", tc.tool_name);
            }
            for ec in &report.ecosystem_checks {
                println!("  Ecosystem: {} — {:?}", ec.name, ec.status);
            }
            if report.summary.failed > 0 {
                std::process::exit(1);
            }
        }
        ToolsAction::Catalog {
            format,
            category,
            tag,
        } => {
            let registry = build_tool_registry();
            let catalog = odin_tools::ToolCatalog::from_registry(&registry);
            match format.as_str() {
                "json" => println!("{}", serde_json::to_string_pretty(&catalog)?),
                "yaml" => println!("{}", serde_yaml::to_string(&catalog)?),
                _ => {
                    // Table output showing categories and tools
                    let filtered_categories: Vec<String> = if let Some(ref cat) = category {
                        if catalog.by_category.contains_key(cat) {
                            vec![cat.clone()]
                        } else {
                            vec![]
                        }
                    } else {
                        catalog.categories().iter().map(|c| c.to_string()).collect()
                    };

                    for cat_name in &filtered_categories {
                        if let Some(group) = catalog.by_category(cat_name) {
                            let tools: Vec<&odin_tools::CatalogEntry> = if let Some(ref t) = tag {
                                group
                                    .tools
                                    .iter()
                                    .filter(|e| e.tags.iter().any(|gt| gt == t))
                                    .collect()
                            } else {
                                group.tools.iter().collect()
                            };

                            if tools.is_empty() {
                                continue;
                            }

                            println!("\n── {} ── {}", cat_name, group.description);
                            for entry in &tools {
                                print_tool_entry(entry);
                            }
                        }
                    }

                    println!(
                        "\n{} tools in {} categories",
                        catalog.total,
                        catalog.categories().len()
                    );
                }
            }
        }
        ToolsAction::Reliability => {
            let registry = build_tool_registry();
            let tools = registry.all_tools();
            let tracker = odin_tools::ReliabilityTracker::default();

            // For demo: simulate some calls based on existing tool type
            for tool in &tools {
                let name = tool.name().to_string();
                if tool.is_safe() {
                    tracker.record_success(&name, 10);
                    tracker.record_success(&name, 15);
                    tracker.record_success(&name, 12);
                } else {
                    tracker.record_success(&name, 50);
                    tracker.record_failure(&name, 100, "simulated dry-run failure");
                }
            }

            let scores = tracker.all();
            println!("\n╔══════════════════════════════════════════════════════════════╗");
            println!("║              Tool Reliability Scores                        ║");
            println!("╠══════════════════════════════════════════════════════════════╣");
            println!("║ Tool                    Score   Calls   Rate    Unreliable  ║");
            println!("╠══════════════════════════════════════════════════════════════╣");
            for s in &scores {
                let status = if s.is_unreliable {
                    "⚠ YES "
                } else {
                    "  no  "
                };
                let name = &s.tool_name;
                let display_name = if name.len() > 22 {
                    format!("{}…", &name[..21])
                } else {
                    name.to_string()
                };
                println!(
                    "║ {:<22}  {:.3}  {:>5}  {:.3}  {}      ║",
                    display_name, s.score, s.total_calls, s.success_rate, status
                );
            }
            println!("╚══════════════════════════════════════════════════════════════╝");
            println!();
            println!("Note: Scores are based on simulated call data for demonstration.");
            println!("      In production, scores come from actual tool execution history.");
        }
    }
    Ok(())
}

/// `odin audit replay <id>` — Replay audit entries for a task.
async fn cmd_audit(action: AuditAction) -> anyhow::Result<()> {
    match action {
        AuditAction::Replay { task_id } => {
            let config = load_config(None)?;
            let audit_path = format!("{}.audit.jsonl", config.general.instance_name);
            let path = std::path::Path::new(&audit_path);

            if !path.exists() {
                println!("No audit log found at {}", audit_path);
                return Ok(());
            }

            let contents = std::fs::read_to_string(path)
                .map_err(|e| anyhow::anyhow!("Failed to read audit log: {e}"))?;

            let target_id = uuid::Uuid::parse_str(&task_id)
                .map_err(|e| anyhow::anyhow!("Invalid task ID '{task_id}': {e}"))?;

            let mut entries = Vec::new();
            for line in contents.lines() {
                if let Ok(entry) = serde_json::from_str::<odin_core::types::AuditEntry>(line)
                    && (entry.id == target_id || entry.session_id == target_id)
                {
                    entries.push(entry);
                }
            }

            if entries.is_empty() {
                println!("No audit entries found for task '{task_id}'.");
                return Ok(());
            }

            println!("Audit replay for task '{task_id}':");
            println!();

            for entry in &entries {
                let icon = match entry.result {
                    odin_core::types::AuditResult::Success => "✓",
                    odin_core::types::AuditResult::Failure => "✗",
                    odin_core::types::AuditResult::Denied => "−",
                    odin_core::types::AuditResult::Pending => "○",
                };
                println!(
                    "  {icon} [{ts}] {event:<15} | {action}",
                    ts = entry.timestamp.to_rfc3339(),
                    event = format!("{:?}", entry.event_type),
                    action = entry.action,
                );
                println!(
                    "       agent={agent} session={session} id={id} result={result:?}",
                    agent = entry.agent_id,
                    session = entry.session_id,
                    id = entry.id,
                    result = entry.result,
                );
                if !entry.details.is_null() {
                    println!(
                        "       details: {}",
                        serde_json::to_string(&entry.details).unwrap_or_default()
                    );
                }
                println!();
            }

            println!("{} entries replayed.", entries.len());
        }
    }

    Ok(())
}

/// `odin status` — Show runtime status summary.
fn cmd_status() -> anyhow::Result<()> {
    let config = load_config(None)?;

    println!("╔══════════════════════════════════════════╗");
    println!("║       Odin Runtime Status                ║");
    println!("╠══════════════════════════════════════════╣");
    println!("║  Version:    {:30} ║", env!("CARGO_PKG_VERSION"));
    println!("║  Instance:   {:30} ║", config.general.instance_name);
    println!("║  Provider:   {:30} ║", config.models.default_provider);
    println!(
        "║  Debug:      {:30} ║",
        if cfg!(debug_assertions) { "yes" } else { "no" }
    );
    println!("║  Log Level:  {:30} ║", "info");
    println!("║  Providers:  {:30} ║", config.models.providers.len());
    println!("║  Skills Dir: {:30} ║", config.agent.skills_dir);
    println!("╚══════════════════════════════════════════╝");

    Ok(())
}

/// Build and register all available tools.
fn build_tool_registry() -> odin_tools::ToolRegistry {
    let registry = odin_tools::ToolRegistry::new();
    let sandbox = Arc::new(odin_tools::Sandbox::default());

    macro_rules! try_reg {
        ($reg:expr, $tool:expr) => {
            if let Err(e) = $reg.register($tool) {
                tracing::warn!("[odin-cli] Failed to register tool: {e}");
            }
        };
    }

    try_reg!(
        registry,
        Box::new(odin_tools::builtins::file::FileRead::new(sandbox.clone()))
    );
    try_reg!(
        registry,
        Box::new(odin_tools::builtins::file::FileWrite::new(sandbox.clone()))
    );
    try_reg!(
        registry,
        Box::new(odin_tools::builtins::shell::Shell::new())
    );
    try_reg!(registry, Box::new(odin_tools::builtins::git::Git::new()));
    try_reg!(
        registry,
        Box::new(odin_tools::builtins::web::WebFetch::new())
    );
    try_reg!(
        registry,
        Box::new(odin_tools::builtins::web::WebSearch::new())
    );
    try_reg!(
        registry,
        Box::new(odin_tools::builtins::web::HttpRequest::new())
    );
    try_reg!(
        registry,
        Box::new(odin_tools::builtins::system::SystemInfo::new())
    );
    try_reg!(
        registry,
        Box::new(odin_tools::builtins::system::DiskUsage::new())
    );
    try_reg!(
        registry,
        Box::new(odin_tools::builtins::data::JsonExtract::new())
    );

    // Utility tools (10 new tools — Phase 4.0 expansion)
    try_reg!(registry, Box::new(odin_tools::builtins::utility::FileList));
    try_reg!(
        registry,
        Box::new(odin_tools::builtins::utility::FileDelete)
    );
    try_reg!(
        registry,
        Box::new(odin_tools::builtins::utility::FileExists)
    );
    try_reg!(registry, Box::new(odin_tools::builtins::utility::EnvVar));
    try_reg!(registry, Box::new(odin_tools::builtins::utility::TimeNow));
    try_reg!(
        registry,
        Box::new(odin_tools::builtins::utility::RandomNumber)
    );
    try_reg!(
        registry,
        Box::new(odin_tools::builtins::utility::JsonValidate)
    );
    try_reg!(
        registry,
        Box::new(odin_tools::builtins::utility::TextSearch)
    );
    try_reg!(
        registry,
        Box::new(odin_tools::builtins::utility::ProcessList)
    );
    try_reg!(
        registry,
        Box::new(odin_tools::builtins::utility::NetworkPing)
    );

    registry
}

/// Print a single catalog entry with its metadata.
fn print_tool_entry(tool: &odin_tools::CatalogEntry) {
    let icon = if tool.is_dangerous { "⚠" } else { " " };
    println!(
        "  {icon} {name:<30} {desc}",
        name = tool.name,
        desc = tool.description
    );
    println!("     Tags: {}", tool.tags.join(", "));
    if tool.requires_approval {
        println!("     [requires approval]");
    }
    println!();
}

// ── Helpers ──────────────────────────────────────────────────────────

/// Load configuration from an optional path.
fn load_config(path: Option<&std::path::Path>) -> anyhow::Result<OdinConfig> {
    match path {
        Some(p) if p.exists() => {
            OdinConfig::load(p).map_err(|e| anyhow::anyhow!("Config load error: {e}"))
        }
        Some(p) => {
            tracing::warn!("Config file not found: {}", p.display());
            Ok(OdinConfig::default())
        }
        None => {
            // Try default paths
            let default_paths = vec![
                shellexpand::tilde("~/.odin/config.yaml").to_string(),
                "odin.yaml".to_string(),
                "odin.yml".to_string(),
            ];

            for path_str in &default_paths {
                let path = std::path::Path::new(path_str);
                if path.exists() {
                    return OdinConfig::load(path)
                        .map_err(|e| anyhow::anyhow!("Config load error: {e}"));
                }
            }

            Ok(OdinConfig::default())
        }
    }
}

/// Get the Rust compiler version.
fn rustc_version() -> String {
    option_env!("CARGO_PKG_RUST_VERSION")
        .unwrap_or("stable")
        .to_string()
}

/// Get the build profile name.
fn build_profile() -> &'static str {
    if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::Cli;
    use super::Commands;
    use clap::Parser;

    #[test]
    fn test_cli_parses_run_command() {
        let _cli = Cli::parse_from(["odin", "run", "write a test"]);
    }

    #[test]
    fn test_cli_parses_serve_command() {
        let _cli = Cli::parse_from(["odin", "serve"]);
    }

    #[test]
    fn test_cli_parses_serve_with_addr() {
        let _cli = Cli::parse_from(["odin", "serve", "--addr", "0.0.0.0:8080"]);
    }

    #[test]
    fn test_cli_parses_config() {
        let _cli = Cli::parse_from(["odin", "config", "/tmp/test.yaml"]);
    }

    #[test]
    fn test_cli_parses_version() {
        let _cli = Cli::parse_from(["odin", "version"]);
    }

    #[test]
    fn test_verify_cli() {
        let _cmd = <Cli as clap::CommandFactory>::command();
    }

    #[test]
    fn test_cli_parses_schedule_add() {
        let cli = Cli::parse_from([
            "odin",
            "schedule",
            "add",
            "my-job",
            "0 */6 * * *",
            "run tests",
        ]);
        assert!(matches!(cli.command, Commands::Schedule { .. }));
    }

    #[test]
    fn test_cli_parses_schedule_list() {
        let cli = Cli::parse_from(["odin", "schedule", "list"]);
        assert!(matches!(cli.command, Commands::Schedule { .. }));
    }

    #[test]
    fn test_cli_parses_schedule_remove() {
        let cli = Cli::parse_from([
            "odin",
            "schedule",
            "remove",
            "550e8400-e29b-41d4-a716-446655440000",
        ]);
        assert!(matches!(cli.command, Commands::Schedule { .. }));
    }

    #[test]
    fn test_cli_parses_schedule_enable() {
        let cli = Cli::parse_from([
            "odin",
            "schedule",
            "enable",
            "550e8400-e29b-41d4-a716-446655440000",
        ]);
        assert!(matches!(cli.command, Commands::Schedule { .. }));
    }

    #[test]
    fn test_cli_parses_schedule_disable() {
        let cli = Cli::parse_from([
            "odin",
            "schedule",
            "disable",
            "550e8400-e29b-41d4-a716-446655440000",
        ]);
        assert!(matches!(cli.command, Commands::Schedule { .. }));
    }

    #[test]
    fn test_cli_parses_providers_list() {
        let cli = Cli::parse_from(["odin", "providers", "list"]);
        assert!(matches!(cli.command, Commands::Providers { .. }));
    }

    #[test]
    fn test_cli_parses_skills_list() {
        let cli = Cli::parse_from(["odin", "skills", "list"]);
        assert!(matches!(cli.command, Commands::Skills { .. }));
    }

    #[test]
    fn test_cli_parses_tasks_list() {
        let cli = Cli::parse_from(["odin", "tasks", "list"]);
        assert!(matches!(cli.command, Commands::Tasks { .. }));
    }

    #[test]
    fn test_cli_parses_tasks_inspect() {
        let cli = Cli::parse_from([
            "odin",
            "tasks",
            "inspect",
            "550e8400-e29b-41d4-a716-446655440000",
        ]);
        assert!(matches!(cli.command, Commands::Tasks { .. }));
    }

    #[test]
    fn test_cli_parses_sessions_list() {
        let cli = Cli::parse_from(["odin", "sessions", "list"]);
        assert!(matches!(cli.command, Commands::Sessions { .. }));
    }

    #[test]
    fn test_cli_parses_sessions_inspect() {
        let cli = Cli::parse_from([
            "odin",
            "sessions",
            "inspect",
            "550e8400-e29b-41d4-a716-446655440000",
        ]);
        assert!(matches!(cli.command, Commands::Sessions { .. }));
    }

    #[test]
    fn test_cli_parses_tools_list() {
        let cli = Cli::parse_from(["odin", "tools", "list"]);
        assert!(matches!(cli.command, Commands::Tools { .. }));
    }

    #[test]
    fn test_cli_parses_tools_inspect() {
        let cli = Cli::parse_from(["odin", "tools", "inspect", "file_read"]);
        assert!(matches!(cli.command, Commands::Tools { .. }));
    }

    #[test]
    fn test_cli_parses_tools_validate() {
        let cli = Cli::parse_from(["odin", "tools", "validate"]);
        assert!(matches!(cli.command, Commands::Tools { .. }));
    }

    #[test]
    fn test_cli_parses_tools_test() {
        let cli = Cli::parse_from(["odin", "tools", "test", "file_read"]);
        assert!(matches!(cli.command, Commands::Tools { .. }));
    }

    #[test]
    fn test_cli_parses_tools_doctor() {
        let cli = Cli::parse_from(["odin", "tools", "doctor"]);
        assert!(matches!(cli.command, Commands::Tools { .. }));
    }

    #[test]
    fn test_cli_parses_tools_catalog() {
        let cli = Cli::parse_from(["odin", "tools", "catalog"]);
        assert!(matches!(cli.command, Commands::Tools { .. }));
    }

    #[test]
    fn test_cli_parses_audit_replay() {
        let cli = Cli::parse_from([
            "odin",
            "audit",
            "replay",
            "550e8400-e29b-41d4-a716-446655440000",
        ]);
        assert!(matches!(cli.command, Commands::Audit { .. }));
    }

    #[test]
    fn test_cli_parses_status() {
        let cli = Cli::parse_from(["odin", "status"]);
        assert!(matches!(cli.command, Commands::Status));
    }
}
