use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use y_gateway::{OpenRouterClient, OpenRouterConfig, SseStreamParser};

fn config() -> OpenRouterConfig {
    OpenRouterConfig {
        api_key: "secret-offline-test-key".into(),
        model: "test/model".into(),
        referer: "http://localhost".into(),
        title: "test".into(),
        timeout_secs: 5,
        max_tokens: 1024,
    }
}

async fn mock(
    responses: Vec<(u16, String, &'static str)>,
) -> (OpenRouterClient, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let client = OpenRouterClient::with_endpoint(
        config(),
        &format!("http://{}/chat", listener.local_addr().unwrap()),
    )
    .unwrap();
    let task = tokio::spawn(async move {
        for (status, body, extra) in responses {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = Vec::new();
            let mut buffer = [0; 4096];
            let (end, length) = loop {
                let n = socket.read(&mut buffer).await.unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buffer[..n]);
                if let Some(pos) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                    let head = String::from_utf8_lossy(&bytes[..pos]);
                    let length = head
                        .lines()
                        .find_map(|l| {
                            let (k, v) = l.split_once(':')?;
                            if k.eq_ignore_ascii_case("content-length") {
                                v.trim().parse::<usize>().ok()
                            } else {
                                None
                            }
                        })
                        .unwrap();
                    break (pos + 4, length);
                }
            };
            while bytes.len() < end + length {
                let n = socket.read(&mut buffer).await.unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buffer[..n]);
            }
            let head = format!(
                "HTTP/1.1 {status} Mock\r\nContent-Length: {}\r\nConnection: close\r\n{extra}\r\n",
                body.len()
            );
            socket.write_all(head.as_bytes()).await.unwrap();
            let _ = socket.write_all(body.as_bytes()).await;
        }
    });
    (client, task)
}

fn user() -> Vec<serde_json::Value> {
    vec![json!({"role":"user","content":"hello"})]
}
fn answer() -> String {
    json!({"choices":[{"message":{"role":"assistant","content":"hello"},"finish_reason":"stop"}]})
        .to_string()
}

#[test]
fn configuration_debug_redacts_key_and_http_requires_loopback() {
    assert!(!format!("{:?}", config()).contains("secret-offline-test-key"));
    assert!(OpenRouterClient::with_endpoint(config(), "http://example.com/chat").is_err());
}

#[tokio::test]
async fn authentication_and_credit_errors_are_actionable_and_redacted() {
    for (status, hint) in [(401, "OPENROUTER_API_KEY"), (402, "credits")] {
        let (client, server) = mock(vec![(status, "secret-offline-test-key".into(), "")]).await;
        let error = client.complete(&user(), &[]).await.unwrap_err().to_string();
        assert!(error.contains(hint));
        assert!(!error.contains("secret-offline-test-key"));
        server.await.unwrap();
    }
}

#[tokio::test]
async fn explicit_rate_limit_is_retried_with_retry_after() {
    let (client, server) = mock(vec![
        (429, "slow down".into(), "Retry-After: 0\r\n"),
        (200, answer(), ""),
    ])
    .await;
    assert_eq!(
        client.complete(&user(), &[]).await.unwrap()["content"],
        "hello"
    );
    server.await.unwrap();
}

#[tokio::test]
async fn invalid_empty_and_truncated_responses_fail() {
    for body in ["not json".into(), "{\"choices\":[]}".into(), "{\"error\":{\"message\":\"broken\"}}".into(), json!({"choices":[{"message":{"role":"assistant","content":"partial"},"finish_reason":"length"}]}).to_string()] {
        let (client, server) = mock(vec![(200, body, "")]).await;
        assert!(client.complete(&user(), &[]).await.is_err()); server.await.unwrap();
    }
}

#[tokio::test]
async fn streaming_preserves_unicode_and_requires_done() {
    let chunk = "data: {\"choices\":[{\"delta\":{\"content\":\"hello 🚀\"}}]}\r\n\r\n";
    let (client, server) = mock(vec![(
        200,
        format!(": keepalive\r\n\r\n{chunk}data: [DONE]\r\n\r\n"),
        "Content-Type: text/event-stream\r\n",
    )])
    .await;
    let mut printed = String::new();
    assert_eq!(
        client
            .stream(&user(), |s| printed.push_str(s))
            .await
            .unwrap(),
        "hello 🚀"
    );
    assert_eq!(printed, "hello 🚀");
    server.await.unwrap();
    let (client, server) = mock(vec![(200, chunk.into(), "")]).await;
    assert!(client
        .stream(&user(), |_| {})
        .await
        .unwrap_err()
        .to_string()
        .contains("disconnected"));
    server.await.unwrap();
}

#[tokio::test]
async fn streaming_error_payload_is_not_silently_ignored() {
    let (client, server) = mock(vec![(
        200,
        "data: {\"error\":{\"message\":\"provider failed\"}}\n\ndata: [DONE]\n\n".into(),
        "",
    )])
    .await;
    assert!(client
        .stream(&user(), |_| {})
        .await
        .unwrap_err()
        .to_string()
        .contains("provider failed"));
    server.await.unwrap();
}

#[tokio::test]
async fn http_body_limit_is_enforced_during_read() {
    let (client, server) = mock(vec![(
        200,
        "x".repeat(y_gateway::openrouter::MAX_BODY_BYTES + 1),
        "",
    )])
    .await;
    assert!(client
        .complete(&user(), &[])
        .await
        .unwrap_err()
        .to_string()
        .contains("8 MiB"));
    server.await.unwrap();
}

#[test]
fn sse_handles_multiline_events_comments_done_and_invalid_utf8() {
    let mut parser = SseStreamParser::new();
    assert!(parser
        .parse_chunk(b":comment\r\ndata: first\r\n")
        .unwrap()
        .is_empty());
    assert_eq!(
        parser
            .parse_chunk(b"data: second\r\n\r\ndata: [DONE]\n\n")
            .unwrap(),
        ["first\nsecond", "[DONE]"]
    );
    assert!(parser.parse_chunk(b"data: ignored\n\n").unwrap().is_empty());
    let mut parser = SseStreamParser::new();
    assert!(parser.parse_chunk(b"data: \xff\n\n").is_err());
}
