//! Odin Tools — Tool system for AI agents.
//!
//! This crate provides the tool abstraction layer: a `ToolRegistry` that
//! manages registered tools, a `Sandbox` for filesystem boundary enforcement,
//! and a set of built-in tools (file, shell, web, git) implementing the
//! `Tool` trait from `odin_core`.

pub mod builtins;
pub mod sandbox;
pub mod tool;

pub use sandbox::Sandbox;
pub use tool::ToolRegistry;
