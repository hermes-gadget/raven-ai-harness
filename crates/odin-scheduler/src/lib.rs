//! odin-scheduler — Cron-like job scheduling for the Odin harness.
//!
//! Provides a [`Scheduler`] that manages a collection of scheduled [`Job`]s,
//! parses cron-like expressions via [`Schedule`], and dispatches due jobs
//! to Tokio tasks.

pub mod job;
pub mod scheduler;

pub use job::{CronField, Job, JobId, JobTask, Schedule};
pub use scheduler::Scheduler;
