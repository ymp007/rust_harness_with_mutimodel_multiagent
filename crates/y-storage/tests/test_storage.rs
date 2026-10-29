use y_storage::{StagingRing, StorageEngine};
use y_types::{SessionId, TurnId, TurnSummary};

#[test]
fn invalid_database_path_fails_at_startup() {
    let path = std::env::temp_dir().join(format!("missing-{}/state.db", TurnId::new()));
    assert!(StorageEngine::with_defaults(path).is_err());
}

#[test]
fn shutdown_waits_for_all_queued_turns_to_persist() {
    let path = std::env::temp_dir().join(format!("durability-{}.db", TurnId::new()));
    let engine = StorageEngine::with_defaults(&path).unwrap();
    for _ in 0..20 {
        engine
            .record_turn(TurnSummary {
                turn_id: TurnId::new(),
                session_id: SessionId::new(),
                prompt_tokens: 1,
                completion_tokens: 1,
                wall_time_ns: 1,
                harness_overhead_ns: 1,
                tool_invocations: 0,
            })
            .unwrap();
    }
    engine.shutdown();
    let connection = rusqlite::Connection::open(&path).unwrap();
    let count: usize = connection
        .query_row("SELECT COUNT(*) FROM turns", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 20);
    drop(connection);
    std::fs::remove_file(path).unwrap();
}

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

#[test]
fn acknowledged_turn_is_actually_persisted() {
    let db_path = std::env::temp_dir().join(format!("persist-{}.db", TurnId::new()));
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
    let turn_id = summary.turn_id.to_string();
    engine.record_turn(summary).expect("Failed to record turn");

    // No sleep: shutdown joins the worker, so the row must be durable on return.
    assert!(!engine.is_degraded(), "storage degraded before shutdown");
    engine.shutdown();
    assert!(
        !engine.persist_failed(),
        "a persist failed while the turn was acknowledged"
    );

    let connection = rusqlite::Connection::open(&db_path).unwrap();
    let row: (i64, i64, i64) = connection
        .query_row(
            "SELECT prompt_tokens, completion_tokens, tool_invocations FROM turns WHERE turn_id = ?1",
            [&turn_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .expect("acknowledged turn was not persisted");
    assert_eq!(row, (150, 75, 1));
    drop(connection);
    std::fs::remove_file(db_path).unwrap();
}

#[test]
fn schema_mismatch_is_rejected_instead_of_silently_dropping_turns() {
    let db_path = std::env::temp_dir().join(format!("drift-{}.db", TurnId::new()));
    {
        // A `turns` table written by an incompatible build.
        let c = rusqlite::Connection::open(&db_path).unwrap();
        c.execute_batch("CREATE TABLE turns (turn_id TEXT PRIMARY KEY, session_id TEXT NOT NULL);")
            .unwrap();
    }

    let engine = StorageEngine::with_defaults(&db_path);
    assert!(
        engine.is_err(),
        "schema drift must fail at startup, not lose every turn silently"
    );
    std::fs::remove_file(db_path).unwrap();
}

#[test]
fn staging_survives_wal_failure() {
    let engine = StorageEngine::with_defaults(
        std::env::temp_dir().join(format!("stage-{}.db", TurnId::new())),
    )
    .unwrap();
    for _ in 0..10 {
        engine
            .record_turn(TurnSummary {
                turn_id: TurnId::new(),
                session_id: SessionId::new(),
                prompt_tokens: 1,
                completion_tokens: 1,
                wall_time_ns: 1,
                harness_overhead_ns: 1,
                tool_invocations: 0,
            })
            .unwrap();
    }
    // The RAM ring is the durability fallback and must be populated regardless
    // of what the WAL worker accepted.
    assert_eq!(engine.ring().len(), 10);
    engine.shutdown();
}

#[test]
fn evicted_deltas_are_observable() {
    let ring = StagingRing::new(2);
    for _ in 0..5 {
        ring.stage(TurnSummary {
            turn_id: TurnId::new(),
            session_id: SessionId::new(),
            prompt_tokens: 1,
            completion_tokens: 1,
            wall_time_ns: 1,
            harness_overhead_ns: 1,
            tool_invocations: 0,
        });
    }
    assert_eq!(ring.len(), 2);
    assert_eq!(ring.evicted(), 3, "silent data loss must be observable");
    assert_eq!(ring.capacity(), 2);
}
