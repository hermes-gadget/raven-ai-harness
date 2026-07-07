//! Scheduler — cron-like job scheduling for the Odin harness.
//!
//! The [`Scheduler`] manages a collection of [`Job`]s, checks for
//! due jobs on a configurable tick interval, and spawns tasks via
//! Tokio.

use crate::job::{Job, JobId, JobTask, Schedule};
use chrono::Utc;
use odin_core::config::SchedulerConfig;
use odin_core::error::OdinResult;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{Duration, interval};
use tracing::{debug, info, trace, warn};
use uuid::Uuid;

/// A cron-like job scheduler.
///
/// Manages a set of [`Job`]s, periodically checks for due jobs,
/// and spawns their tasks asynchronously.
pub struct Scheduler {
    /// The scheduled jobs, keyed by ID.
    jobs: Arc<RwLock<HashMap<JobId, Job>>>,
    /// Configuration for the scheduler.
    config: SchedulerConfig,
    /// Whether the scheduler loop is running.
    running: Arc<RwLock<bool>>,
    /// Handle to the spawned tick loop.
    tick_handle: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
}

impl Scheduler {
    /// Create a new scheduler with the given configuration.
    pub fn new(config: SchedulerConfig) -> Self {
        Self {
            jobs: Arc::new(RwLock::new(HashMap::new())),
            config,
            running: Arc::new(RwLock::new(false)),
            tick_handle: Arc::new(RwLock::new(None)),
        }
    }

    /// Create a new scheduler with default configuration.
    #[allow(clippy::should_implement_trait)]
    pub fn default() -> Self {
        Self::new(SchedulerConfig::default())
    }

    /// Add a job to the scheduler.
    ///
    /// Returns the assigned job ID.
    pub async fn add_job(&self, name: &str, schedule: &str, task: JobTask) -> OdinResult<JobId> {
        let schedule = Schedule::parse(schedule).map_err(|e| {
            odin_core::error::OdinError::Config(format!("Invalid schedule '{}': {}", schedule, e))
        })?;

        let job = Job::new(name, schedule.clone(), task);
        let id = job.id;

        self.jobs.write().await.insert(id, job);
        info!(job_id = %id, name = %name, schedule = %schedule.expression, "Job added to scheduler");
        Ok(id)
    }

    /// Remove a job from the scheduler by ID.
    pub async fn remove_job(&self, job_id: JobId) -> OdinResult<bool> {
        let existed = self.jobs.write().await.remove(&job_id).is_some();
        if existed {
            info!(job_id = %job_id, "Job removed from scheduler");
        } else {
            warn!(job_id = %job_id, "Job not found for removal");
        }
        Ok(existed)
    }

    /// List all jobs currently registered.
    pub async fn list_jobs(&self) -> Vec<Job> {
        self.jobs.read().await.values().cloned().collect()
    }

    /// Get a specific job by ID.
    pub async fn get_job(&self, job_id: JobId) -> Option<Job> {
        self.jobs.read().await.get(&job_id).cloned()
    }

    /// Enable or disable a job.
    pub async fn set_job_enabled(&self, job_id: JobId, enabled: bool) -> OdinResult<bool> {
        let mut jobs = self.jobs.write().await;
        if let Some(job) = jobs.get_mut(&job_id) {
            job.enabled = enabled;
            info!(job_id = %job_id, enabled = enabled, "Job enabled state changed");
            Ok(true)
        } else {
            warn!(job_id = %job_id, "Job not found for enable/disable");
            Ok(false)
        }
    }

    /// Run all pending jobs synchronously (for manual tick).
    ///
    /// Returns the number of jobs that were run.
    pub async fn run_pending(&self) -> OdinResult<usize> {
        let now = Utc::now();
        let due_jobs: Vec<(JobId, Job)> = {
            let jobs = self.jobs.read().await;
            jobs.iter()
                .filter(|(_, job)| job.is_due(&now))
                .map(|(id, job)| (*id, job.clone()))
                .collect()
        };

        let count = due_jobs.len();
        for (job_id, job) in due_jobs {
            // Check concurrency limit
            if job.max_concurrent > 0 && job.running_count >= job.max_concurrent {
                trace!(
                    job_id = %job_id,
                    name = %job.name,
                    running = job.running_count,
                    max = job.max_concurrent,
                    "Skipping job due to concurrency limit"
                );
                continue;
            }

            let task_id = Uuid::new_v4();
            info!(
                job_id = %job_id,
                name = %job.name,
                task_id = %task_id,
                "Running scheduled job"
            );

            // Update job state
            {
                let mut jobs = self.jobs.write().await;
                if let Some(active) = jobs.get_mut(&job_id) {
                    active.mark_run(task_id);
                    active.running_count += 1;
                }
            }

            // Spawn the task
            let jobs_clone = self.jobs.clone();
            let task_fn = job.task.clone();
            tokio::spawn(async move {
                debug!(task_id = %task_id, "Job task started");
                let start = std::time::Instant::now();
                (task_fn)().await;
                let duration = start.elapsed();
                debug!(task_id = %task_id, duration_ms = duration.as_millis() as u64, "Job task completed");

                // Decrement running count
                let mut jobs = jobs_clone.write().await;
                if let Some(active) = jobs.get_mut(&job_id) {
                    active.running_count = active.running_count.saturating_sub(1);
                }
            });
        }

        Ok(count)
    }

    /// Start the scheduler loop that ticks on the configured interval.
    pub async fn start(&self) -> OdinResult<()> {
        let mut running = self.running.write().await;
        if *running {
            warn!("Scheduler is already running");
            return Ok(());
        }
        *running = true;
        drop(running);

        let interval_secs = self.config.check_interval_secs;
        let jobs = self.jobs.clone();
        let running_flag = self.running.clone();

        info!(
            check_interval_secs = interval_secs,
            "Scheduler loop starting"
        );

        let handle = tokio::spawn(async move {
            let mut ticker = interval(Duration::from_secs(interval_secs));
            ticker.tick().await; // Skip the first immediate tick

            loop {
                ticker.tick().await;

                let is_running = *running_flag.read().await;
                if !is_running {
                    info!("Scheduler loop stopped");
                    break;
                }

                let now = Utc::now();
                let due_ids: Vec<JobId> = {
                    let jobs_guard = jobs.read().await;
                    jobs_guard
                        .iter()
                        .filter(|(_, job)| job.is_due(&now))
                        .map(|(id, _)| *id)
                        .collect()
                };

                for job_id in due_ids {
                    // Read job details under read lock
                    let (task_fn, name, _max_concurrent, _running_count) = {
                        let jobs_guard = jobs.read().await;
                        let job = match jobs_guard.get(&job_id) {
                            Some(j) => j,
                            None => continue,
                        };
                        if job.max_concurrent > 0 && job.running_count >= job.max_concurrent {
                            trace!(
                                job_id = %job_id,
                                name = %job.name,
                                "Skipping job due to concurrency limit (loop)"
                            );
                            continue;
                        }
                        (
                            job.task.clone(),
                            job.name.clone(),
                            job.max_concurrent,
                            job.running_count,
                        )
                    };

                    let task_id = Uuid::new_v4();
                    info!(
                        job_id = %job_id,
                        name = %name,
                        task_id = %task_id,
                        "Running scheduled job (loop)"
                    );

                    // Update job state
                    {
                        let mut jobs_guard = jobs.write().await;
                        if let Some(active) = jobs_guard.get_mut(&job_id) {
                            active.mark_run(task_id);
                            active.running_count += 1;
                        }
                    }

                    // Spawn
                    let jobs_clone = jobs.clone();
                    tokio::spawn(async move {
                        debug!(task_id = %task_id, "Job task started (loop)");
                        let start = std::time::Instant::now();
                        (task_fn)().await;
                        let duration = start.elapsed();
                        debug!(
                            task_id = %task_id,
                            duration_ms = duration.as_millis() as u64,
                            "Job task completed (loop)"
                        );
                        let mut jobs_guard = jobs_clone.write().await;
                        if let Some(active) = jobs_guard.get_mut(&job_id) {
                            active.running_count = active.running_count.saturating_sub(1);
                        }
                    });
                }
            }
        });

        *self.tick_handle.write().await = Some(handle);
        Ok(())
    }

    /// Stop the scheduler loop.
    pub async fn stop(&self) -> OdinResult<()> {
        let mut running = self.running.write().await;
        *running = false;
        drop(running);

        if let Some(handle) = self.tick_handle.write().await.take() {
            info!("Scheduler stop requested, waiting for loop to finish");
            // Don't await forever — just detach
            drop(handle);
        }

        info!("Scheduler stopped");
        Ok(())
    }

    /// Check if the scheduler loop is running.
    pub async fn is_running(&self) -> bool {
        *self.running.read().await
    }

    /// Get the total number of registered jobs.
    pub async fn job_count(&self) -> usize {
        self.jobs.read().await.len()
    }

    /// Get configuration.
    pub fn config(&self) -> &SchedulerConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test]
    async fn test_add_and_list_jobs() {
        let sched = Scheduler::default();
        let task: JobTask = Arc::new(|| Box::pin(async {}));

        let id = sched.add_job("test", "* * * * *", task).await.unwrap();
        let jobs = sched.list_jobs().await;
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].id, id);
        assert_eq!(jobs[0].name, "test");
    }

    #[tokio::test]
    async fn test_remove_job() {
        let sched = Scheduler::default();
        let task: JobTask = Arc::new(|| Box::pin(async {}));

        let id = sched.add_job("test", "* * * * *", task).await.unwrap();
        assert!(sched.remove_job(id).await.unwrap());
        assert!(!sched.remove_job(id).await.unwrap()); // already gone
        assert_eq!(sched.list_jobs().await.len(), 0);
    }

    #[tokio::test]
    async fn test_run_pending() {
        let sched = Scheduler::default();
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();

        let task: JobTask = Arc::new(move || {
            let c = counter_clone.clone();
            Box::pin(async move {
                c.fetch_add(1, Ordering::SeqCst);
            })
        });

        // Use a schedule that matches every minute
        let id = sched.add_job("counter", "* * * * *", task).await.unwrap();

        // Manually set next_run to past to trigger immediate execution
        {
            let mut jobs = sched.jobs.write().await;
            if let Some(job) = jobs.get_mut(&id) {
                job.next_run = Some(Utc::now() - chrono::TimeDelta::minutes(1));
            }
        }

        let count = sched.run_pending().await.unwrap();
        assert_eq!(count, 1);

        // Give the spawned task time to run
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_enable_disable_job() {
        let sched = Scheduler::default();
        let task: JobTask = Arc::new(|| Box::pin(async {}));

        let id = sched.add_job("toggle", "* * * * *", task).await.unwrap();

        assert!(sched.set_job_enabled(id, false).await.unwrap());
        let job = sched.get_job(id).await.unwrap();
        assert!(!job.enabled);

        assert!(sched.set_job_enabled(id, true).await.unwrap());
        let job = sched.get_job(id).await.unwrap();
        assert!(job.enabled);
    }

    #[tokio::test]
    async fn test_invalid_schedule() {
        let sched = Scheduler::default();
        let task: JobTask = Arc::new(|| Box::pin(async {}));

        let result = sched.add_job("bad", "not-a-cron", task).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_start_stop() {
        let sched = Scheduler::default();
        assert!(!sched.is_running().await);

        sched.start().await.unwrap();
        assert!(sched.is_running().await);

        // Give it a moment to start ticking
        tokio::time::sleep(Duration::from_millis(50)).await;

        sched.stop().await.unwrap();
        assert!(!sched.is_running().await);
    }
}
