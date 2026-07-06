//! Built-in tools for the Odin harness.
//!
//! This module provides the standard set of tools that every agent should
//! have access to: file operations, shell commands, web operations, and git.

pub mod file;
pub mod git;
pub mod shell;
pub mod web;
