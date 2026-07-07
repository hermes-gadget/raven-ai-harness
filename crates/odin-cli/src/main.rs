//! Odin CLI — Command-line interface for the Raven AI harness.
//!
//! Usage:
//!   odin run <task>       — Execute a task
//!   odin serve [--addr]   — Start the HTTP API server
//!   odin config [--show]  — Show or edit configuration
//!   odin version          — Show version information

use std::path::PathBuf;
use std::sync::Arc;

use clap::{Parser, Subcommand};
use odin_core::config::OdinConfig;
use odin_core::types::AgentTask;
use odin_runtime::{Agent, Runtime};
use tracing_subscriber::EnvFilter;

// ── CLI Definition ───────────────────────────────────────────────────

/// Raven AI harness — the Odin agent system.
#[derive(Parser, Debug)]
#[command(
    name = "odin",
    version,
    about = "Raven AI harness — agent loop, skills, gateway",
    long_about = "Odin is the core runtime for the Raven AI agent harness.\n\
                  It provides a structured agent loop, skill management,\n\
                  and gateway interfaces for building AI-powered workflows.",
    author = "Raven Contributors"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Execute a task through the agent loop.
    Run {
        /// The task goal to execute.
        task: String,

        /// Optional config file path.
        #[arg(short, long, global = true, env = "ODIN_CONFIG")]
        config: Option<PathBuf>,

        /// Max iterations for the task.
        #[arg(short = 'n', long, default_value_t = 100)]
        max_iterations: u32,
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
        } => cmd_run(task, config, max_iterations).await,
        Commands::Serve { addr, config } => cmd_serve(addr, config).await,
        Commands::Config { path, edit } => cmd_config(path, edit),
        Commands::Version => cmd_version(),
    }
}

// ── Command Implementations ──────────────────────────────────────────

/// `odin run <task>` — Execute a task through the agent loop.
async fn cmd_run(
    task: String,
    config_path: Option<PathBuf>,
    max_iterations: u32,
) -> anyhow::Result<()> {
    tracing::info!("[CLI] Running task: {}", task);

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
            }
        });

    tracing::info!(
        "[CLI] Creating provider '{}' (type: {})",
        provider_name,
        provider_cfg.provider_type
    );

    // Create the provider via the factory
    let provider = odin_providers::create_provider(&provider_cfg)?;

    // Create the loop engine with the provider attached
    let engine = odin_loop::LoopEngine::new()
        .with_provider(provider.clone())
        .with_max_iterations(max_iterations);

    // Create a tool registry and register built-in tools
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

    // Get available tools as Vec<Arc<dyn Tool>>
    let tools: Vec<Arc<dyn odin_core::traits::Tool>> = tool_registry
        .list_schemas()
        .iter()
        .filter_map(|s| tool_registry.get(&s.function.name))
        .collect();

    // Create the agent
    let agent = Agent::new("default-agent", Arc::new(engine), provider, tools);
    let agent_id = agent.id;

    // Register agent in runtime
    let runtime = Runtime::new().with_default_max_iterations(max_iterations);
    runtime.register_agent(agent);
    let session = runtime.create_session_with_label("cli-run");

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

    // Submit the task
    tracing::info!("[CLI] Submitting task '{}' to agent 'default-agent'", task);
    let start = std::time::Instant::now();
    let result = runtime
        .submit_task(&agent_id, &agent_task, Some(session.id))
        .await?;
    let elapsed = start.elapsed();

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
        });

    let provider = odin_providers::create_provider(&provider_cfg)?;
    let tool_registry = Arc::new(odin_tools::ToolRegistry::new());
    let sandbox = Arc::new(odin_tools::Sandbox::new(
        odin_core::types::PathBoundary::default(),
    ));

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

    // Build the task handler closure
    let handler: odin_gateway::TaskHandlerFn = {
        Arc::new(move |req: odin_gateway::ChatRequest| {
            let provider = provider.clone();
            let tool_registry = tool_registry.clone();
            let tools = tools.clone();
            Box::pin(async move {
                let start = std::time::Instant::now();

                let engine = odin_loop::LoopEngine::new()
                    .with_provider(provider.clone())
                    .with_max_iterations(req.max_iterations.unwrap_or(100))
                    .with_tool_registry(tool_registry.clone());

                let agent = Agent::new(
                    "serve-agent",
                    Arc::new(engine),
                    provider.clone(),
                    tools.clone(),
                );
                let agent_id = agent.id;

                let runtime = Runtime::new();
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

                let result = runtime
                    .submit_task(&agent_id, &runtime_task, session_id)
                    .await
                    .map_err(|e| odin_core::error::OdinError::Internal(e.to_string()))?;

                let elapsed = start.elapsed().as_millis() as u64;

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
    println!("║     Odin Gateway — Raven AI Harness     ║");
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
    println!("Odin CLI (Raven AI Harness)");
    println!("Version:    {}", env!("CARGO_PKG_VERSION"));
    println!("Repository: {}", env!("CARGO_PKG_REPOSITORY"));
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
    // Detect build profile from debug assertions
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
        // Just verify the CommandFactory works
        let _cmd = <Cli as clap::CommandFactory>::command();
    }
}
