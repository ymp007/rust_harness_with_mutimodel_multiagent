use std::time::Duration;
use y_storage::{StagingRing, StorageEngine};
use y_types::{SessionId, TurnId, TurnSummary};

#[test]
fn test_staging_ring_microsecond_delta() {
    let ring = StagingRing::new(10);
    assert!(ring.is_empty());

    let summary = TurnSummary {
        turn_id: TurnId::new(),
        session_id: SessionId::new(),
        prompt_tokens: 100,
        completion_tokens: 50,
        wall_time_ns: 500_000,
        harness_overhead_ns: 200_000,
        tool_invocations: 0,
    };

    ring.stage(summary);
    assert_eq!(ring.len(), 1);

    let drained = ring.drain_all();
    assert_eq!(drained.len(), 1);
    assert!(ring.is_empty());
}

#[tokio::test]
async fn test_storage_engine_sqlite_wal() {
    let temp_dir = std::env::temp_dir();
    let db_path = temp_dir.join(format!("test_y_storage_{}.db", TurnId::new()));

    let engine = StorageEngine::with_defaults(&db_path).expect("Failed to initialize storage");

    let summary = TurnSummary {
        turn_id: TurnId::new(),
        session_id: SessionId::new(),
        prompt_tokens: 150,
        completion_tokens: 75,
        wall_time_ns: 800_000,
        harness_overhead_ns: 300_000,
        tool_invocations: 1,
    };

    engine.record_turn(summary).expect("Failed to record turn");
    // Give async WAL worker a moment to process commit
    tokio::time::sleep(Duration::from_millis(100)).await;

    engine.shutdown();
    let _ = std::fs::remove_file(db_path);
}
