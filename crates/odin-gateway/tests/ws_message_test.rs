//! E2E test: WebSocket message flow via broadcast channel.
//!
//! Tests that:
//! 1. task_submit and task_complete messages are received in order
//! 2. Multiple receivers all get the same messages
//! 3. Messages survive serialization round-trip through the channel

use odin_gateway::ws::WsMessage;
use tokio::sync::broadcast;

#[tokio::test]
async fn test_ws_messages_received_in_order() {
    let capacity = 16;
    let (tx, mut rx) = broadcast::channel::<WsMessage>(capacity);

    // Create task_submit and task_complete messages
    let submit_msg = WsMessage {
        msg_type: "task_submit".into(),
        payload: Some(serde_json::json!({
            "goal": "Write unit tests for the scheduler",
            "max_iterations": 10,
        })),
        correlation_id: Some("req-001".into()),
    };

    let progress_msg = WsMessage::task_progress("task-abc", 3, 0.85, "ACT");

    let complete_msg = WsMessage::task_complete(
        "task-abc",
        true,
        "All tests pass",
        5,
        0.95,
        Some("req-001".into()),
    );

    // Send all three messages
    tx.send(submit_msg.clone()).expect("send submit");
    tx.send(progress_msg.clone()).expect("send progress");
    tx.send(complete_msg.clone()).expect("send complete");

    // Verify they arrive in order (broadcast is FIFO per receiver)
    let received1 = rx.recv().await.expect("receive first message");
    assert_eq!(
        received1.msg_type, "task_submit",
        "first message should be task_submit"
    );
    assert_eq!(
        received1
            .payload
            .as_ref()
            .and_then(|p| p.get("goal"))
            .and_then(|v| v.as_str()),
        Some("Write unit tests for the scheduler"),
        "task_submit payload should contain goal"
    );

    let received2 = rx.recv().await.expect("receive second message");
    assert_eq!(
        received2.msg_type, "task_progress",
        "second message should be task_progress"
    );
    assert_eq!(
        received2
            .payload
            .as_ref()
            .and_then(|p| p.get("iteration"))
            .and_then(|v| v.as_u64()),
        Some(3),
        "task_progress should have iteration 3"
    );

    let received3 = rx.recv().await.expect("receive third message");
    assert_eq!(
        received3.msg_type, "task_complete",
        "third message should be task_complete"
    );
    assert_eq!(
        received3
            .payload
            .as_ref()
            .and_then(|p| p.get("success"))
            .and_then(|v| v.as_bool()),
        Some(true),
        "task_complete should indicate success"
    );
    assert_eq!(
        received3.correlation_id.as_deref(),
        Some("req-001"),
        "task_complete should carry correlation_id"
    );
}

#[tokio::test]
async fn test_ws_broadcast_to_multiple_receivers() {
    let capacity = 16;
    let (tx, _rx) = broadcast::channel::<WsMessage>(capacity);

    // Subscribe two receivers
    let mut rx1 = tx.subscribe();
    let mut rx2 = tx.subscribe();

    let msg = WsMessage::task_started("t1", "Do something", None);

    tx.send(msg.clone()).expect("send to all");

    // Both receivers should get the message
    let received1 = rx1.recv().await.expect("rx1 receives");
    assert_eq!(received1.msg_type, "task_started");

    let received2 = rx2.recv().await.expect("rx2 receives");
    assert_eq!(received2.msg_type, "task_started");
}

#[tokio::test]
async fn test_ws_message_serde_round_trip() {
    let original = WsMessage {
        msg_type: "task_submit".into(),
        payload: Some(serde_json::json!({
            "goal": "Refactor the auth module",
            "max_iterations": 20,
        })),
        correlation_id: Some("corr-42".into()),
    };

    // Serialize and deserialize
    let json = serde_json::to_string(&original).expect("serialize");
    let deserialized: WsMessage = serde_json::from_str(&json).expect("deserialize");

    assert_eq!(deserialized.msg_type, "task_submit");
    assert_eq!(
        deserialized
            .payload
            .as_ref()
            .and_then(|p| p.get("goal"))
            .and_then(|v| v.as_str()),
        Some("Refactor the auth module")
    );
    assert_eq!(deserialized.correlation_id, Some("corr-42".into()));
}
