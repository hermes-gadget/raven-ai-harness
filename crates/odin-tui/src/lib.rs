//! Interactive Terminal UI for Raven Agent.
//! Clean layout: top bar, chat (65-70%), agents side (30-35%), bottom bar.

pub mod app;
pub mod events;
pub mod runner;
pub mod ui;

#[cfg(test)]
#[path = "tests.rs"]
mod tui_tests;

pub use app::{App, Panel, RunMode};
pub use events::{Action, Event, EventHandler};
pub use ui::run_ui;

use anyhow::Result;
use std::path::PathBuf;

pub async fn start(db_path: Option<PathBuf>) -> Result<()> {
    let db_path = db_path.unwrap_or_else(|| {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        PathBuf::from(home).join(".raven-agent/orchestration.db")
    });
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut app = App::new(db_path).await?;
    run_ui(&mut app).await?;
    Ok(())
}
