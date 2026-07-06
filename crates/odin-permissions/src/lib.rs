//! odin-permissions — Safety permissions and security for the Odin harness.
//!
//! Provides a complete permission system including:
//! - [`PolicyEngine`]: Evaluates tool calls and commands against allow/deny rules
//! - [`ApprovalGate`]: Interactive approval for dangerous operations
//! - [`SecretManager`]: Secure credential and secret handling

pub mod approval;
pub mod policy;
pub mod secrets;

pub use approval::{ApprovalGate, ApprovalRequest, ApprovalStatus};
pub use policy::PolicyEngine;
pub use secrets::{Secret, SecretManager};
