//! Odin Tools — Tool system for AI agents.
//!
//! This crate provides the tool abstraction layer: a `ToolRegistry` that
//! manages registered tools, a `Sandbox` for filesystem boundary enforcement,
//! a `ToolCatalog` for category-based tool organization, and a set of built-in
//! tools (file, shell, web, git) implementing the `Tool` trait from `odin_core`.

pub mod builtins;
pub mod catalog;
pub mod dry_run;
pub mod reliability;
pub mod sandbox;
pub mod tool;
pub mod validator;

pub use catalog::{CatalogEntry, ToolCatalog};
pub use dry_run::{DryRunConfig, DryRunTool};
pub use reliability::*;
pub use sandbox::Sandbox;
pub use tool::ToolRegistry;
pub use validator::*;
