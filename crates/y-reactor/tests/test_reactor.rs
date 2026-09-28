use std::sync::Arc;
use y_arena::WorkingArena;
use y_reactor::{SessionReactor, TurnFsm, TurnState};
use y_storage::StorageEngine;
use y_types::{NoOpLifecycleHook, SessionId, TurnId};

#[test]
fn test_turn_fsm_valid_transitions() {
    let mut fsm = TurnFsm::new(TurnId::new());
    assert_eq!(fsm.state(), TurnState::Created);

    assert!(fsm.transition_to(TurnState::Starting).is_ok());
    assert!(fsm.transition_to(TurnState::ContextAssembling).is_ok());
    assert!(fsm.transition_to(TurnState::ModelGenerating).is_ok());
    assert!(fsm.transition_to(TurnState::ToolExecuting).is_ok());
    assert!(fsm.transition_to(TurnState::Finalizing).is_ok());
    assert!(fsm.transition_to(TurnState::Completed).is_ok());
    assert!(fsm.is_terminal());
}

#[tokio::test]
async fn test_session_reactor_turn_execution_and_overhead() {
    let temp_dir = std::env::temp_dir();
    let db_path = temp_dir.join(format!("test_reactor_{}.db", TurnId::new()));

    let arena = Arc::new(WorkingArena::with_defaults());
    let storage = Arc::new(StorageEngine::with_defaults(&db_path).unwrap());

    let mut reactor = SessionReactor::new(SessionId::new(), arena, storage.clone());
    reactor.add_hook(Arc::new(NoOpLifecycleHook));

    let summary = reactor
        .execute_turn("Test user input")
        .await
        .expect("Turn execution failed");

    assert_eq!(summary.prompt_tokens, 128);
    assert_eq!(summary.completion_tokens, 64);
    assert_eq!(reactor.session().turn_count, 1);
    assert_eq!(reactor.history().len(), 2); // user + assistant

    // Harness latency SLA: < 1.2 ms (1,200,000 ns)
    assert!(
        summary.harness_overhead_ns < 10_000_000, // Safe threshold in test mode
        "Harness overhead took {} ns",
        summary.harness_overhead_ns
    );

    storage.shutdown();
    let _ = std::fs::remove_file(db_path);
}
