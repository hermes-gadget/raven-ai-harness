#[cfg(test)]
mod tests {
    use super::super::app::*;
    use super::super::events::*;
    use super::super::runner::{AgentStage, RunnerCommand, RunnerEvent};

    #[test]
    fn test_panel_names() {
        assert_eq!(Panel::Chat.name(), "Chat");
        assert_eq!(Panel::Agents.name(), "Agents");
        assert_eq!(Panel::TaskGraph.name(), "Task Graph");
        assert_eq!(Panel::Locks.name(), "Files/Locks");
        assert_eq!(Panel::Tools.name(), "Tools");
        assert_eq!(Panel::Logs.name(), "Logs/Audit");
        assert_eq!(Panel::History.name(), "History");
        assert_eq!(Panel::Conflicts.name(), "Conflicts");
    }

    #[test]
    fn test_panel_count() {
        assert_eq!(Panel::ALL.len(), 8);
    }

    #[test]
    fn test_panel_next() {
        assert_eq!(Panel::Chat.next(), Panel::Agents);
        assert_eq!(Panel::Agents.next(), Panel::TaskGraph);
        assert_eq!(Panel::Conflicts.next(), Panel::Chat);
    }

    #[test]
    fn test_panel_prev() {
        assert_eq!(Panel::Chat.prev(), Panel::Conflicts);
    }

    #[test]
    fn test_panel_numbers() {
        assert_eq!(Panel::Chat.number(), 1);
        assert_eq!(Panel::Agents.number(), 2);
        assert_eq!(Panel::Conflicts.number(), 8);
    }

    #[test]
    fn test_mode() {
        let (txt, _) = crate::ui::mode_str(RunMode::Idle);
        assert_eq!(txt, "IDLE");
        let (txt, _) = crate::ui::mode_str(RunMode::Running);
        assert_eq!(txt, "RUNNING");
    }

    #[test]
    fn test_phase_icon() {
        let (icon, _) = crate::ui::phase_icon("running");
        assert_eq!(icon, "[>]");
        let (icon, _) = crate::ui::phase_icon("failed");
        assert_eq!(icon, "[X]");
        let (icon, _) = crate::ui::phase_icon("done");
        assert_eq!(icon, "[v]");
    }

    #[test]
    fn test_message_roles() {
        let msg = ChatMessage {
            role: MessageRole::User,
            content: "hello".into(),
            timestamp: std::time::Instant::now(),
            run_id: None,
        };
        assert_eq!(msg.role, MessageRole::User);
    }

    #[test]
    fn test_orch_snapshot_default() {
        let s = OrchSnapshot::default();
        assert!(s.agents.is_empty());
        assert!(s.locks.is_empty());
        assert!(s.conflicts.is_empty());
    }

    #[tokio::test]
    async fn test_action_submit_starts_real_persistent_run() {
        let mut app = App::default_test();
        let dir = std::env::temp_dir().join(format!("raven-tui-{}", uuid::Uuid::new_v4()));
        app.db_path = dir.join("orchestration.db");
        app.input = "test goal".into();
        app.submit_goal().await.unwrap();
        assert!(app.input.is_empty());
        assert!(app.active_run_id.is_some());
        assert_eq!(app.orch.active_runs.len(), 1);
        assert_eq!(app.orch.active_runs[0].goal, "test goal");
        for _ in 0..20 {
            app.drain_runner_events();
            app.refresh_orchestration().await.unwrap();
            if app.mode == RunMode::Idle {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn test_persisted_running_run_without_runner_is_inspection_only() {
        use odin_orchestrator::persistence::{OrchestrationStore, SqliteOrchestrationStore};
        use odin_orchestrator::task_graph::{TaskGraph, TaskGraphStatus};

        let mut app = App::default_test();
        let dir = std::env::temp_dir().join(format!("raven-tui-stale-{}", uuid::Uuid::new_v4()));
        let db_path = dir.join("orchestration.db");
        std::fs::create_dir_all(&dir).unwrap();
        let store = SqliteOrchestrationStore::new(&db_path).await.unwrap();
        store.initialize().await.unwrap();

        let mut graph = TaskGraph::new("stale persisted run");
        graph.status = TaskGraphStatus::Running;
        store.save_task_graph(&graph).await.unwrap();

        app.db_path = db_path;
        app.refresh_orchestration().await.unwrap();

        assert_eq!(app.orch.active_runs.len(), 1);
        assert_eq!(app.mode, RunMode::Idle);
        assert!(app.running_runs.is_empty());
        assert!(app.active_run_id.is_none());
        assert!(
            app.messages
                .iter()
                .any(|message| { message.content.contains("no live runner is attached") })
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn test_action_input_editing() {
        let mut app = App::default_test();
        app.dispatch(Action::InsertChar('h'));
        app.dispatch(Action::InsertChar('i'));
        assert_eq!(app.input, "hi");
        app.dispatch(Action::DeletePrev);
        assert_eq!(app.input, "h");
        app.dispatch(Action::MoveCursorHome);
        assert_eq!(app.cursor, 0);
        app.dispatch(Action::MoveCursorEnd);
        assert_eq!(app.cursor, 1);
    }

    #[test]
    fn test_unicode_input_editing_uses_character_boundaries() {
        let mut app = App::default_test();
        app.dispatch(Action::InsertChar('é'));
        app.dispatch(Action::InsertChar('🦅'));
        assert_eq!(app.input, "é🦅");
        assert_eq!(app.cursor, app.input.len());
        app.dispatch(Action::MoveCursorLeft);
        app.dispatch(Action::DeleteNext);
        assert_eq!(app.input, "é");
        app.dispatch(Action::DeletePrev);
        assert!(app.input.is_empty());
    }

    #[test]
    fn test_multiline_input_editing() {
        let mut app = App::default_test();
        app.dispatch(Action::InsertChar('a'));
        app.dispatch(Action::InsertNewline);
        app.dispatch(Action::InsertChar('b'));
        assert_eq!(app.input, "a\nb");
    }

    #[test]
    fn test_action_panel_nav() {
        let mut app = App::default_test();
        assert_eq!(app.focused_panel, Panel::Chat);
        app.dispatch(Action::NextPanel);
        assert_eq!(app.focused_panel, Panel::Agents);
        app.dispatch(Action::PrevPanel);
        assert_eq!(app.focused_panel, Panel::Chat);
    }

    #[test]
    fn test_action_quit() {
        let mut app = App::default_test();
        assert!(!app.should_quit);
        app.dispatch(Action::Quit);
        assert!(app.should_quit);
    }

    #[test]
    fn test_action_toggle_help() {
        let mut app = App::default_test();
        assert!(!app.show_help);
        app.dispatch(Action::ToggleHelp);
        assert!(app.show_help);
    }

    #[test]
    fn test_search_mode() {
        let mut app = App::default_test();
        app.is_searching = true;
        app.dispatch(Action::InsertChar('e'));
        app.dispatch(Action::InsertChar('r'));
        assert_eq!(app.search_query, "er");
        app.dispatch(Action::DeletePrev);
        assert_eq!(app.search_query, "e");
    }

    #[test]
    fn test_toggle_search() {
        let mut app = App::default_test();
        assert!(!app.is_searching);
        app.dispatch(Action::ToggleSearch);
        assert!(app.is_searching);
        app.is_searching = true;
        app.search_query = "test".into();
        app.dispatch(Action::ToggleSearch);
        assert!(!app.is_searching);
        assert!(app.search_query.is_empty());
    }

    #[test]
    fn test_esc_cancels_search() {
        let mut app = App::default_test();
        app.is_searching = true;
        app.search_query = "query".into();
        app.dispatch(Action::CancelSearch);
        assert!(!app.is_searching);
        assert!(app.search_query.is_empty());
    }

    #[test]
    fn test_esc_closes_help() {
        let mut app = App::default_test();
        app.show_help = true;
        app.dispatch(Action::CancelSearch);
        assert!(!app.show_help);
        assert!(!app.should_quit);
    }

    #[test]
    fn test_esc_quits_when_idle() {
        let mut app = App::default_test();
        assert!(!app.is_searching);
        assert!(!app.show_help);
        app.dispatch(Action::CancelSearch);
        assert!(app.should_quit);
    }

    #[test]
    fn test_progress_bar() {
        assert_eq!(crate::ui::make_progress_bar(0, 8), "[        ]");
        assert_eq!(crate::ui::make_progress_bar(50, 8), "[====    ]");
        assert_eq!(crate::ui::make_progress_bar(100, 8), "[========]");
    }

    // Event handler tests
    #[test]
    fn test_evt_enter() {
        let h = EventHandler::new_dummy();
        let e = crossterm::event::Event::Key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        ));
        assert_eq!(h.handle_event(&e), Some(Action::Submit));
    }

    #[test]
    fn test_evt_tab() {
        let h = EventHandler::new_dummy();
        let e = crossterm::event::Event::Key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Tab,
            crossterm::event::KeyModifiers::NONE,
        ));
        assert_eq!(h.handle_event(&e), Some(Action::NextPanel));
    }

    #[test]
    fn test_evt_esc() {
        let h = EventHandler::new_dummy();
        let e = crossterm::event::Event::Key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Esc,
            crossterm::event::KeyModifiers::NONE,
        ));
        assert_eq!(h.handle_event(&e), Some(Action::CancelSearch));
    }

    #[test]
    fn test_evt_alt_1_jumps_to_chat() {
        let h = EventHandler::new_dummy();
        let e = crossterm::event::Event::Key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('1'),
            crossterm::event::KeyModifiers::ALT,
        ));
        assert_eq!(h.handle_event(&e), Some(Action::FocusPanel(Panel::Chat)));
    }

    #[test]
    fn test_evt_alt_8_jumps_to_conflicts() {
        let h = EventHandler::new_dummy();
        let e = crossterm::event::Event::Key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('8'),
            crossterm::event::KeyModifiers::ALT,
        ));
        assert_eq!(
            h.handle_event(&e),
            Some(Action::FocusPanel(Panel::Conflicts))
        );
    }

    #[test]
    fn test_evt_shift_enter_inserts_newline() {
        let h = EventHandler::new_dummy();
        let e = crossterm::event::Event::Key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::SHIFT,
        ));
        assert_eq!(h.handle_event(&e), Some(Action::InsertNewline));
    }

    #[test]
    fn test_evt_slash_is_input_for_commands() {
        let h = EventHandler::new_dummy();
        let e = crossterm::event::Event::Key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('/'),
            crossterm::event::KeyModifiers::NONE,
        ));
        assert_eq!(h.handle_event(&e), Some(Action::InsertChar('/')));
    }

    #[test]
    fn test_evt_ctrl_f_toggles_search() {
        let h = EventHandler::new_dummy();
        let e = crossterm::event::Event::Key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('f'),
            crossterm::event::KeyModifiers::CONTROL,
        ));
        assert_eq!(h.handle_event(&e), Some(Action::ToggleSearch));
    }

    #[tokio::test]
    async fn test_cancel_command_requires_approval() {
        let mut app = App::default_test();
        app.active_run_id = Some("1234567890".into());
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        app.runner_tx = Some(tx);
        app.mode = RunMode::Running;
        app.input = "/cancel".into();
        app.submit_goal().await.unwrap();
        assert!(app.pending_approval.is_some());
        app.deny_pending();
        assert!(app.pending_approval.is_none());
    }

    #[tokio::test]
    async fn test_cancel_during_model_wait_targets_active_runner() {
        let mut app = App::default_test();
        app.active_run_id = Some("1234567890".into());
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        app.runner_tx = Some(tx);
        app.mode = RunMode::Running;
        app.apply_runner_event(RunnerEvent::AgentStage {
            agent_id: "agent-wait".into(),
            label: "main".into(),
            stage: AgentStage::WaitingForModel,
            detail: "waiting for model... 10s elapsed (planning/decomposition)".into(),
            elapsed_ms: 10_000,
        });

        app.input = "/cancel".into();
        app.submit_goal().await.unwrap();
        assert!(app.pending_approval.is_some());

        app.approve_pending().await.unwrap();

        assert!(matches!(rx.try_recv().unwrap(), RunnerCommand::Cancel));
        assert!(
            app.messages
                .iter()
                .any(|message| { message.content.contains("waiting for model... 10s elapsed") })
        );
        assert!(
            app.messages
                .iter()
                .any(|message| { message.content.contains("Cancel approved.") })
        );
    }

    #[tokio::test]
    async fn test_followup_message_redirects_active_run() {
        let mut app = App::default_test();
        app.active_run_id = Some("1234567890".into());
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        app.runner_tx = Some(tx);
        app.mode = RunMode::Running;
        app.input = "focus on docs first".into();

        app.submit_goal().await.unwrap();

        assert!(matches!(
            rx.try_recv().unwrap(),
            RunnerCommand::Redirect(message) if message == "focus on docs first"
        ));
        assert!(app.orch.active_runs.is_empty());
    }

    #[tokio::test]
    async fn test_active_run_control_commands_are_sent_to_runner() {
        let mut app = App::default_test();
        app.active_run_id = Some("1234567890".into());
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        app.runner_tx = Some(tx);
        app.mode = RunMode::Running;

        app.input = "/pause".into();
        app.submit_goal().await.unwrap();
        assert!(matches!(rx.try_recv().unwrap(), RunnerCommand::Pause));

        app.input = "/resume".into();
        app.submit_goal().await.unwrap();
        assert!(matches!(rx.try_recv().unwrap(), RunnerCommand::Resume));

        app.input = "/redirect focus tests".into();
        app.submit_goal().await.unwrap();
        assert!(matches!(
            rx.try_recv().unwrap(),
            RunnerCommand::Redirect(message) if message == "focus tests"
        ));

        app.input = "/prio abc123 2".into();
        app.submit_goal().await.unwrap();
        assert!(matches!(
            rx.try_recv().unwrap(),
            RunnerCommand::Reprioritise {
                agent_id_prefix,
                priority
            } if agent_id_prefix == "abc123" && priority == 2
        ));
    }

    #[test]
    fn test_runner_stage_event_adds_chat_feedback() {
        let mut app = App::default_test();
        app.apply_runner_event(RunnerEvent::RunStage {
            run_id: "run-12345678".into(),
            stage: AgentStage::Decomposing,
            detail: "decomposed goal into 3 task(s)".into(),
            elapsed_ms: 12,
        });

        assert!(app.last_runner_event_at.is_some());
        assert!(app.last_runner_stage.contains("decomposing"));
        assert!(
            app.messages
                .iter()
                .any(|message| { message.content.contains("decomposed goal into 3 task(s)") })
        );
    }

    #[test]
    fn test_model_wait_stage_updates_live_agent_panel_and_chat() {
        let mut app = App::default_test();
        app.mode = RunMode::Running;
        app.apply_runner_event(RunnerEvent::AgentStage {
            agent_id: "agent-12345678".into(),
            label: "task-1".into(),
            stage: AgentStage::WaitingForModel,
            detail: "waiting for model... 10s elapsed (planning/decomposition)".into(),
            elapsed_ms: 10_000,
        });

        assert_eq!(app.orch.agents.len(), 1);
        let agent = &app.orch.agents[0];
        assert_eq!(agent.phase, "waiting_for_model");
        assert_eq!(agent.current_tool_call.as_deref(), Some("model"));
        assert_eq!(agent.elapsed_secs, 10);
        assert!(agent.last_update.contains("waiting for model"));
        assert!(
            app.messages
                .iter()
                .any(|message| { message.content.contains("waiting for model... 10s elapsed") })
        );
    }

    #[test]
    fn test_fake_slow_provider_equivalent_keeps_agent_state_fresh() {
        let mut app = App::default_test();
        app.mode = RunMode::Running;
        for elapsed_ms in [0, 1_000, 10_000, 45_000, 90_000] {
            app.apply_runner_event(RunnerEvent::AgentStage {
                agent_id: "agent-slow".into(),
                label: "slow-agent".into(),
                stage: AgentStage::WaitingForModel,
                detail: format!("waiting for model... {}s elapsed", elapsed_ms / 1000),
                elapsed_ms,
            });
        }

        let agent = &app.orch.agents[0];
        assert_eq!(agent.phase, "waiting_for_model");
        assert_eq!(agent.elapsed_secs, 90);
        assert_eq!(agent.last_event_age_secs, Some(0));
        assert!(agent.last_update.contains("90s elapsed"));
        assert!(!app.stale_warning_emitted);
    }

    #[test]
    fn test_no_runner_event_for_15s_warns_with_last_stage() {
        let mut app = App::default_test();
        app.mode = RunMode::Running;
        app.last_runner_event_at =
            Some(std::time::Instant::now() - std::time::Duration::from_secs(16));
        app.last_runner_stage = "waiting_for_model: planning".into();

        app.dispatch(Action::Tick);

        assert!(app.stale_warning_emitted);
        assert!(app.messages.iter().any(|message| {
            message
                .content
                .contains("No runner event for 15s. Last known stage")
        }));
    }

    #[test]
    fn test_submit_progress_sequence_to_result_updates_ui() {
        let mut app = App::default_test();
        app.apply_runner_event(RunnerEvent::RunStarted {
            run_id: "run-12345678".into(),
            goal: "demo goal".into(),
            task_count: 1,
        });
        app.apply_runner_event(RunnerEvent::AgentStarted {
            agent_id: "agent-12345678".into(),
            label: "main".into(),
            task: "demo goal".into(),
        });
        app.apply_runner_event(RunnerEvent::AgentStage {
            agent_id: "agent-12345678".into(),
            label: "main".into(),
            stage: AgentStage::WaitingForModel,
            detail: "model call in progress... 1s elapsed (action/tool selection)".into(),
            elapsed_ms: 1_000,
        });
        app.apply_runner_event(RunnerEvent::AgentStage {
            agent_id: "agent-12345678".into(),
            label: "main".into(),
            stage: AgentStage::RunningTool,
            detail: "model requested tool call(s): file_read".into(),
            elapsed_ms: 1_500,
        });
        app.apply_runner_event(RunnerEvent::AgentFinished {
            agent_id: "agent-12345678".into(),
            label: "main".into(),
            success: true,
            summary: "done".into(),
        });
        app.apply_runner_event(RunnerEvent::RunFinished {
            run_id: "run-12345678".into(),
            success: true,
            summary: "final done".into(),
        });

        assert_eq!(app.mode, RunMode::Idle);
        assert!(app.runner_tx.is_none());
        assert!(
            app.messages
                .iter()
                .any(|message| { message.content.contains("Running 1 task(s) for: demo goal") })
        );
        assert!(
            app.messages
                .iter()
                .any(|message| { message.content.contains("Final summary for run-1234") })
        );
    }

    #[test]
    fn test_blocker_and_error_events_surface_in_ui() {
        let mut app = App::default_test();
        app.mode = RunMode::Running;

        app.apply_runner_event(RunnerEvent::AgentQueued {
            agent_id: "agent-lock".into(),
            label: "writer".into(),
            reason: "Waiting for write lock on 'src/lib.rs'".into(),
        });
        app.apply_runner_event(RunnerEvent::AgentStage {
            agent_id: "agent-approval".into(),
            label: "danger".into(),
            stage: AgentStage::ApprovalNeeded,
            detail: "approval required for shell".into(),
            elapsed_ms: 0,
        });
        app.apply_runner_event(RunnerEvent::AgentStage {
            agent_id: "agent-fail".into(),
            label: "tool-user".into(),
            stage: AgentStage::Failed,
            detail: "tool failed: missing file".into(),
            elapsed_ms: 500,
        });

        assert!(app.orch.agents.iter().any(|agent| {
            agent.phase == "waiting_for_lock"
                && agent
                    .blocked_reason
                    .as_deref()
                    .unwrap_or_default()
                    .contains("write lock")
        }));
        assert!(app.orch.agents.iter().any(|agent| {
            agent.phase == "approval_needed"
                && agent
                    .blocked_reason
                    .as_deref()
                    .unwrap_or_default()
                    .contains("approval required")
        }));
        assert!(
            app.messages
                .iter()
                .any(|message| { message.content.contains("tool-user failed: tool failed") })
        );
    }

    // Helpers
    impl App {
        fn default_test() -> Self {
            Self {
                db_path: std::path::PathBuf::from("/tmp/t"),
                messages: vec![],
                input: String::new(),
                cursor: 0,
                focused_panel: Panel::Chat,
                side_scroll: 0,
                chat_scroll: 0,
                show_help: false,
                should_quit: false,
                orch: OrchSnapshot::default(),
                running_runs: vec![],
                mode: RunMode::Idle,
                last_tick: std::time::Instant::now(),
                log_entries: vec![],
                tool_displays: vec![],
                skill_displays: vec![],
                provider_displays: vec![],
                search_query: String::new(),
                is_searching: false,
                provider_name: String::new(),
                model_name: String::new(),
                elapsed: std::time::Duration::ZERO,
                tool_call_count: 0,
                error_count: 0,
                last_action: String::new(),
                runner_tx: None,
                runner_rx: None,
                active_run_id: None,
                pending_approval: None,
                max_iterations: 100,
                last_runner_event_at: None,
                last_runner_stage: String::new(),
                stale_warning_emitted: false,
                offline_run_notice_emitted: false,
                live_agents: std::collections::HashMap::new(),
            }
        }
    }

    impl EventHandler {
        fn new_dummy() -> Self {
            let (_, rx) = tokio::sync::mpsc::unbounded_channel();
            Self {
                terminal_rx: rx,
                tick_rate: std::time::Duration::from_millis(500),
            }
        }
    }
}
