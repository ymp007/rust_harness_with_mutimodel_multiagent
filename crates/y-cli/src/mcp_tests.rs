use crate::{
    mcp::{Hub, Server},
    tools::{self, Permissions, Workspace},
};
use futures_util::StreamExt;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

fn permission(read_only: bool) -> Permissions {
    Permissions {
        read_only,
        allow_write: true,
        allow_shell: true,
    }
}
fn workspace(root: &std::path::Path, server: Server, approved: bool) -> Workspace {
    let mut ws = Workspace::new(root).unwrap();
    ws.mcp = Hub::new(
        ws.root.clone(),
        BTreeMap::from([("test".into(), server)]),
        approved,
    );
    ws
}
fn stdio(version: &str, log: &std::path::Path) -> Server {
    Server {
        command: Some("node".into()),
        args: vec![
            format!(
                "{}/tests/fixtures/mcp-stdio.cjs",
                env!("CARGO_MANIFEST_DIR")
            ),
            version.into(),
            log.to_string_lossy().into_owned(),
        ],
        env: BTreeMap::from([
            ("YCODE_MCP_CONFIGURED".into(), "explicit-value".into()),
            ("MCP_TEST_SECRET".into(), "fixture-private-mcp-token".into()),
            ("YCODE_MCP_NUMERIC".into(), "1".into()),
        ]),
        read_only_tools: vec!["echo".into(), "roots".into()],
        allow_read_resources: true,
        allow_prompts: true,
        ..Default::default()
    }
}
#[tokio::test]
async fn stdio_older_and_current_protocols_tools_resources_prompts_roots_and_environment() {
    for version in [
        "2024-11-05",
        "2025-03-26",
        "2025-06-18",
        "2025-11-25",
        "2026-07-28",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let log = temp.path().join("requests.jsonl");
        let ws = workspace(temp.path(), stdio(version, &log), true);
        let check = ws
            .mcp
            .request(&permission(true), "test", "check", &json!({}))
            .await
            .unwrap();
        assert_eq!(check["peer"]["protocolVersion"], version);
        assert_eq!(check["catalogs"]["tools"].as_array().unwrap().len(), 3);
        assert_eq!(check["catalogs"]["templates"][0]["name"], "template");
        let output = tools::execute(
            &ws,
            &permission(true),
            "mcp_call",
            &json!({"server":"test","tool":"echo","arguments":{"text":"MCP_ECHO"}}),
        )
        .await
        .unwrap();
        assert!(output.contains("MCP_ECHO"));
        assert!(output.contains("[REDACTED]"));
        assert!(output.contains("explicit-value"));
        assert!(!output.contains("fixture-private-mcp-token"));
        assert_eq!(
            serde_json::from_str::<Value>(&output).unwrap()["structuredContent"]["value"],
            42
        );
        assert!(output.contains("secretInherited\\\":false"));
        assert!(output.contains("structuredContent"));
        assert!(tools::execute(
            &ws,
            &permission(true),
            "mcp_call",
            &json!({"server":"test","tool":"mutate"})
        )
        .await
        .is_err());
        let roots = tools::execute(
            &ws,
            &permission(true),
            "mcp_call",
            &json!({"server":"test","tool":"roots"}),
        )
        .await
        .unwrap();
        assert!(roots.contains("file:///"));
        assert!(tools::execute(
            &ws,
            &permission(true),
            "mcp_read_resource",
            &json!({"server":"test","uri":"test://guide"})
        )
        .await
        .unwrap()
        .contains("REFERENCE_FROM_MCP"));
        assert!(tools::execute(
            &ws,
            &permission(true),
            "mcp_get_prompt",
            &json!({"server":"test","prompt":"review","arguments":{"feature":"cache"}})
        )
        .await
        .unwrap()
        .contains("Review cache"));
        let calls = std::fs::read_to_string(log).unwrap();
        assert!(!calls.contains("\"name\":\"mutate\""));
    }
}
#[tokio::test]
async fn mcp_does_not_inherit_file_or_shell_approval_and_read_only_blocks_before_connect() {
    let temp = tempfile::tempdir().unwrap();
    let marker = temp.path().join("must-not-exist");
    let ws = workspace(temp.path(), stdio("2025-11-25", &marker), false);
    assert!(tools::execute(
        &ws,
        &permission(false),
        "mcp_call",
        &json!({"server":"test","tool":"echo"})
    )
    .await
    .unwrap_err()
    .to_string()
    .contains("permission denied"));
    assert!(!marker.exists());
    let ws = workspace(temp.path(), stdio("2025-11-25", &marker), true);
    assert!(tools::execute(
        &ws,
        &permission(true),
        "mcp_call",
        &json!({"server":"test","tool":"mutate"})
    )
    .await
    .is_err());
    assert!(!marker.exists());
    assert!(!serde_json::to_string(&tools::definitions_for(
        &Workspace::new(temp.path()).unwrap(),
        false
    ))
    .unwrap()
    .contains("mcp_call"));
}
#[test]
fn configuration_precedence_disabled_servers_and_secret_safe_diagnostics() {
    let temp = tempfile::tempdir().unwrap();
    let ws = Workspace::new(temp.path()).unwrap();
    let config = temp.path().join("global.json");
    std::fs::write(
        &config,
        r#"{"mcp_servers":{"shared":{"command":"node","args":["global"]}}}"#,
    )
    .unwrap();
    std::fs::write(
        temp.path().join("ycode.config.json"),
        r#"{"mcpServers":{"shared":{"command":"node","args":["project"]}}}"#,
    )
    .unwrap();
    std::fs::write(temp.path().join(".mcp.json"),r#"{"mcpServers":{"shared":{"command":"node","args":["local"],"env":{"TOKEN":"hidden-test-value"}}}}"#).unwrap();
    let matches = crate::cli()
        .try_get_matches_from(["ycode", "--config", config.to_str().unwrap()])
        .unwrap();
    let settings = crate::settings::Settings::load(&ws, &matches).unwrap();
    assert!(ws.read(".mcp.json").is_err());
    assert!(ws.read("ycode.config.json").is_err());
    assert_eq!(settings.mcp_servers["shared"].args, vec!["local"]);
    let hub = Hub::new(ws.root.clone(), settings.mcp_servers, false);
    assert!(!hub.describe().to_string().contains("hidden-test-value"));
    for url in [
        "http://remote.example/mcp",
        "https://user:secret@example.com/mcp",
        "https://example.com/mcp?key=secret",
        "file:///mcp",
    ] {
        assert!(crate::mcp::validate_url(url).is_err());
    }
    assert!(crate::mcp::validate_url("http://127.0.0.1:1234/mcp").is_ok());
    let bad = serde_json::from_value::<Server>(
        json!({"url":"https://example.com/mcp","headers_env":{"Host":"TOKEN"}}),
    )
    .unwrap();
    assert!(bad.validate().is_err());
    let disabled = workspace(
        temp.path(),
        Server {
            command: Some("not-a-command".into()),
            disabled: true,
            ..Default::default()
        },
        false,
    );
    assert!(!disabled.mcp.configured());
}

#[derive(Clone, Copy)]
struct Mode {
    modern: bool,
    sse: bool,
    stateful: bool,
    repeated: bool,
    status: u16,
}
struct Mock {
    url: String,
    requests: Arc<Mutex<Vec<Value>>>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Mock {
    fn drop(&mut self) {
        self.task.abort();
    }
}
async fn request(socket: &mut TcpStream) -> (String, Value) {
    let mut bytes = Vec::new();
    let mut buf = [0; 4096];
    let (start, len) = loop {
        let n = socket.read(&mut buf).await.unwrap();
        assert!(n > 0);
        bytes.extend_from_slice(&buf[..n]);
        if let Some(pos) = bytes.windows(4).position(|s| s == b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&bytes[..pos]);
            let len = head
                .lines()
                .find_map(|s| {
                    let (k, v) = s.split_once(':')?;
                    if k.eq_ignore_ascii_case("content-length") {
                        v.trim().parse().ok()
                    } else {
                        None
                    }
                })
                .unwrap_or(0);
            break (pos + 4, len);
        }
    };
    while bytes.len() < start + len {
        let n = socket.read(&mut buf).await.unwrap();
        assert!(n > 0);
        bytes.extend_from_slice(&buf[..n]);
    }
    (
        String::from_utf8_lossy(&bytes[..start]).into_owned(),
        if len > 0 {
            serde_json::from_slice(&bytes[start..start + len]).unwrap()
        } else {
            Value::Null
        },
    )
}
async fn reply(socket: &mut TcpStream, status: u16, content: &str, extra: &str, body: &str) {
    socket.write_all(format!("HTTP/1.1 {status} Test\r\nContent-Type: {content}\r\nContent-Length: {}\r\nConnection: close\r\n{extra}\r\n{body}",body.len()).as_bytes()).await.unwrap();
}
async fn mock(mode: Mode) -> Mock {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/mcp", listener.local_addr().unwrap());
    let requests = Arc::new(Mutex::new(Vec::new()));
    let observed = requests.clone();
    let task = tokio::spawn(async move {
        loop {
            let (mut socket, _) = listener.accept().await.unwrap();
            let observed = observed.clone();
            tokio::spawn(async move {
                let (head, req) = request(&mut socket).await;
                observed
                    .lock()
                    .unwrap()
                    .push(json!({"headers":head,"rpc":req}));
                if !head.starts_with("POST") {
                    reply(&mut socket, 405, "text/plain", "", "").await;
                    return;
                }
                let method = req["method"].as_str().unwrap_or("");
                if mode.status != 200 && (mode.status != 404 || method == "tools/call") {
                    reply(&mut socket, mode.status, "text/plain", "", "denied").await;
                    return;
                }
                if method == "notifications/initialized" || method == "notifications/cancelled" {
                    reply(&mut socket, 202, "text/plain", "", "").await;
                    return;
                }
                if method == "server/discover" && !mode.modern {
                    reply(&mut socket, 400, "text/plain", "", "initialize first").await;
                    return;
                }
                let capabilities = json!({"tools":{},"resources":{},"prompts":{}});
                let result = match method {
                    "server/discover" => {
                        json!({"resultType":"complete","supportedVersions":["2026-07-28"],"capabilities":capabilities,"ttlMs":0,"cacheScope":"private"})
                    }
                    "initialize" => {
                        json!({"protocolVersion":"2025-11-25","capabilities":capabilities,"serverInfo":{"name":"http-fixture","version":"1"}})
                    }
                    "tools/list" => {
                        if req["params"]["cursor"].is_null() || mode.repeated {
                            json!({"tools":[{"name":"first","inputSchema":{"type":"object"}}],"nextCursor":"page2"})
                        } else {
                            json!({"tools":[{"name":"echo","inputSchema":{"type":"object","additionalProperties":true}}]})
                        }
                    }
                    "resources/list" => json!({"resources":[]}),
                    "resources/templates/list" => json!({"resourceTemplates":[]}),
                    "prompts/list" => json!({"prompts":[]}),
                    "tools/call" => {
                        json!({"content":[{"type":"text","text":"HTTP_TOOL_RESULT"},{"type":"image","data":"AQID","mimeType":"image/png"},{"type":"audio","data":"AQID","mimeType":"audio/wav"},{"type":"resource_link","uri":"test://linked","name":"linked"},{"type":"resource","resource":{"uri":"test://embedded","text":"embedded"}}],"structuredContent":{"answer":42},"isError":false})
                    }
                    _ => json!({}),
                };
                let body = json!({"jsonrpc":"2.0","id":req["id"],"result":result}).to_string();
                let extra = if mode.stateful && method == "initialize" {
                    "Mcp-Session-Id: test-session\r\n"
                } else {
                    ""
                };
                if mode.sse {
                    reply(
                        &mut socket,
                        200,
                        "text/event-stream",
                        extra,
                        &format!(": keepalive\r\n\r\nevent: message\r\ndata: {body}\r\n\r\n"),
                    )
                    .await;
                } else {
                    reply(&mut socket, 200, "application/json", extra, &body).await;
                }
            });
        }
    });
    Mock {
        url,
        requests,
        task,
    }
}
#[tokio::test]
async fn streamable_http_json_sse_stateless_stateful_and_current_protocol_are_interoperable() {
    for mode in [
        Mode {
            modern: false,
            sse: false,
            stateful: true,
            repeated: false,
            status: 200,
        },
        Mode {
            modern: false,
            sse: true,
            stateful: false,
            repeated: false,
            status: 200,
        },
        Mode {
            modern: true,
            sse: false,
            stateful: false,
            repeated: false,
            status: 200,
        },
        Mode {
            modern: true,
            sse: true,
            stateful: false,
            repeated: false,
            status: 200,
        },
    ] {
        let server = mock(mode).await;
        let temp = tempfile::tempdir().unwrap();
        let ws = workspace(
            temp.path(),
            Server {
                url: Some(server.url.clone()),
                read_only_tools: vec!["echo".into()],
                timeout_secs: Some(10),
                ..Default::default()
            },
            true,
        );
        let check = ws
            .mcp
            .request(&permission(true), "test", "check", &json!({}))
            .await
            .unwrap();
        assert_eq!(check["catalogs"]["tools"].as_array().unwrap().len(), 2);
        let out = tools::execute(
            &ws,
            &permission(true),
            "mcp_call",
            &json!({"server":"test","tool":"echo","arguments":{"custom":[1,2]}}),
        )
        .await
        .unwrap();
        for text in [
            "HTTP_TOOL_RESULT",
            "structuredContent",
            "image/png",
            "audio/wav",
            "resource_link",
            "embedded",
        ] {
            assert!(out.contains(text));
        }
        let requests = server.requests.lock().unwrap();
        let call = requests
            .iter()
            .find(|r| r["rpc"]["method"] == "tools/call")
            .unwrap();
        assert_eq!(call["rpc"]["params"]["arguments"]["custom"], json!([1, 2]));
        if mode.stateful {
            assert!(call["headers"]
                .as_str()
                .unwrap()
                .to_lowercase()
                .contains("mcp-session-id: test-session"));
        }
        if mode.modern {
            assert!(call["headers"]
                .as_str()
                .unwrap()
                .to_lowercase()
                .contains("mcp-method: tools/call"));
        }
    }
}
#[tokio::test]
async fn authentication_and_repeated_pagination_fail_with_actionable_errors() {
    for status in [401, 403] {
        let server = mock(Mode {
            modern: false,
            sse: false,
            stateful: false,
            repeated: false,
            status,
        })
        .await;
        let temp = tempfile::tempdir().unwrap();
        let ws = workspace(
            temp.path(),
            Server {
                url: Some(server.url.clone()),
                timeout_secs: Some(5),
                ..Default::default()
            },
            true,
        );
        let error = ws
            .mcp
            .request(&permission(true), "test", "check", &json!({}))
            .await
            .unwrap_err()
            .to_string();
        assert!(
            error.contains(if status == 401 {
                "authentication required"
            } else {
                "access denied"
            }),
            "{error}"
        );
    }
    let server = mock(Mode {
        modern: true,
        sse: false,
        stateful: false,
        repeated: true,
        status: 200,
    })
    .await;
    let temp = tempfile::tempdir().unwrap();
    let ws = workspace(
        temp.path(),
        Server {
            url: Some(server.url.clone()),
            ..Default::default()
        },
        true,
    );
    assert!(ws
        .mcp
        .request(&permission(true), "test", "check", &json!({}))
        .await
        .unwrap_err()
        .to_string()
        .contains("repeated"));
}
#[tokio::test]
async fn bounded_stdio_rejects_unterminated_oversized_frames() {
    let source = std::io::Cursor::new(vec![b'x'; crate::mcp_transport::MAX_WIRE + 1]);
    let mut reader = crate::mcp_transport::BoundedRead {
        inner: source,
        size: 0,
    };
    let mut output = Vec::new();
    assert!(reader.read_to_end(&mut output).await.is_err());
}

#[tokio::test]
async fn expired_http_session_does_not_replay_a_tool_action() {
    let server = mock(Mode {
        modern: false,
        sse: false,
        stateful: true,
        repeated: false,
        status: 404,
    })
    .await;
    let temp = tempfile::tempdir().unwrap();
    let ws = workspace(
        temp.path(),
        Server {
            url: Some(server.url.clone()),
            ..Default::default()
        },
        true,
    );
    let error = ws
        .mcp
        .request(
            &permission(false),
            "test",
            "call",
            &json!({"tool":"mutating-action"}),
        )
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("expired"), "{error}");
    let requests = server.requests.lock().unwrap();
    assert_eq!(
        requests
            .iter()
            .filter(|r| r["rpc"]["method"] == "tools/call")
            .count(),
        1
    );
    assert_eq!(
        requests
            .iter()
            .filter(|r| r["rpc"]["method"] == "initialize")
            .count(),
        1
    );
}

#[tokio::test]
async fn agent_discovers_and_calls_mcp_in_saved_design_and_plan_mode() {
    let temp = tempfile::tempdir().unwrap();
    let ws = workspace(
        temp.path(),
        stdio("2025-11-25", &temp.path().join("rpc.jsonl")),
        true,
    );
    crate::design::DesignContext::capture(&ws, "test/model", "Saved project design".into())
        .unwrap()
        .save(&ws)
        .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/chat", listener.local_addr().unwrap());
    let provider = tokio::spawn(async move {
        let mut observed = Vec::new();
        for step in 0..3 {
            let (mut socket, _) = listener.accept().await.unwrap();
            let (_, req) = request(&mut socket).await;
            observed.push(req);
            let message = if step < 2 {
                let (name, args) = if step == 0 {
                    ("mcp_list", json!({"server":"test"}))
                } else {
                    (
                        "mcp_call",
                        json!({"server":"test","tool":"echo","arguments":{"text":"AGENT_MCP_RESULT"}}),
                    )
                };
                json!({"role":"assistant","content":null,"tool_calls":[{"id":format!("mcp-{step}"),"type":"function","function":{"name":name,"arguments":args.to_string()}}]})
            } else {
                json!({"role":"assistant","content":"MCP plan complete"})
            };
            reply(&mut socket,200,"application/json","",&json!({"choices":[{"message":message,"finish_reason":if step<2 {"tool_calls"}else{"stop"}}]}).to_string()).await;
        }
        observed
    });
    let client = y_gateway::OpenRouterClient::with_endpoint(
        y_gateway::OpenRouterConfig {
            api_key: "offline-mcp-provider-key".into(),
            model: "test/model".into(),
            referer: String::new(),
            title: "test".into(),
            timeout_secs: 15,
            max_tokens: 1024,
        },
        &endpoint,
    )
    .unwrap();
    let mut session = crate::session::Session::new(&ws, "test/model").unwrap();
    session.plan_mode = true;
    session.sudo_mode = true;
    let output = crate::agent::turn(
        &client,
        &ws,
        &mut session,
        &permission(true),
        "Plan using MCP",
        4,
    )
    .await
    .unwrap();
    assert_eq!(output, "MCP plan complete");
    let observed = provider.await.unwrap();
    assert!(observed[0]["tools"].to_string().contains("mcp_call"));
    assert!(observed[1]["messages"]
        .to_string()
        .contains("Echo in this workspace"));
    assert!(observed[2]["messages"]
        .to_string()
        .contains("AGENT_MCP_RESULT"));
    assert!(observed[2]["messages"]
        .to_string()
        .contains("Saved project design"));
}

#[tokio::test]
async fn worker_mcp_uses_its_checkout_and_workspace_argument_substitution() {
    let temp = tempfile::tempdir().unwrap();
    let log = temp.path().join("rpc.jsonl");
    let mut server = stdio("2025-11-25", &log);
    server
        .env
        .insert("YCODE_MCP_CONFIGURED".into(), "${YCODE_WORKSPACE}".into());
    let ws = workspace(temp.path(), server, true);
    for args in [
        &["init"][..],
        &["config", "user.name", "MCP tests"],
        &["config", "user.email", "mcp-tests@example.invalid"],
    ] {
        crate::team_git::git(&ws.root, args, None, None)
            .await
            .unwrap();
    }
    std::fs::write(ws.root.join("source.txt"), "initial").unwrap();
    crate::team_git::git(&ws.root, &["add", "source.txt"], None, None)
        .await
        .unwrap();
    crate::team_git::git(&ws.root, &["commit", "-m", "fixture"], None, None)
        .await
        .unwrap();
    let parent = crate::team_git::head(&ws).await.unwrap();
    let checkout = temp.path().join("worker");
    let worker = crate::team_git::worktree(&ws, &checkout, &parent)
        .await
        .unwrap();
    assert!(!Arc::ptr_eq(&ws.mcp, &worker.mcp));
    assert_eq!(worker.mcp.root, worker.root);
    let result = worker
        .mcp
        .request(
            &permission(true),
            "test",
            "call",
            &json!({"tool":"echo","arguments":{"text":"CHECKOUT"}}),
        )
        .await
        .unwrap();
    let output: Value =
        serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(
        std::path::Path::new(output["cwd"].as_str().unwrap())
            .canonicalize()
            .unwrap(),
        worker.root
    );
    assert_eq!(
        std::path::Path::new(output["configured"].as_str().unwrap())
            .canonicalize()
            .unwrap(),
        worker.root
    );
}

#[tokio::test]
async fn http_body_and_unterminated_sse_events_are_bounded_before_parsing() {
    for sse in [false, true] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/mcp", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            request(&mut socket).await;
            let huge = "x".repeat(crate::mcp_transport::MAX_WIRE + 1);
            let body = if sse { format!("data: {huge}") } else { huge };
            reply(
                &mut socket,
                200,
                if sse {
                    "text/event-stream"
                } else {
                    "application/json"
                },
                "",
                &body,
            )
            .await;
        });
        let response = reqwest::Client::new().get(url).send().await.unwrap();
        if sse {
            let mut events = crate::mcp_transport::events(response);
            assert!(events.next().await.unwrap().is_err());
        } else {
            assert!(crate::mcp_transport::body(response)
                .await
                .unwrap_err()
                .to_string()
                .contains("1 MiB"));
        }
        server.await.unwrap();
    }
}

#[tokio::test]
async fn startup_timeout_kills_process_and_never_replays() {
    let temp = tempfile::tempdir().unwrap();
    let script = temp.path().join("timeout.cjs");
    let marker = temp.path().join("started");
    std::fs::write(&script,"require('node:fs').appendFileSync(process.argv[2], 'started\\n'); setInterval(()=>{},1000);").unwrap();
    let ws = workspace(
        temp.path(),
        Server {
            command: Some("node".into()),
            args: vec![
                script.to_string_lossy().into_owned(),
                marker.to_string_lossy().into_owned(),
            ],
            timeout_secs: Some(5),
            ..Default::default()
        },
        true,
    );
    let error = ws
        .mcp
        .request(&permission(true), "test", "check", &json!({}))
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("startup timed out"));
    assert_eq!(std::fs::read_to_string(marker).unwrap(), "started\n");
}
