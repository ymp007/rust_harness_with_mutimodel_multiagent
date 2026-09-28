use y_gateway::{ShadowWorkerSupervisor, SseStreamParser, StreamCancellationToken};

#[test]
fn test_sse_parser_split_utf8_resilience() {
    let mut parser = SseStreamParser::new();

    // Split multibyte emoji "🚀" (4 bytes: [240, 159, 154, 128]) across chunks
    let chunk1 = b"data: Hello \xF0\x9F"; // Incomplete 2 bytes
    let tokens1 = parser.parse_chunk(chunk1).expect("Chunk 1 failed");
    assert_eq!(tokens1.len(), 0); // No newline yet, buffered
    assert!(parser.buffered_bytes() > 0);

    let chunk2 = b"\x9A\x80 World\n\n"; // Completing 2 bytes and ending line
    let tokens2 = parser.parse_chunk(chunk2).expect("Chunk 2 failed");
    assert_eq!(parser.buffered_bytes(), 0);
    assert_eq!(tokens2.len(), 1);
    assert!(tokens2[0].contains("🚀"));
    assert_eq!(tokens2[0], "Hello 🚀 World");
}

#[test]
fn test_shadow_gpu_worker_instant_failover() {
    let supervisor = ShadowWorkerSupervisor::new();
    let initial_primary = supervisor.primary_id();
    let initial_shadow = supervisor.shadow_id();

    assert_eq!(initial_primary, 1);
    assert_eq!(initial_shadow, 2);

    // Primary crashes!
    let new_primary = supervisor.handle_primary_crash().expect("Failover failed");
    assert_eq!(new_primary, initial_shadow); // Promoted shadow
    assert_eq!(supervisor.primary_id(), 2);
    assert_eq!(supervisor.shadow_id(), 3); // Pre-warmed fresh shadow
}

#[test]
fn test_active_stream_cancellation() {
    let token = StreamCancellationToken::new();
    assert!(!token.is_cancelled());

    token.cancel();
    assert!(token.is_cancelled());
}
