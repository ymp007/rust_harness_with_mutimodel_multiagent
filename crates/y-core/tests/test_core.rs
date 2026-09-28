use y_core::{SessionId, TurnId, YCodeEngine};

#[tokio::test]
async fn test_y_code_engine_lifecycle_and_turn() {
    let temp_dir = std::env::temp_dir();
    let db_path = temp_dir.join(format!("test_core_{}.db", TurnId::new()));

    let engine = YCodeEngine::new(&db_path).expect("Failed to create YCodeEngine");

    let session_id = SessionId::new();
    let mut session = engine.create_session(session_id);

    let summary = session
        .execute_turn("Execute core engine verification")
        .await
        .expect("Turn execution failed");

    assert_eq!(summary.prompt_tokens, 128);
    assert_eq!(summary.completion_tokens, 64);
    assert_eq!(session.session().turn_count, 1);

    // Verify telemetry
    let snapshot = engine.telemetry().capture_snapshot();
    assert!(snapshot.total_memory_mb > 0.0);

    engine.shutdown();
    let _ = std::fs::remove_file(db_path);
}
