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

    // Token counts are derived from the assembled history, not fixed literals.
    assert!(summary.prompt_tokens > 0);
    assert!(summary.completion_tokens > 0);
    assert_eq!(reactor.session().turn_count, 1);
    assert_eq!(reactor.history().len(), 2); // user + assistant
    assert_eq!(
        reactor.session().total_prompt_tokens,
        u64::from(summary.prompt_tokens)
    );

    // Overhead must be a subset of wall time, not the whole turn.
    assert!(summary.harness_overhead_ns <= summary.wall_time_ns);
    assert!(summary.wall_time_ns > 0, "wall time was never measured");

    storage.shutdown();
    let _ = std::fs::remove_file(db_path);
}

#[tokio::test]
async fn completed_turn_is_persisted_to_the_database() {
    let db_path = std::env::temp_dir().join(format!("test_reactor_persist_{}.db", TurnId::new()));
    let arena = Arc::new(WorkingArena::with_defaults());
    let storage = Arc::new(StorageEngine::with_defaults(&db_path).unwrap());
    let mut reactor = SessionReactor::new(SessionId::new(), arena, storage.clone());

    reactor.execute_turn("persist me").await.unwrap();
    storage.shutdown();

    let connection = rusqlite::Connection::open(&db_path).unwrap();
    let count: usize = connection
        .query_row("SELECT COUNT(*) FROM turns", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 1, "a completed turn must be durable");
    drop(connection);
    let _ = std::fs::remove_file(db_path);
}
