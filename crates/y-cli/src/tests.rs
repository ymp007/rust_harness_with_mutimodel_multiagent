use crate::{agent, process, session, tools};
use serde_json::{json, Value};
use std::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use y_gateway::{OpenRouterClient, OpenRouterConfig};

fn permissions(write: bool) -> tools::Permissions {
    tools::Permissions {
        read_only: !write,
        allow_write: write,
        allow_shell: false,
    }
}

#[test]
fn paths_deny_traversal_secrets_and_windows_streams() {
    let temp = tempfile::tempdir().unwrap();
    let ws = tools::Workspace::new(temp.path()).unwrap();
    for path in [
        "../outside",
        "nested/../../escape",
        ".env",
        ".env.local",
        ".git/config",
        ".ycode/sessions/a",
        ".ssh/id_rsa",
        "file:stream",
        "foo.",
        "foo ",
        "NUL",
        "CON.txt",
        "COM1",
    ] {
        assert!(ws.resolve(path).is_err(), "{path}");
    }
    assert!(ws.resolve("src/main.rs").is_ok());
    assert!(ws.resolve(".env.example").is_ok());
}

#[tokio::test]
async fn file_tools_require_permission_and_preserve_unrelated_content() {
    let temp = tempfile::tempdir().unwrap();
    let ws = tools::Workspace::new(temp.path()).unwrap();
    fs::write(temp.path().join("source.txt"), "one\r\ntwo\r\nthree\r\n").unwrap();
    let edit = json!({"path":"source.txt","old_text":"two","new_text":"changed"});
    let denied = tools::execute(&ws, &permissions(false), "edit_file", &edit)
        .await
        .unwrap();
    assert!(denied.contains("denied"));
    assert!(ws.read("source.txt").unwrap().contains("two"));
    tools::execute(&ws, &permissions(true), "edit_file", &edit)
        .await
        .unwrap();
    assert_eq!(
        ws.read("source.txt").unwrap(),
        "one\r\nchanged\r\nthree\r\n"
    );
    assert!(tools::execute(&ws, &permissions(true), "edit_file", &edit)
        .await
        .is_err());
    tools::execute(
        &ws,
        &permissions(true),
        "edit_file",
        &json!({"path":"source.txt","old_text":"one\nchanged","new_text":"ONE\nCHANGED"}),
    )
    .await
    .unwrap();
    assert_eq!(
        ws.read("source.txt").unwrap(),
        "ONE\r\nCHANGED\r\nthree\r\n"
    );
    let create = json!({"path":"source.txt","content":"overwrite"});
    assert!(
        tools::execute(&ws, &permissions(true), "write_file", &create)
            .await
            .is_err()
    );
    fs::write(temp.path().join("duplicates.txt"), "same same").unwrap();
    assert!(tools::execute(
        &ws,
        &permissions(true),
        "edit_file",
        &json!({"path":"duplicates.txt","old_text":"same","new_text":"changed"})
    )
    .await
    .is_err());
}

#[test]
fn sessions_save_resume_lock_and_recover_without_replaying() {
    let temp = tempfile::tempdir().unwrap();
    let ws = tools::Workspace::new(temp.path()).unwrap();
    let lock = session::acquire_lock(&ws).unwrap();
    assert!(session::acquire_lock(&ws).is_err());
    let mut session = session::Session::new(&ws, "test/model").unwrap();
    session.messages.push(json!({"role":"assistant","tool_calls":[{"id":"pending","type":"function","function":{"name":"write_file","arguments":"{}"}}]}));
    session.save(&ws).unwrap();
    let loaded = session::Session::load(&ws, "last").unwrap();
    assert_eq!(loaded.id, session.id);
    assert_eq!(loaded.messages.last().unwrap()["tool_call_id"], "pending");
    assert!(loaded.messages.last().unwrap()["content"]
        .as_str()
        .unwrap()
        .contains("Side effects"));
    assert!(session::Session::load(&ws, "../escape").is_err());
    drop(lock);
    assert!(session::acquire_lock(&ws).is_ok());
}

#[tokio::test]
async fn complete_agent_tool_roundtrip_and_resume_offline() {
    let temp = tempfile::tempdir().unwrap();
    let ws = tools::Workspace::new(temp.path()).unwrap();
    fs::write(temp.path().join("answer.txt"), "forty-two").unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/chat", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let mut requests = Vec::new();
        for step in 0..3 {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = Vec::new();
            let mut buf = [0; 4096];
            let (header_end, length) = loop {
                let n = socket.read(&mut buf).await.unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buf[..n]);
                if let Some(pos) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                    let head = String::from_utf8_lossy(&bytes[..pos]);
                    let len = head
                        .lines()
                        .find_map(|l| {
                            let (key, value) = l.split_once(':')?;
                            if key.eq_ignore_ascii_case("content-length") {
                                value.trim().parse::<usize>().ok()
                            } else {
                                None
                            }
                        })
                        .unwrap();
                    break (pos + 4, len);
                }
            };
            while bytes.len() < header_end + length {
                let n = socket.read(&mut buf).await.unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buf[..n]);
            }
            let request: Value =
                serde_json::from_slice(&bytes[header_end..header_end + length]).unwrap();
            requests.push(request);
            let message = if step == 0 {
                json!({"role":"assistant","content":null,"tool_calls":[{"id":"read-1","type":"function","function":{"name":"read_file","arguments":"{\"path\":\"answer.txt\"}"}}]})
            } else if step == 1 {
                json!({"role":"assistant","content":"The answer is forty-two."})
            } else {
                json!({"role":"assistant","content":"I remember forty-two."})
            };
            let body = json!({"choices":[{"message":message,"finish_reason":if step == 0 {"tool_calls"} else {"stop"}}]}).to_string();
            socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
        }
        requests
    });
    let cfg = OpenRouterConfig {
        api_key: "offline-test-key".into(),
        model: "test/model".into(),
        referer: "http://localhost".into(),
        title: "test".into(),
        timeout_secs: 5,
        max_tokens: 1024,
    };
    let client = OpenRouterClient::with_endpoint(cfg, &endpoint).unwrap();
    let mut session = session::Session::new(&ws, "test/model").unwrap();
    let text = agent::turn(
        &client,
        &ws,
        &mut session,
        &permissions(false),
        "Read answer.txt",
        3,
    )
    .await
    .unwrap();
    assert!(text.contains("forty-two"));
    let mut resumed = session::Session::load(&ws, "last").unwrap();
    assert!(agent::turn(
        &client,
        &ws,
        &mut resumed,
        &permissions(false),
        "What was the answer?",
        3
    )
    .await
    .unwrap()
    .contains("remember"));
    let requests = server.await.unwrap();
    assert!(requests[1]["messages"]
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m["role"] == "tool"
            && m["tool_call_id"] == "read-1"
            && m["content"].as_str().unwrap().contains("forty-two")));
    assert!(
        requests[2]["messages"].as_array().unwrap().len()
            > requests[1]["messages"].as_array().unwrap().len()
    );
    assert!(!requests[0]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .any(|t| t["function"]["name"] == "run_command"));
}

#[tokio::test]
async fn command_exit_status_and_output_are_reported() {
    let temp = tempfile::tempdir().unwrap();
    let result = process::run(temp.path(), "echo YCODE_OK").await.unwrap();
    assert!(result.contains("YCODE_OK"));
    assert!(result.contains("Some(0)"));
    let result = process::run(temp.path(), "exit 7").await.unwrap();
    assert!(result.contains("Some(7)"));
}

#[test]
fn cli_rejects_unknown_flags_and_invalid_limits() {
    assert!(crate::cli()
        .try_get_matches_from(["ycode", "--unknown"])
        .is_err());
    assert!(crate::cli()
        .try_get_matches_from(["ycode", "--model"])
        .is_err());
    assert!(crate::cli()
        .try_get_matches_from(["ycode", "--max-steps", "0"])
        .is_err());
    assert!(crate::cli()
        .try_get_matches_from(["ycode", "--read-only", "--allow-shell"])
        .is_err());
    let matches = crate::cli()
        .try_get_matches_from(["ycode", "--resume", "hello"])
        .unwrap();
    assert_eq!(matches.get_one::<String>("resume").unwrap(), "last");
    assert_eq!(crate::prompt(&matches), "hello");
}

#[test]
fn terminal_output_is_safe_and_utf8_limits_hold() {
    assert!(!tools::display("a\u{001b}[31mb\u{202e}c").contains('\u{001b}'));
    assert!(tools::bounded(&"🚀".repeat(10000)).ends_with("[output truncated]"));
}
