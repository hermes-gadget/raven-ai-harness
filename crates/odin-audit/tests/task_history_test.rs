//! E2E test: task history via AuditLoggerImpl.
//!
//! Tests that:
//! 1. Task start/end entries can be logged and queried by session_id
//! 2. Multiple entries for the same session are returned in the correct
//!    order (most-recent-first, matching the logger's query implementation)
//! 3. Entries from different sessions are isolated

use odin_audit::logger::{AuditLoggerImpl, audit_entry};
use odin_core::traits::AuditLogger;
use odin_core::types::{AuditEventType, AuditResult};
use serde_json::json;
use uuid::Uuid;

/// Helper to log a task-related entry with the given event type.
async fn log_task_entry(
    logger: &AuditLoggerImpl,
    session_id: Uuid,
    agent_id: Uuid,
    event_type: AuditEventType,
    task_goal: &str,
    result: AuditResult,
) {
    let entry = audit_entry(
        agent_id,
        session_id,
        event_type,
        format!("task:{}", task_goal),
        json!({
            "task_goal": task_goal,
            "details": format!("Processing: {}", task_goal),
        }),
        result,
    );
    logger.log(entry).await.expect("log entry");
}

#[tokio::test]
async fn test_task_history_query_by_session_id() {
    let logger = AuditLoggerImpl::default();
    let agent_id = Uuid::new_v4();
    let session_id = Uuid::new_v4();

    // ── Log a sequence of task lifecycle events ──────────────────────
    log_task_entry(
        &logger,
        session_id,
        agent_id,
        AuditEventType::SessionStart,
        "start session",
        AuditResult::Success,
    )
    .await;

    log_task_entry(
        &logger,
        session_id,
        agent_id,
        AuditEventType::ToolCall,
        "write unit tests",
        AuditResult::Success,
    )
    .await;

    log_task_entry(
        &logger,
        session_id,
        agent_id,
        AuditEventType::ModelCall,
        "generate code",
        AuditResult::Success,
    )
    .await;

    log_task_entry(
        &logger,
        session_id,
        agent_id,
        AuditEventType::Decision,
        "review output",
        AuditResult::Success,
    )
    .await;

    log_task_entry(
        &logger,
        session_id,
        agent_id,
        AuditEventType::SessionEnd,
        "end session",
        AuditResult::Success,
    )
    .await;

    // ── Query by session_id ──────────────────────────────────────────
    let results = logger
        .query(Some(agent_id), Some(session_id), None, 10)
        .await
        .expect("query by agent + session");

    assert!(
        !results.is_empty(),
        "should find entries for the session"
    );
    assert_eq!(
        results.len(),
        5,
        "all 5 entries should be returned"
    );

    // All returned entries should belong to the queried session and agent
    for entry in &results {
        assert_eq!(
            entry.session_id, session_id,
            "entry session_id should match"
        );
        assert_eq!(
            entry.agent_id, agent_id,
            "entry agent_id should match"
        );
    }

    // ── Verify ordering (query returns most-recent-first due to .rev()) ──
    assert_eq!(
        results[0].event_type,
        AuditEventType::SessionEnd,
        "first result should be the most recent entry (SessionEnd)"
    );
    assert_eq!(
        results[4].event_type,
        AuditEventType::SessionStart,
        "last result should be the oldest entry (SessionStart)"
    );

    // Verify intermediate ordering
    assert_eq!(results[1].event_type, AuditEventType::Decision);
    assert_eq!(results[2].event_type, AuditEventType::ModelCall);
    assert_eq!(results[3].event_type, AuditEventType::ToolCall);
}

#[tokio::test]
async fn test_task_history_isolation_between_sessions() {
    let logger = AuditLoggerImpl::default();
    let agent_id = Uuid::new_v4();
    let session_a = Uuid::new_v4();
    let session_b = Uuid::new_v4();

    // Log entries for session A
    log_task_entry(
        &logger,
        session_a,
        agent_id,
        AuditEventType::SessionStart,
        "session A start",
        AuditResult::Success,
    )
    .await;

    log_task_entry(
        &logger,
        session_a,
        agent_id,
        AuditEventType::SessionEnd,
        "session A end",
        AuditResult::Success,
    )
    .await;

    // Log entries for session B
    log_task_entry(
        &logger,
        session_b,
        agent_id,
        AuditEventType::SessionStart,
        "session B start",
        AuditResult::Success,
    )
    .await;

    log_task_entry(
        &logger,
        session_b,
        agent_id,
        AuditEventType::ToolCall,
        "session B work",
        AuditResult::Success,
    )
    .await;

    log_task_entry(
        &logger,
        session_b,
        agent_id,
        AuditEventType::SessionEnd,
        "session B end",
        AuditResult::Success,
    )
    .await;

    // ── Query session A only ─────────────────────────────────────────
    let session_a_results = logger
        .query(Some(agent_id), Some(session_a), None, 10)
        .await
        .expect("query session A");

    assert_eq!(
        session_a_results.len(),
        2,
        "session A should have exactly 2 entries"
    );
    for entry in &session_a_results {
        assert_eq!(
            entry.session_id, session_a,
            "entry must belong to session A"
        );
    }
    // Verify event types
    assert_eq!(session_a_results[0].event_type, AuditEventType::SessionEnd);
    assert_eq!(session_a_results[1].event_type, AuditEventType::SessionStart);

    // ── Query session B only ─────────────────────────────────────────
    let session_b_results = logger
        .query(Some(agent_id), Some(session_b), None, 10)
        .await
        .expect("query session B");

    assert_eq!(
        session_b_results.len(),
        3,
        "session B should have exactly 3 entries"
    );
    for entry in &session_b_results {
        assert_eq!(
            entry.session_id, session_b,
            "entry must belong to session B"
        );
    }
}

#[tokio::test]
async fn test_task_history_empty_for_unknown_session() {
    let logger = AuditLoggerImpl::default();
    let agent_id = Uuid::new_v4();
    let known_session = Uuid::new_v4();
    let unknown_session = Uuid::new_v4();

    log_task_entry(
        &logger,
        known_session,
        agent_id,
        AuditEventType::SessionStart,
        "known session",
        AuditResult::Success,
    )
    .await;

    let results = logger
        .query(Some(agent_id), Some(unknown_session), None, 10)
        .await
        .expect("query unknown session");

    assert!(
        results.is_empty(),
        "should return no entries for an unknown session"
    );
}
