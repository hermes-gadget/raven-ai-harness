//! Per-tool reliability scoring for the Odin harness.
//!
//! Tracks success/failure rates per tool, computes a reliability score (0.0–1.0),
//! and exposes query methods for tool selection guidance.
//!
//! ## Design
//!
//! - **Sliding window**: Only the last N calls per tool are considered (default: 100).
//! - **Decay**: Older results contribute less (exponential decay with configurable half-life).
//! - **Cold start**: New tools without data get a neutral score of 0.5.
//! - **Thread-safe**: Designed for concurrent access from the agent loop.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for the reliability tracker.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReliabilityConfig {
    /// Maximum number of recent calls to track per tool.
    pub window_size: usize,
    /// Half-life for exponential decay (older results count less).
    pub half_life: Duration,
    /// Default score for tools with no data.
    pub default_score: f64,
    /// Minimum score below which a tool is considered unreliable.
    pub alert_threshold: f64,
}

impl Default for ReliabilityConfig {
    fn default() -> Self {
        Self {
            window_size: 100,
            half_life: Duration::from_secs(3600), // 1 hour
            default_score: 0.5,
            alert_threshold: 0.7,
        }
    }
}

// ---------------------------------------------------------------------------
// Call record
// ---------------------------------------------------------------------------

/// A single tool call outcome.
#[derive(Debug, Clone)]
pub struct CallRecord {
    /// Whether the call succeeded.
    pub success: bool,
    /// Duration of the call in milliseconds.
    pub duration_ms: u64,
    /// When the call completed (for decay calculation).
    pub timestamp: Instant,
    /// Error message if the call failed.
    pub error: Option<String>,
}

impl CallRecord {
    pub fn success(duration_ms: u64) -> Self {
        Self {
            success: true,
            duration_ms,
            timestamp: Instant::now(),
            error: None,
        }
    }

    pub fn failure(duration_ms: u64, error: impl Into<String>) -> Self {
        Self {
            success: false,
            duration_ms,
            timestamp: Instant::now(),
            error: Some(error.into()),
        }
    }

    /// Age of this record (how long ago it was recorded).
    fn age(&self) -> Duration {
        self.timestamp.elapsed()
    }

    /// Decay weight based on age relative to half-life.
    fn weight(&self, half_life: Duration) -> f64 {
        let age_secs = self.age().as_secs_f64();
        let hl_secs = half_life.as_secs_f64();
        if hl_secs <= 0.0 {
            return 1.0;
        }
        // Exponential decay: weight = 2^(-age / half_life)
        2.0_f64.powf(-age_secs / hl_secs)
    }
}

// ---------------------------------------------------------------------------
// Tool reliability
// ---------------------------------------------------------------------------

/// Reliability information for a single tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolReliability {
    /// Tool name.
    pub tool_name: String,
    /// Reliability score (0.0–1.0).
    pub score: f64,
    /// Total calls recorded.
    pub total_calls: usize,
    /// Successful calls.
    pub success_count: usize,
    /// Failure count.
    pub failure_count: usize,
    /// Success rate (unweighted).
    pub success_rate: f64,
    /// Average duration in ms.
    pub avg_duration_ms: f64,
    /// Whether the tool is below the alert threshold.
    pub is_unreliable: bool,
    /// How many calls until the window is full (0 if full).
    pub calls_until_mature: usize,
}

// ---------------------------------------------------------------------------
// Reliability tracker
// ---------------------------------------------------------------------------

/// Thread-safe reliability tracker for all registered tools.
pub struct ReliabilityTracker {
    config: ReliabilityConfig,
    /// Per-tool records: tool_name → Vec<CallRecord>
    records: RwLock<HashMap<String, Vec<CallRecord>>>,
}

impl ReliabilityTracker {
    /// Create a new tracker with the given configuration.
    pub fn new(config: ReliabilityConfig) -> Self {
        Self {
            config,
            records: RwLock::new(HashMap::new()),
        }
    }

    /// Create a tracker with default configuration.
    pub fn default() -> Self {
        Self::new(ReliabilityConfig::default())
    }

    /// Record a successful tool call.
    pub fn record_success(&self, tool_name: &str, duration_ms: u64) {
        self.record(tool_name, CallRecord::success(duration_ms));
    }

    /// Record a failed tool call.
    pub fn record_failure(&self, tool_name: &str, duration_ms: u64, error: impl Into<String>) {
        self.record(tool_name, CallRecord::failure(duration_ms, error));
    }

    /// Record a call outcome.
    pub fn record(&self, tool_name: &str, record: CallRecord) {
        let mut records = self.records.write().expect("reliability tracker lock poisoned");
        let entry = records.entry(tool_name.to_string()).or_default();
        entry.push(record);

        // Trim to window size
        if entry.len() > self.config.window_size {
            let excess = entry.len() - self.config.window_size;
            entry.drain(0..excess);
        }
    }

    /// Get the reliability score for a tool.
    ///
    /// Returns 0.0–1.0, where 1.0 means perfect reliability in recent history.
    /// Tools with no data get `config.default_score`.
    pub fn score(&self, tool_name: &str) -> f64 {
        let records = self.records.read().expect("reliability tracker lock poisoned");
        let entry = match records.get(tool_name) {
            Some(e) if !e.is_empty() => e,
            _ => return self.config.default_score,
        };

        self.compute_score(entry)
    }

    /// Get detailed reliability info for a tool.
    pub fn get(&self, tool_name: &str) -> ToolReliability {
        let records = self.records.read().expect("reliability tracker lock poisoned");
        let entry = records.get(tool_name);

        let (total, successes, failures, avg_ms, score, mature) = match entry {
            Some(e) if !e.is_empty() => {
                let total = e.len();
                let successes = e.iter().filter(|r| r.success).count();
                let failures = total - successes;
                let avg_ms = if total > 0 {
                    e.iter().map(|r| r.duration_ms as f64).sum::<f64>() / total as f64
                } else {
                    0.0
                };
                let score = self.compute_score(e);
                let mature = total >= self.config.window_size;
                (total, successes, failures, avg_ms, score, mature)
            }
            _ => (0, 0, 0, 0.0, self.config.default_score, false),
        };

        let success_rate = if total > 0 {
            successes as f64 / total as f64
        } else {
            0.0
        };

        ToolReliability {
            tool_name: tool_name.to_string(),
            score,
            total_calls: total,
            success_count: successes,
            failure_count: failures,
            success_rate,
            avg_duration_ms: avg_ms,
            is_unreliable: score < self.config.alert_threshold,
            calls_until_mature: if mature {
                0
            } else {
                self.config.window_size.saturating_sub(total)
            },
        }
    }

    /// Get reliability info for all tracked tools.
    pub fn all(&self) -> Vec<ToolReliability> {
        let records = self.records.read().expect("reliability tracker lock poisoned");
        let mut results: Vec<ToolReliability> = records
            .keys()
            .map(|name| self.get(name))
            .collect();
        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        results
    }

    /// List tools below the alert threshold.
    pub fn unreliable(&self) -> Vec<ToolReliability> {
        self.all()
            .into_iter()
            .filter(|r| r.is_unreliable)
            .collect()
    }

    /// Reset tracking data for a tool.
    pub fn reset(&self, tool_name: &str) {
        let mut records = self.records.write().expect("reliability tracker lock poisoned");
        records.remove(tool_name);
    }

    /// Reset all tracking data.
    pub fn reset_all(&self) {
        let mut records = self.records.write().expect("reliability tracker lock poisoned");
        records.clear();
    }

    /// Number of tools being tracked.
    pub fn tool_count(&self) -> usize {
        self.records.read().expect("reliability tracker lock poisoned").len()
    }

    // -- internals ----------------------------------------------------------

    /// Compute a weighted reliability score from call records.
    fn compute_score(&self, records: &[CallRecord]) -> f64 {
        if records.is_empty() {
            return self.config.default_score;
        }

        let mut weighted_sum = 0.0_f64;
        let mut total_weight = 0.0_f64;

        for record in records {
            let w = record.weight(self.config.half_life);
            weighted_sum += if record.success { w } else { 0.0 };
            total_weight += w;
        }

        if total_weight <= 0.0 {
            return self.config.default_score;
        }

        weighted_sum / total_weight
    }
}

impl Default for ReliabilityTracker {
    fn default() -> Self {
        Self::default()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_default_score_for_unknown_tool() {
        let tracker = ReliabilityTracker::default();
        assert!((tracker.score("nonexistent") - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_perfect_score_after_successes() {
        let tracker = ReliabilityTracker::default();
        for _ in 0..10 {
            tracker.record_success("perfect_tool", 50);
        }
        let score = tracker.score("perfect_tool");
        assert!(score > 0.95, "expected >0.95, got {score}");
    }

    #[test]
    fn test_low_score_after_failures() {
        let tracker = ReliabilityTracker::default();
        for _ in 0..10 {
            tracker.record_failure("bad_tool", 100, "timeout");
        }
        let score = tracker.score("bad_tool");
        assert!(score < 0.1, "expected <0.1, got {score}");
    }

    #[test]
    fn test_mixed_results() {
        let tracker = ReliabilityTracker::default();
        for _ in 0..5 {
            tracker.record_success("mixed", 50);
        }
        for _ in 0..5 {
            tracker.record_failure("mixed", 50, "error");
        }
        let score = tracker.score("mixed");
        assert!(score > 0.4 && score < 0.6, "expected ~0.5, got {score}");
    }

    #[test]
    fn test_window_trimming() {
        let mut config = ReliabilityConfig::default();
        config.window_size = 5;
        let tracker = ReliabilityTracker::new(config);

        // First 5: all failures
        for i in 0..5 {
            tracker.record_failure("tool", 50, format!("fail {i}"));
        }
        assert!(tracker.score("tool") < 0.1);

        // Next 5: all successes — failures should be pushed out
        for _ in 0..5 {
            tracker.record_success("tool", 50);
        }
        assert!(tracker.score("tool") > 0.9);
    }

    #[test]
    fn test_decay_prefers_recent() {
        let mut config = ReliabilityConfig::default();
        config.half_life = Duration::from_millis(10);
        let tracker = ReliabilityTracker::new(config);

        // Failure far in the past
        tracker.record_failure("tool", 50, "old error");
        thread::sleep(Duration::from_millis(30));

        // Recent successes
        for _ in 0..5 {
            tracker.record_success("tool", 50);
        }

        let score = tracker.score("tool");
        assert!(score > 0.8, "recent successes should dominate, got {score}");
    }

    #[test]
    fn test_get_returns_full_info() {
        let tracker = ReliabilityTracker::default();
        tracker.record_success("tool", 50);
        tracker.record_success("tool", 60);
        tracker.record_failure("tool", 100, "timeout");

        let info = tracker.get("tool");
        assert_eq!(info.total_calls, 3);
        assert_eq!(info.success_count, 2);
        assert_eq!(info.failure_count, 1);
        assert!((info.avg_duration_ms - 70.0).abs() < 1.0);
        assert!((info.success_rate - 2.0 / 3.0).abs() < 0.01);
    }

    #[test]
    fn test_unreliable_detection() {
        let mut config = ReliabilityConfig::default();
        config.alert_threshold = 0.8;
        let tracker = ReliabilityTracker::new(config);

        for _ in 0..5 {
            tracker.record_success("good", 50);
        }
        for _ in 0..5 {
            tracker.record_failure("bad", 50, "err");
        }

        assert!(!tracker.get("good").is_unreliable);
        assert!(tracker.get("bad").is_unreliable);
        assert_eq!(tracker.unreliable().len(), 1);
    }

    #[test]
    fn test_all_sorted_by_score() {
        let tracker = ReliabilityTracker::default();
        for _ in 0..10 {
            tracker.record_failure("low", 50, "err");
        }
        for _ in 0..10 {
            tracker.record_success("high", 50);
        }

        let all = tracker.all();
        assert!(all[0].score >= all[1].score);
        assert_eq!(all[0].tool_name, "high");
    }

    #[test]
    fn test_reset() {
        let tracker = ReliabilityTracker::default();
        tracker.record_success("tool", 50);
        assert!(tracker.score("tool") > 0.9);

        tracker.reset("tool");
        assert!((tracker.score("tool") - 0.5).abs() < 0.001);
        assert_eq!(tracker.tool_count(), 0);
    }

    #[test]
    fn test_reset_all() {
        let tracker = ReliabilityTracker::default();
        tracker.record_success("a", 50);
        tracker.record_success("b", 50);
        assert!(tracker.tool_count() >= 1);

        tracker.reset_all();
        assert_eq!(tracker.tool_count(), 0);
        assert!((tracker.score("a") - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_calls_until_mature() {
        let mut config = ReliabilityConfig::default();
        config.window_size = 10;
        let tracker = ReliabilityTracker::new(config);

        tracker.record_success("young", 50);
        let info = tracker.get("young");
        assert_eq!(info.calls_until_mature, 9); // 10 - 1

        for _ in 0..9 {
            tracker.record_success("young", 50);
        }
        let info = tracker.get("young");
        assert_eq!(info.calls_until_mature, 0); // full
    }
}
