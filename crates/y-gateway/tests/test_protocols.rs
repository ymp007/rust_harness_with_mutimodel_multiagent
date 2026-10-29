use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use y_gateway::{ApiEndpoint, ApiFormat, OpenRouterClient, OpenRouterConfig};

async fn case(format: ApiFormat) {
    let key_env = match format {
        ApiFormat::Anthropic => Some("YCODE_TEST_ANTHROPIC_KEY"),
        ApiFormat::Gemini => Some("YCODE_TEST_GEMINI_KEY"),
        ApiFormat::Openai => None,
    };
    if let Some(name) = key_env {
        std::env::set_var(name, "offline-worker-credential");
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!(
        "http://{}/models/{{model}}:generateContent",
        listener.local_addr().unwrap()
    );
    let cfg = OpenRouterConfig {
        api_key: "offline-secret".into(),
        model: "test-model".into(),
        referer: String::new(),
        title: String::new(),
        timeout_secs: 5,
        max_tokens: 1024,
    };
    let client = OpenRouterClient::new(cfg)
        .unwrap()
        .with_routes(std::collections::HashMap::from([(
            "worker".into(),
            ApiEndpoint {
                url,
                format,
                key_env: key_env.map(str::to_owned),
            },
        )]))
        .with_provider("worker")
        .unwrap();
    let server = tokio::spawn(async move {
        let mut captured = Vec::new();
        for round in 0..2 {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = Vec::new();
            let mut buf = [0; 4096];
            let (end, len) = loop {
                let n = socket.read(&mut buf).await.unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buf[..n]);
                if let Some(p) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&bytes[..p]);
                    assert!(!headers.contains("offline-secret"));
                    assert!(!headers.to_lowercase().contains("authorization:"));
                    if format == ApiFormat::Anthropic {
                        assert!(headers.contains("x-api-key: offline-worker-credential"));
                    }
                    if format == ApiFormat::Gemini {
                        assert!(headers.contains("x-goog-api-key: offline-worker-credential"));
                    }
                    if format == ApiFormat::Anthropic {
                        assert!(headers.contains("anthropic-version: 2023-06-01"));
                    }
                    if format == ApiFormat::Gemini {
                        assert!(headers.starts_with("POST /models/test-model:generateContent"));
                    }
                    let len = headers
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
                    break (p + 4, len);
                }
            };
            while bytes.len() < end + len {
                let n = socket.read(&mut buf).await.unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buf[..n]);
            }
            captured.push(serde_json::from_slice::<Value>(&bytes[end..end + len]).unwrap());
            let response=match (format,round) {
                (ApiFormat::Openai,0)=>json!({"choices":[{"finish_reason":"tool_calls","message":{"role":"assistant","content":null,"tool_calls":[{"id":"call-1","type":"function","function":{"name":"read_file","arguments":"{\"path\":\"main.rs\"}"}}]}}]}),
                (ApiFormat::Anthropic,0)=>json!({"stop_reason":"tool_use","content":[{"type":"tool_use","id":"call-1","name":"read_file","input":{"path":"main.rs"}}]}),
                (ApiFormat::Gemini,0)=>json!({"candidates":[{"finishReason":"STOP","content":{"parts":[{"functionCall":{"id":"call-1","name":"read_file","args":{"path":"main.rs"}},"thoughtSignature":"opaque-signature"}]}}]}),
                (ApiFormat::Openai,_)=>json!({"choices":[{"finish_reason":"stop","message":{"role":"assistant","content":"verified"}}]}),
                (ApiFormat::Anthropic,_)=>json!({"stop_reason":"end_turn","content":[{"type":"text","text":"verified"}]}),
                (ApiFormat::Gemini,_)=>json!({"candidates":[{"finishReason":"STOP","content":{"parts":[{"text":"verified"}]}}]}),
            }.to_string();
            socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",response.len()).as_bytes()).await.unwrap();
        }
        captured
    });
    let mut messages = vec![
        json!({"role":"system","content":"Inspect carefully"}),
        json!({"role":"user","content":"Read main.rs"}),
    ];
    let tools = [
        json!({"type":"function","function":{"name":"read_file","description":"Read file","parameters":{"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}}}),
    ];
    let response = client.complete(&messages, &tools).await.unwrap();
    assert_eq!(response["tool_calls"][0]["function"]["name"], "read_file");
    messages.push(response);
    messages.push(json!({"role":"tool","tool_call_id":"call-1","content":"fn main() {}"}));
    assert_eq!(
        client.complete(&messages, &tools).await.unwrap()["content"],
        "verified"
    );
    let requests = server.await.unwrap();
    match format {
        ApiFormat::Openai => {
            assert!(requests[0].get("provider").is_none());
            assert_eq!(requests[1]["messages"][3]["tool_call_id"], "call-1");
        }
        ApiFormat::Anthropic => {
            assert_eq!(requests[0]["tools"][0]["input_schema"]["type"], "object");
            assert_eq!(
                requests[1]["messages"][2]["content"][0]["tool_use_id"],
                "call-1"
            );
        }
        ApiFormat::Gemini => {
            assert_eq!(
                requests[1]["contents"][1]["parts"][0]["thoughtSignature"],
                "opaque-signature"
            );
            assert_eq!(
                requests[1]["contents"][2]["parts"][0]["functionResponse"]["name"],
                "read_file"
            );
        }
    }
}
#[tokio::test]
async fn compatible_tool_roundtrip() {
    case(ApiFormat::Openai).await;
}
#[tokio::test]
async fn anthropic_tool_roundtrip() {
    case(ApiFormat::Anthropic).await;
}
#[tokio::test]
async fn gemini_tool_roundtrip_preserves_thought_signatures() {
    case(ApiFormat::Gemini).await;
}
