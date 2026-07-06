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
use odin_runtime::Runtime;
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
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info")),
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
    let _config = load_config(config_path.as_deref())?;
    tracing::debug!("[CLI] Config loaded");

    // Create runtime
    let runtime = Runtime::new().with_default_max_iterations(max_iterations);

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

    // Since no real agents/providers are registered, this is a stub that
    // would be wired up with actual providers in production.
    tracing::warn!(
        "[CLI] No agents registered — task would be dispatched to a configured agent"
    );

    // Print the task info
    println!("Task ID: {}", agent_task.id);
    println!("Goal: {}", agent_task.goal);
    println!("Max iterations: {}", agent_task.max_iterations);
    println!();
    println!("Note: This is a scaffold. Wire up a real agent with odin-providers");
    println!("      and odin-tools to execute tasks. See crates/odin-runtime/README.md");
    println!();
    println!("Runtime state:");
    let summary = runtime.summary();
    println!("  Sessions: {}", summary.sessions);
    println!("  Agents: {}", summary.agents);
    println!("  Sub-agents: {}", summary.sub_agents);

    Ok(())
}

/// `odin serve` — Start the HTTP API server.
async fn cmd_serve(addr: String, config_path: Option<PathBuf>) -> anyhow::Result<()> {
    tracing::info!("[CLI] Starting HTTP server on {addr}");

    let _config = load_config(config_path.as_deref())?;

    println!("╔══════════════════════════════════════════╗");
    println!("║     Odin Gateway — Raven AI Harness     ║");
    println!("╠══════════════════════════════════════════╣");
    println!("║  HTTP API: http://{addr:<15}  ║", addr = addr);
    println!("║  Health:   http://{addr:<15}/health  ║", addr = addr);
    println!("║  Chat:     POST http://{addr:<15}/chat  ║", addr = addr);
    println!("╚══════════════════════════════════════════╝");
    println!();

    tracing::info!("[CLI] No task handler configured — /chat will return 503");

    // Start the HTTP server without a task handler (would be injected in production)
    odin_gateway::run_http_server(&addr, None)
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
            .map_err(|e| {
                anyhow::anyhow!("Failed to open editor '{}': {e}", editor)
            })?;

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
            println!(
                "No config file found at {}",
                config_path.display()
            );
            println!("Creating with defaults...");

            let config = OdinConfig::default();
            if let Some(parent) = config_path.parent() {
                std::fs::create_dir_all(parent).ok();
            }
            config.save(&config_path).map_err(|e| {
                anyhow::anyhow!("Failed to save config: {e}")
            })?;

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
    println!("  odin-core:    {}", odin_core::OdinConfig::default().general.instance_name);
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
    use clap::CommandFactory;

    #[test]
    fn test_cli_parses_run_command() {
        use super::Cli;
        let cli = Cli::try_parse_from(["odin", "run", "write a test"]);
        assert!(cli.is_ok());
    }

    #[test]
    fn test_cli_parses_serve_command() {
        use super::Cli;
        let cli = Cli::try_parse_from(["odin", "serve"]);
        assert!(cli.is_ok());
    }

    #[test]
    fn test_cli_parses_serve_with_addr() {
        use super::Cli;
        let cli = Cli::try_parse_from(["odin", "serve", "--addr", "0.0.0.0:8080"]);
        assert!(cli.is_ok());
    }

    #[test]
    fn test_cli_parses_config_show() {
        use super::Cli;
        let cli = Cli::try_parse_from(["odin", "config", "--show", "/tmp/test.yaml"]);
        assert!(cli.is_ok());
        let cli = Cli::try_parse_from(["odin", "config", "/tmp/test.yaml"]);
        assert!(cli.is_ok());
    }

    #[test]
    fn test_cli_parses_version() {
        use super::Cli;
        let cli = Cli::try_parse_from(["odin", "version"]);
        assert!(cli.is_ok());
    }

    #[test]
    fn test_cli_command_enum() {
        use super::Commands;
        // Just verify the enum compiles
        let _cmd = Commands::Version;
    }

    #[test]
    fn test_verify_cli() {
        use super::Cli;
        Cli::command().debug_assert();
    }
}
