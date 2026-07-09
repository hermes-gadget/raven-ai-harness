//! Built-in tools for Raven Agent.
//!
//! This module provides the standard set of tools that every agent should
//! have access to: file operations, shell commands, web operations, and git.

pub mod data;
pub mod file;
pub mod git;
pub mod github;
pub mod shell;
pub mod system;
pub mod utility;
pub mod web;
