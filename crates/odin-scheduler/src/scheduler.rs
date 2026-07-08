//! Scheduler — cron-like job scheduling for the Odin harness.
//!
//! The [`Scheduler`] manages a collection of [`Job`]s, checks for
//! due jobs on a configurable tick interval, and spawns tasks via
//! Tokio. Supports optional persistence via [`SchedulerStore`] and
//! optional runtime-driven task execution via [`Runtime`].

use crate::job::{Job, JobId, JobTask, Schedule};
use crate::store::{PersistedJob, SchedulerJobConfig, SchedulerStore};
use chrono::Utc;
use odin_core::config::SchedulerConfig;
use odin_core::error::{OdinError, OdinResult};
use odin_runtime::Runtime;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{Duration, interval};
use tracing::{debug, info, trace, warn};
use uuid::Uuid;

/// A cron-like job scheduler with optional persistence and runtime execution.
///
/// Manages a set of [`Job`]s, periodically checks for due jobs,
/// and spawns their tasks asynchronously. When a [`SchedulerStore`]
/// is provided, all job mutations are persisted automatically.
///
/// When a [`Runtime`] is provided, jobs with a `task_goal` will
/// submit [`AgentTask`]s to the runtime instead of running generic
/// closures.
pub struct Scheduler {
    /// The scheduled jobs, keyed by ID.
    jobs: Arc<RwLock<HashMap<JobId, Job>>>,
    /// Configuration for the scheduler.
    config: SchedulerConfig,
    /// Whether the scheduler loop is running.
    running: Arc<RwLock<bool>>,
    /// Handle to the spawned tick loop.
    tick_handle: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
    /// Optional persistent store for job definitions and state.
    store: Option<Arc<dyn SchedulerStore>>,
    /// Optional runtime for submitting agent tasks.
    runtime: Option<Arc<Runtime>>,
}

impl Scheduler {
    /// Create a new scheduler with the given configuration.
    pub fn new(config: SchedulerConfig) -> Self {
        Self {
            jobs: Arc::new(RwLock::new(HashMap::new())),
            config,
            running: Arc::new(RwLock::new(false)),
            tick_handle: Arc::new(RwLock::new(None)),
            store: None,
            runtime: None,
        }
    }

    /// Create a new scheduler with default configuration.
    #[allow(clippy::should_implement_trait)]
    pub fn default() -> Self {
        Self::new(SchedulerConfig::default())
    }

    /// Attach a persistent [`SchedulerStore`].
    ///
    /// When set, all job mutations are persisted automatically and
    /// persisted jobs are loaded into memory on [`start`](Self::start).
    pub fn with_store(mut self, store: Arc<dyn SchedulerStore>) -> Self {
        self.store = Some(store);
        self
    }

    /// Attach a [`Runtime`] for submitting agent tasks.
    ///
    /// When set, jobs that have a `task_goal` will create `AgentTask`s
    /// and submit them via the runtime instead of running generic closures.
    pub fn with_runtime(mut self, runtime: Arc<Runtime>) -> Self {
        self.runtime = Some(runtime);
        self
    }

    /// Add a job to the scheduler using a generic closure task.
    ///
    /// Returns the assigned job ID. If a store is configured, the job
    /// is persisted immediately.
    pub async fn add_job(&self, name: &str, schedule: &str, task: JobTask) -> OdinResult<JobId> {
        let sched = Schedule::parse(schedule).map_err(|e| {
            OdinError::Config(format!("Invalid schedule '{}': {}", schedule, e))
        })?;

        let job = Job::new(name, sched.clone(), task);
        let id = job.id;

        // Persist to store if configured
        if let Some(ref store) = self.store {
            let persisted = PersistedJob::from_job(&job);
            store.save_job(&persisted).await?;
        }

        self.jobs.write().await.insert(id, job);
        info!(job_id = %id, name = %name, schedule = %sched.expression, "Job added to scheduler");
        Ok(id)
    }

    /// Add a job with a [`SchedulerJobConfig`] instead of a closure.
    ///
    /// The job will be executed via the runtime (if configured) or
    /// run a no-op task if no runtime is available.
    pub async fn add_job_with_config(
        &self,
        name: &str,
        schedule: &str,
        config: SchedulerJobConfig,
    ) -> OdinResult<JobId> {
        let sched = Schedule::parse(schedule).map_err(|e| {
            OdinError::Config(format!("Invalid schedule '{}': {}", schedule, e))
        })?;

        let mut job = Job::new(name, sched.clone(), crate::job::noop_task());
        job.task_goal = Some(config.task_goal);
        job.max_iterations = config.max_iterations;
        let id = job.id;

        // Persist to store if configured
        if let Some(ref store) = self.store {
            let persisted = PersistedJob::from_job(&job);
            store.save_job(&persisted).await?;
        }

        self.jobs.write().await.insert(id, job);
        info!(job_id = %id, name = %name, schedule = %sched.expression, "Job added to scheduler (with config)");
        Ok(id)
    }

    /// Remove a job from the scheduler by ID.
    pub async fn remove_job(&self, job_id: JobId) -> OdinResult<bool> {
        // Delete from store first
        if let Some(ref store) = self.store {
            store.delete_job(&job_id).await?;
        }

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

            // Persist to store
            if let Some(ref store) = self.store {
                let id = job.id;
                let run_count = job.run_count;
                let last_run = job.last_run;
                let next_run = job.next_run;
                // Drop lock before async call
                drop(jobs);
                store
                    .update_job_state(&id, enabled, last_run, next_run, run_count)
                    .await?;
                info!(job_id = %job_id, enabled = enabled, "Job enabled state changed");
                return Ok(true);
            }

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

            // Determine if this job should use the runtime or the closure
            let use_runtime = self.runtime.is_some() && job.task_goal.is_some();

            // Update job state in memory
            {
                let mut jobs = self.jobs.write().await;
                if let Some(active) = jobs.get_mut(&job_id) {
                    active.mark_run(task_id);
                    active.running_count += 1;
                }
            }

            // Persist updated state
            if let Some(ref store) = self.store {
                let jobs_guard = self.jobs.read().await;
                if let Some(active) = jobs_guard.get(&job_id) {
                    let _ = store
                        .update_job_state(
                            &job_id,
                            active.enabled,
                            active.last_run,
                            active.next_run,
                            active.run_count,
                        )
                        .await;
                }
                drop(jobs_guard);
            }

            if use_runtime {
                // Runtime-driven execution path
                let runtime = self.runtime.clone().unwrap();
                let task_goal = job.task_goal.clone().unwrap();
                let max_iterations = job.max_iterations;
                let jobs_clone = self.jobs.clone();

                tokio::spawn(async move {
                    debug!(task_id = %task_id, goal = %task_goal, "Job task starting via runtime");

                    // Create a minimal runtime task
                    let agent_task = odin_core::types::AgentTask {
                        id: task_id,
                        goal: task_goal,
                        context: None,
                        sub_tasks: vec![],
                        success_criteria: vec![],
                        max_iterations,
                        created_at: chrono::Utc::now(),
                    };

                    let _ = runtime
                        .submit_task(&task_id, &agent_task, None)
                        .await;

                    // Decrement running count
                    let mut jobs_guard = jobs_clone.write().await;
                    if let Some(active) = jobs_guard.get_mut(&job_id) {
                        active.running_count = active.running_count.saturating_sub(1);
                    }
                    drop(jobs_guard);

                    debug!(task_id = %task_id, "Job task completed (runtime)");
                });
            } else {
                // Closure-based execution path (original behaviour)
                let jobs_clone = self.jobs.clone();
                let task_fn = job.task.clone();

                tokio::spawn(async move {
                    debug!(task_id = %task_id, "Job task started");
                    let start = std::time::Instant::now();
                    (task_fn)().await;
                    let duration = start.elapsed();
                    debug!(task_id = %task_id, duration_ms = duration.as_millis() as u64, "Job task completed");

                    // Decrement running count
                    let mut jobs_guard = jobs_clone.write().await;
                    if let Some(active) = jobs_guard.get_mut(&job_id) {
                        active.running_count = active.running_count.saturating_sub(1);
                    }
                    drop(jobs_guard);
                });
            }
        }

        Ok(count)
    }

    /// Start the scheduler loop that ticks on the configured interval.
    ///
    /// If a [`SchedulerStore`] is configured, all persisted jobs are
    /// loaded into memory before the loop starts.
    pub async fn start(&self) -> OdinResult<()> {
        let mut running = self.running.write().await;
        if *running {
            warn!("Scheduler is already running");
            return Ok(());
        }
        *running = true;
        drop(running);

        // Load persisted jobs from store
        if let Some(ref store) = self.store {
            let persisted_jobs = store.load_all_jobs().await?;
            info!(
                count = persisted_jobs.len(),
                "Loading persisted jobs into scheduler"
            );
            let mut jobs = self.jobs.write().await;
            for pj in persisted_jobs {
                let mut job = pj.into_job();
                // If we have a runtime and the job has a task_goal, the
                // run_pending method will handle runtime dispatch separately.
                // The closure can stay as noop.
                if self.runtime.is_some() && job.task_goal.is_some() {
                    job.task = crate::job::noop_task();
                }
                jobs.insert(job.id, job);
            }
        }

        let interval_secs = self.config.check_interval_secs;
        let jobs = self.jobs.clone();
        let running_flag = self.running.clone();
        let store = self.store.clone();
        let runtime = self.runtime.clone();

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
                    let (task_fn, name, task_goal, _max_concurrent, _running_count) = {
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
                            job.task_goal.clone(),
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

                    // Update job state in memory
                    {
                        let mut jobs_guard = jobs.write().await;
                        if let Some(active) = jobs_guard.get_mut(&job_id) {
                            active.mark_run(task_id);
                            active.running_count += 1;
                        }
                    }

                    // Persist updated state
                    if let Some(ref store) = store {
                        let jobs_guard = jobs.read().await;
                        if let Some(active) = jobs_guard.get(&job_id) {
                            let _ = store
                                .update_job_state(
                                    &job_id,
                                    active.enabled,
                                    active.last_run,
                                    active.next_run,
                                    active.run_count,
                                )
                                .await;
                        }
                        drop(jobs_guard);
                    }

                    if task_goal.is_some() && runtime.is_some() {
                        // Runtime-driven execution path in the loop
                        let runtime = runtime.clone().unwrap();
                        let goal = task_goal.unwrap();
                        let max_iters = 100; // default
                        let jobs_clone = jobs.clone();
                        let _store = store.clone();
                        tokio::spawn(async move {
                            debug!(task_id = %task_id, goal = %goal, "Job task started (loop, runtime)");

                            let agent_task = odin_core::types::AgentTask {
                                id: task_id,
                                goal,
                                context: None,
                                sub_tasks: vec![],
                                success_criteria: vec![],
                                max_iterations: max_iters,
                                created_at: chrono::Utc::now(),
                            };

                            let _ = runtime
                                .submit_task(&task_id, &agent_task, None)
                                .await;

                            let mut jobs_guard = jobs_clone.write().await;
                            if let Some(active) = jobs_guard.get_mut(&job_id) {
                                active.running_count = active.running_count.saturating_sub(1);
                            }
                        });
                    } else {
                        // Standard closure-based execution
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
    use crate::store::SqliteSchedulerStore;
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

    // ── Persistence Integration Tests ───────────────────────────────

    #[tokio::test]
    async fn test_add_job_with_store_persists() {
        let store = Arc::new(SqliteSchedulerStore::in_memory().unwrap());
        let sched = Scheduler::default().with_store(store.clone());
        let task: JobTask = Arc::new(|| Box::pin(async {}));

        let id = sched.add_job("persist-test", "*/10 * * * *", task).await.unwrap();
        let loaded = store.load_all_jobs().await.unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].name, "persist-test");
        assert_eq!(loaded[0].id, id);
    }

    #[tokio::test]
    async fn test_add_job_with_config_persists() {
        let store = Arc::new(SqliteSchedulerStore::in_memory().unwrap());
        let sched = Scheduler::default().with_store(store.clone());

        let config = SchedulerJobConfig::new("Run my task").with_max_iterations(50);
        let _id = sched
            .add_job_with_config("config-test", "0 */6 * * *", config)
            .await
            .unwrap();

        let loaded = store.load_all_jobs().await.unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].name, "config-test");
        assert_eq!(
            loaded[0].task_goal.as_deref(),
            Some("Run my task")
        );
        assert_eq!(loaded[0].max_iterations, 50);
    }

    #[tokio::test]
    async fn test_remove_job_deletes_from_store() {
        let store = Arc::new(SqliteSchedulerStore::in_memory().unwrap());
        let sched = Scheduler::default().with_store(store.clone());
        let task: JobTask = Arc::new(|| Box::pin(async {}));

        let id = sched.add_job("delete-me", "0 9 * * *", task).await.unwrap();
        assert_eq!(store.load_all_jobs().await.unwrap().len(), 1);

        sched.remove_job(id).await.unwrap();
        assert_eq!(store.load_all_jobs().await.unwrap().len(), 0);
    }

    #[tokio::test]
    async fn test_set_job_enabled_updates_store() {
        let store = Arc::new(SqliteSchedulerStore::in_memory().unwrap());
        let sched = Scheduler::default().with_store(store.clone());
        let task: JobTask = Arc::new(|| Box::pin(async {}));

        let id = sched.add_job("toggle-me", "* * * * *", task).await.unwrap();

        // Disable
        sched.set_job_enabled(id, false).await.unwrap();
        let loaded = store.load_all_jobs().await.unwrap();
        assert_eq!(loaded.len(), 1);
        assert!(!loaded[0].enabled);
    }

    #[tokio::test]
    async fn test_start_loads_persisted_jobs() {
        let store = Arc::new(SqliteSchedulerStore::in_memory().unwrap());

        // Pre-populate the store directly
        let pj = PersistedJob {
            id: Uuid::new_v4(),
            name: "pre-loaded".to_string(),
            cron_expr: "0 * * * *".to_string(),
            task_goal: None,
            max_iterations: 100,
            enabled: true,
            last_run: None,
            next_run: Some(Utc::now() + chrono::TimeDelta::hours(1)),
            run_count: 0,
            created_at: Utc::now(),
        };
        store.save_job(&pj).await.unwrap();

        // Create scheduler with this store and start it
        let sched = Scheduler::default().with_store(store.clone());
        assert_eq!(sched.job_count().await, 0);

        sched.start().await.unwrap();

        // After start, the persisted job should be loaded
        let jobs = sched.list_jobs().await;
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].name, "pre-loaded");

        sched.stop().await.unwrap();
    }

    #[tokio::test]
    async fn test_scheduler_restart_persistence() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("test_scheduler_restart_{}.db", Uuid::new_v4()));
        let path_str = path.to_str().unwrap().to_string();

        // First scheduler: add job via store
        let id = {
            let store = Arc::new(SqliteSchedulerStore::new(&path_str).unwrap());
            let sched = Scheduler::default().with_store(store.clone());
            let task: JobTask = Arc::new(|| Box::pin(async {}));
            let id = sched.add_job("restart-test", "*/5 * * * *", task).await.unwrap();
            id
        };

        // Second scheduler (simulating restart): verify job is loaded
        {
            let store = Arc::new(SqliteSchedulerStore::new(&path_str).unwrap());
            let sched = Scheduler::default().with_store(store.clone());

            sched.start().await.unwrap();
            let jobs = sched.list_jobs().await;
            assert_eq!(jobs.len(), 1);
            assert_eq!(jobs[0].name, "restart-test");
            assert_eq!(jobs[0].id, id);

            sched.stop().await.unwrap();
        }

        // Cleanup
        let _ = std::fs::remove_file(&path_str);
    }

    #[tokio::test]
    async fn test_in_memory_path_still_works() {
        // Even with store configured, the in-memory-only path for other
        // operations should still work fine
        let store = Arc::new(SqliteSchedulerStore::in_memory().unwrap());
        let sched = Scheduler::default().with_store(store);
        let task: JobTask = Arc::new(|| Box::pin(async {}));

        let id = sched.add_job("mem-test", "0 0 * * *", task).await.unwrap();
        let job = sched.get_job(id).await.unwrap();
        assert_eq!(job.name, "mem-test");
        assert!(job.enabled);
    }

    #[tokio::test]
    async fn test_run_pending_with_store_updates_state() {
        let store = Arc::new(SqliteSchedulerStore::in_memory().unwrap());
        let sched = Scheduler::default().with_store(store.clone());
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();

        let task: JobTask = Arc::new(move || {
            let c = counter_clone.clone();
            Box::pin(async move { c.fetch_add(1, Ordering::SeqCst); })
        });

        let id = sched.add_job("state-update", "* * * * *", task).await.unwrap();

        // Set next_run to past
        {
            let mut jobs = sched.jobs.write().await;
            if let Some(job) = jobs.get_mut(&id) {
                job.next_run = Some(Utc::now() - chrono::TimeDelta::minutes(1));
            }
        }

        let count = sched.run_pending().await.unwrap();
        assert_eq!(count, 1);

        tokio::time::sleep(Duration::from_millis(100)).await;

        // Verify store was updated
        let loaded = store.load_all_jobs().await.unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].run_count, 1);
        assert!(loaded[0].last_run.is_some());
        assert!(loaded[0].next_run.is_some());
    }
}
