use crate::{agent, design, session, tools};
use serde_json::{json, Value};
use std::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use y_gateway::{OpenRouterClient, OpenRouterConfig};

pub(crate) async fn provider(
    messages: Vec<Value>,
) -> (OpenRouterClient, tokio::task::JoinHandle<Vec<Value>>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let cfg = OpenRouterConfig {
        api_key: "offline-mode-test-key".into(),
        model: "test/model".into(),
        referer: "http://localhost".into(),
        title: "test".into(),
        timeout_secs: 5,
        max_tokens: 2048,
    };
    let client = OpenRouterClient::with_endpoint(
        cfg,
        &format!("http://{}/chat", listener.local_addr().unwrap()),
    )
    .unwrap();
    let task = tokio::spawn(async move {
        let mut requests = Vec::new();
        for message in messages {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = Vec::new();
            let mut buf = [0; 4096];
            let (end, length) = loop {
                let n = socket.read(&mut buf).await.unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buf[..n]);
                if let Some(pos) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&bytes[..pos]);
                    let length = headers
                        .lines()
                        .find_map(|line| {
                            let (key, value) = line.split_once(':')?;
                            if key.eq_ignore_ascii_case("content-length") {
                                value.trim().parse::<usize>().ok()
                            } else {
                                None
                            }
                        })
                        .unwrap();
                    break (pos + 4, length);
                }
            };
            while bytes.len() < end + length {
                let n = socket.read(&mut buf).await.unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buf[..n]);
            }
            requests.push(serde_json::from_slice(&bytes[end..end + length]).unwrap());
            let body = json!({"choices":[{"message":message,"finish_reason":"stop"}]}).to_string();
            socket
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
        }
        requests
    });
    (client, task)
}

fn reply(text: &str) -> Value {
    json!({"role":"assistant","content":text})
}

#[tokio::test]
async fn failed_source_reads_do_not_create_an_unobserved_design() {
    let temp = tempfile::tempdir().unwrap();
    let ws = tools::Workspace::new(temp.path()).unwrap();
    let mut session = session::Session::new(&ws, "test/model").unwrap();
    session.sudo_mode = true;
    session.plan_mode = true;
    let (client, server) = provider(vec![
        call("missing", "read_file", json!({"path":"missing.rs"})),
        reply("The requested source was not found."),
    ])
    .await;
    agent::turn(
        &client,
        &ws,
        &mut session,
        &permissions(),
        "Inspect missing.rs",
        3,
    )
    .await
    .unwrap();
    assert!(design::DesignContext::optional(&ws).unwrap().is_none());
    assert_eq!(server.await.unwrap().len(), 2);
}
fn call(id: &str, name: &str, args: Value) -> Value {
    json!({"role":"assistant","content":null,"tool_calls":[{"id":id,"type":"function","function":{"name":name,"arguments":args.to_string()}}]})
}
fn permissions() -> tools::Permissions {
    tools::Permissions {
        read_only: false,
        allow_write: true,
        allow_shell: true,
    }
}

#[tokio::test]
async fn plan_mode_enforces_read_only_even_when_write_and_shell_are_preapproved() {
    let temp = tempfile::tempdir().unwrap();
    let ws = tools::Workspace::new(temp.path()).unwrap();
    let (client, server) = provider(vec![
        call(
            "bad",
            "write_file",
            json!({"path":"bad.txt","content":"should never exist"}),
        ),
        reply("1. Inspect source.\n2. Propose a change.\n3. Verify with tests."),
    ])
    .await;
    let mut session = session::Session::new(&ws, "test/model").unwrap();
    session.plan_mode = true;
    let text = agent::turn(&client, &ws, &mut session, &permissions(), "Plan a fix", 3)
        .await
        .unwrap();
    assert!(!temp.path().join("bad.txt").exists());
    let plan =
        fs::read_to_string(temp.path().join(format!(".ycode/plans/{}.md", session.id))).unwrap();
    assert!(plan.contains(&text));
    assert!(plan.contains("Plan a fix"));
    let requests = server.await.unwrap();
    for request in &requests {
        assert!(request["tools"].as_array().unwrap().iter().all(|t| ![
            "write_file",
            "edit_file",
            "run_command"
        ]
        .contains(&t["function"]["name"].as_str().unwrap())));
        assert!(request["messages"]
            .to_string()
            .contains("Current mode: PLAN"));
    }
    assert!(requests[1]["messages"]
        .to_string()
        .contains("unavailable tool write_file"));
}

#[tokio::test]
async fn generating_design_uses_read_only_tools_and_never_overwrites_last_session() {
    let temp = tempfile::tempdir().unwrap();
    let ws = tools::Workspace::new(temp.path()).unwrap();
    fs::write(
        temp.path().join("main.rs"),
        "fn main() { println!(\"hello\"); }",
    )
    .unwrap();
    let current = session::Session::new(&ws, "test/model").unwrap();
    current.save(&ws).unwrap();
    let (client, server) = provider(vec![
        call("read", "read_file", json!({"path":"main.rs"})),
        reply("# Project design\nEntry: main.rs\nPseudocode: start -> print greeting -> exit."),
    ])
    .await;
    agent::refresh_design(&client, &ws, "test/model", 3)
        .await
        .unwrap();
    let context = design::DesignContext::load(&ws).unwrap();
    assert!(context.summary.contains("Pseudocode"));
    assert!(context.changes(&ws).unwrap().is_empty());
    assert!(fs::read_to_string(temp.path().join(".ycode/design.md"))
        .unwrap()
        .contains("main.rs"));
    assert_eq!(session::Session::load(&ws, "last").unwrap().id, current.id);
    assert!(!temp.path().join(".ycode/plans").exists());
    let requests = server.await.unwrap();
    assert!(
        requests[1]["messages"]
            .to_string()
            .contains("print greeting")
            || requests[1]["messages"].to_string().contains("println!")
    );
    assert!(requests[0]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .all(|t| t["function"]["name"] != "run_command" && t["function"]["name"] != "write_file"));
}

#[tokio::test]
async fn sudo_mode_sends_saved_design_and_targeted_current_reads_not_old_source() {
    let temp = tempfile::tempdir().unwrap();
    let ws = tools::Workspace::new(temp.path()).unwrap();
    fs::write(
        temp.path().join("source.rs"),
        "CURRENT_SOURCE_FOR_THIS_ISSUE",
    )
    .unwrap();
    design::DesignContext::capture(
        &ws,
        "test/model",
        "Architecture: request -> parser -> reply".into(),
    )
    .unwrap()
    .save(&ws)
    .unwrap();
    let mut session = session::Session::new(&ws, "test/model").unwrap();
    session.messages.extend([json!({"role":"user","content":"Earlier issue"}), call("old-read", "read_file", json!({"path":"old.rs"})), json!({"role":"tool","tool_call_id":"old-read","content":"OLD_WHOLE_SOURCE_MUST_NOT_BE_SENT"}), reply("OLD_ASSISTANT_SOURCE_MUST_NOT_BE_SENT")]);
    session.sudo_mode = true;
    design::save_plan(
        &ws,
        &session.id,
        "Plan parser repair",
        "SAVED_PLAN_TARGET_PARSER",
    )
    .unwrap();
    let (client, server) = provider(vec![
        call("current-read", "read_file", json!({"path":"source.rs"})),
        reply("Use a targeted parser fix."),
        reply("Architecture: request -> parser -> reply; source.rs inspected"),
    ])
    .await;
    agent::turn(
        &client,
        &ws,
        &mut session,
        &permissions(),
        "Investigate this parser issue",
        3,
    )
    .await
    .unwrap();
    let requests = server.await.unwrap();
    for request in &requests[..2] {
        let wire = request["messages"].to_string();
        assert!(wire.contains("Architecture: request"));
        assert!(wire.contains("SAVED_PLAN_TARGET_PARSER"));
        assert!(!wire.contains("OLD_WHOLE_SOURCE"));
        assert!(!wire.contains("OLD_ASSISTANT_SOURCE"));
    }
    assert!(requests[1]["messages"]
        .to_string()
        .contains("CURRENT_SOURCE_FOR_THIS_ISSUE"));
    assert_eq!(requests.len(), 3);
    assert!(requests[2]["messages"]
        .to_string()
        .contains("CURRENT_SOURCE_FOR_THIS_ISSUE"));
    assert!(!requests[2]["messages"]
        .to_string()
        .contains("OLD_WHOLE_SOURCE"));
    assert!(session.messages.iter().any(|m| m["content"]
        .as_str()
        .is_some_and(|t| t.contains("OLD_WHOLE_SOURCE"))));
    session.sudo_mode = false;
    assert!(
        design::request_messages(&ws, &session, session.messages.len() - 1)
            .unwrap()
            .iter()
            .any(|m| m["content"]
                .as_str()
                .is_some_and(|t| t.contains("OLD_WHOLE_SOURCE")))
    );
}

#[tokio::test]
async fn first_prompt_builds_design_after_targeted_inspection_and_later_reads_extend_it() {
    let temp = tempfile::tempdir().unwrap();
    let ws = tools::Workspace::new(temp.path()).unwrap();
    fs::write(temp.path().join("parser.rs"), "SCOPED_PARSER_SOURCE").unwrap();
    fs::write(temp.path().join("other.rs"), "UNRELATED_SOURCE_NOT_SENT").unwrap();
    let mut session = session::Session::new(&ws, "test/model").unwrap();
    session.plan_mode = true;
    session.sudo_mode = true;
    let (client, server) = provider(vec![
        reply("Hello; describe your project task."),
        call("read-parser", "read_file", json!({"path":"parser.rs"})),
        reply("Plan a parser repair."),
        reply("Observed parser design in parser.rs. Other areas uninspected."),
        call("read-other", "read_file", json!({"path":"other.rs"})),
        reply("Plan a second feature."),
        reply("Merged parser.rs and other.rs architecture."),
    ])
    .await;
    agent::turn(&client, &ws, &mut session, &permissions(), "Hello", 3)
        .await
        .unwrap();
    assert!(design::DesignContext::optional(&ws).unwrap().is_none());
    agent::turn(
        &client,
        &ws,
        &mut session,
        &permissions(),
        "Investigate parser.rs",
        3,
    )
    .await
    .unwrap();
    assert!(design::DesignContext::load(&ws)
        .unwrap()
        .summary
        .contains("Other areas uninspected"));
    agent::turn(
        &client,
        &ws,
        &mut session,
        &permissions(),
        "Now inspect other.rs",
        3,
    )
    .await
    .unwrap();
    assert!(fs::read_to_string(temp.path().join(".ycode/design.md"))
        .unwrap()
        .contains("Merged"));
    let requests = server.await.unwrap();
    assert_eq!(requests.len(), 7);
    assert!(requests[1]["messages"]
        .to_string()
        .contains("Investigate parser.rs"));
    assert!(!requests[1]["messages"]
        .to_string()
        .contains("Create a concise project architecture"));
    assert!(requests[3]["messages"]
        .to_string()
        .contains("SCOPED_PARSER_SOURCE"));
    assert!(!requests[3]["messages"]
        .to_string()
        .contains("UNRELATED_SOURCE_NOT_SENT"));
    assert!(requests[6]["messages"]
        .to_string()
        .contains("Observed parser design"));
    assert!(requests[6]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .all(|tool| !["write_file", "edit_file", "run_command"]
            .contains(&tool["function"]["name"].as_str().unwrap())));
}

#[test]
fn design_detects_modified_added_deleted_files_and_reports_staleness() {
    let temp = tempfile::tempdir().unwrap();
    let ws = tools::Workspace::new(temp.path()).unwrap();
    fs::write(temp.path().join("change.rs"), "old").unwrap();
    fs::write(temp.path().join("delete.rs"), "old").unwrap();
    let context =
        design::DesignContext::capture(&ws, "test/model", "Observed module map".into()).unwrap();
    context.save(&ws).unwrap();
    fs::write(temp.path().join("change.rs"), "new and longer").unwrap();
    fs::write(temp.path().join("added.rs"), "new").unwrap();
    fs::remove_file(temp.path().join("delete.rs")).unwrap();
    assert_eq!(
        context.changes(&ws).unwrap(),
        ["added.rs", "change.rs", "delete.rs"]
    );
    assert!(context.message(&ws).unwrap()["content"]
        .as_str()
        .unwrap()
        .contains("may be stale"));
}

#[test]
fn modes_survive_resume_and_legacy_sessions_default_to_normal() {
    let temp = tempfile::tempdir().unwrap();
    let ws = tools::Workspace::new(temp.path()).unwrap();
    let mut session = session::Session::new(&ws, "test/model").unwrap();
    session.plan_mode = true;
    session.sudo_mode = true;
    session.save(&ws).unwrap();
    let loaded = session::Session::load(&ws, "last").unwrap();
    assert!(loaded.plan_mode && loaded.sudo_mode);
    let mut old = serde_json::to_value(&session).unwrap();
    old.as_object_mut().unwrap().remove("plan_mode");
    old.as_object_mut().unwrap().remove("sudo_mode");
    let legacy: session::Session = serde_json::from_value(old).unwrap();
    assert!(!legacy.plan_mode && !legacy.sudo_mode);
}

#[test]
fn design_files_are_bounded_and_private_path_shapes_are_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let ws = tools::Workspace::new(temp.path()).unwrap();
    assert!(design::DesignContext::capture(
        &ws,
        "test/model",
        "x".repeat(design::MAX_DESIGN_BYTES + 1)
    )
    .is_err());
    assert!(design::DesignContext::capture(&ws, "test/model", "".into()).is_err());
    fs::write(temp.path().join(".ycode"), "not a directory").unwrap();
    assert!(design::DesignContext::load(&ws).is_err());
    assert!(design::save_plan(&ws, "../../escape", "task", "plan").is_err());
}

#[tokio::test]
async fn failed_refresh_keeps_previous_design() {
    let temp = tempfile::tempdir().unwrap();
    let ws = tools::Workspace::new(temp.path()).unwrap();
    design::DesignContext::capture(&ws, "test/model", "Existing verified design".into())
        .unwrap()
        .save(&ws)
        .unwrap();
    let (client, server) = provider(vec![reply(&"x".repeat(design::MAX_DESIGN_BYTES + 1))]).await;
    assert!(agent::refresh_design(&client, &ws, "test/model", 2)
        .await
        .is_err());
    assert_eq!(
        design::DesignContext::load(&ws).unwrap().summary,
        "Existing verified design"
    );
    server.await.unwrap();
}

#[test]
fn failed_design_export_preserves_canonical_design() {
    let temp = tempfile::tempdir().unwrap();
    let ws = tools::Workspace::new(temp.path()).unwrap();
    design::DesignContext::capture(&ws, "test/model", "Old design".into())
        .unwrap()
        .save(&ws)
        .unwrap();
    fs::remove_file(temp.path().join(".ycode/design.md")).unwrap();
    fs::create_dir(temp.path().join(".ycode/design.md")).unwrap();
    assert!(
        design::DesignContext::capture(&ws, "test/model", "New design".into())
            .unwrap()
            .save(&ws)
            .is_err()
    );
    assert_eq!(
        design::DesignContext::load(&ws).unwrap().summary,
        "Old design"
    );
}

#[test]
fn command_line_modes_aliases_and_toggles_are_explicit() {
    let matches = crate::cli()
        .try_get_matches_from(["ycode", "--plan", "--design-mode", "Plan caching"])
        .unwrap();
    assert!(matches.get_flag("plan") && matches.get_flag("sudo"));
    assert!(crate::cli()
        .try_get_matches_from(["ycode", "--sudo", "--no-sudo"])
        .is_err());
    assert!(crate::cli()
        .try_get_matches_from(["ycode", "--plan", "--no-plan"])
        .is_err());
    assert!(crate::toggle("", false).unwrap());
    assert!(!crate::toggle("off", true).unwrap());
    assert!(crate::toggle("nonsense", false).is_err());
}

#[test]
fn new_cli_sessions_default_to_planning_and_design_but_resume_respects_settings() {
    let temp = tempfile::tempdir().unwrap();
    let ws = tools::Workspace::new(temp.path()).unwrap();
    let mut session = session::Session::new(&ws, "test/model").unwrap();
    let defaults = crate::cli().try_get_matches_from(["ycode"]).unwrap();
    crate::apply_modes(&defaults, &mut session, true);
    assert!(session.plan_mode && session.sudo_mode);
    let overrides = crate::cli()
        .try_get_matches_from(["ycode", "--no-plan", "--no-sudo"])
        .unwrap();
    crate::apply_modes(&overrides, &mut session, true);
    assert!(!session.plan_mode && !session.sudo_mode);
    crate::apply_modes(&defaults, &mut session, false);
    assert!(!session.plan_mode && !session.sudo_mode);
}

#[tokio::test]
async fn plan_implementation_refreshes_observed_design_and_keeps_session_identity() {
    let temp = tempfile::tempdir().unwrap();
    let ws = tools::Workspace::new(temp.path()).unwrap();
    fs::write(temp.path().join("parser.rs"), "old parser").unwrap();
    design::DesignContext::capture(&ws, "test/model", "Existing parser design".into())
        .unwrap()
        .save(&ws)
        .unwrap();
    let mut session = session::Session::new(&ws, "test/model").unwrap();
    session.plan_mode = true;
    session.sudo_mode = true;
    let id = session.id.clone();
    let (client, server) = provider(vec![
        reply("Change parser.rs to handle invalid input"),
        call("edit", "edit_file", json!({"path":"parser.rs","old_text":"old parser","new_text":"new parser with validation"})),
        reply("Implemented parser validation"),
        reply("Observed architecture: parser.rs validates input before processing"),
    ]).await;
    agent::turn(
        &client,
        &ws,
        &mut session,
        &permissions(),
        "Plan input validation",
        3,
    )
    .await
    .unwrap();
    assert_eq!(ws.read("parser.rs").unwrap(), "old parser");
    assert_eq!(
        design::DesignContext::load(&ws).unwrap().summary,
        "Existing parser design"
    );
    let result = agent::implement(&client, &ws, &mut session, &permissions(), "", 3)
        .await
        .unwrap();
    assert_eq!(result, "Implemented parser validation");
    assert!(!session.plan_mode && session.sudo_mode);
    assert_eq!(session.id, id);
    assert_eq!(ws.read("parser.rs").unwrap(), "new parser with validation");
    let context = design::DesignContext::load(&ws).unwrap();
    assert!(context.summary.contains("validates input"));
    assert!(context.changes(&ws).unwrap().is_empty());
    assert_eq!(session::Session::load(&ws, "last").unwrap().id, id);
    let requests = server.await.unwrap();
    assert!(requests[1]["messages"]
        .to_string()
        .contains("Change parser.rs"));
    assert!(requests[3]["messages"]
        .to_string()
        .contains("Existing parser design"));
    assert!(requests[3]["messages"]
        .to_string()
        .contains("Implemented parser validation"));
    assert!(requests[3]["tools"].as_array().unwrap().iter().all(|t| ![
        "edit_file",
        "write_file",
        "run_command"
    ]
    .contains(&t["function"]["name"].as_str().unwrap())));
}

#[tokio::test]
async fn failed_automatic_refresh_preserves_completed_work_and_previous_context() {
    let temp = tempfile::tempdir().unwrap();
    let ws = tools::Workspace::new(temp.path()).unwrap();
    design::DesignContext::capture(&ws, "test/model", "Old architecture".into())
        .unwrap()
        .save(&ws)
        .unwrap();
    let mut session = session::Session::new(&ws, "test/model").unwrap();
    session.sudo_mode = true;
    let (client, server) = provider(vec![
        call(
            "create",
            "write_file",
            json!({"path":"new.rs","content":"actual completed work"}),
        ),
        reply("Completed the requested change"),
        reply(&"x".repeat(design::MAX_DESIGN_BYTES + 1)),
    ])
    .await;
    let result = agent::turn(
        &client,
        &ws,
        &mut session,
        &permissions(),
        "Create new.rs",
        3,
    )
    .await
    .unwrap();
    assert_eq!(result, "Completed the requested change");
    assert_eq!(ws.read("new.rs").unwrap(), "actual completed work");
    assert_eq!(
        design::DesignContext::load(&ws).unwrap().summary,
        "Old architecture"
    );
    assert_eq!(server.await.unwrap().len(), 3);
}

#[tokio::test]
async fn changing_model_preserves_saved_context_and_routes_next_request_to_new_model() {
    let temp = tempfile::tempdir().unwrap();
    let ws = tools::Workspace::new(temp.path()).unwrap();
    design::DesignContext::capture(&ws, "test/model", "Persistent project design".into())
        .unwrap()
        .save(&ws)
        .unwrap();
    let mut session = session::Session::new(&ws, "test/model").unwrap();
    session.sudo_mode = true;
    design::save_plan(&ws, &session.id, "Goal", "Persistent saved plan").unwrap();
    let (mut client, server) =
        provider(vec![reply("First response"), reply("Second response")]).await;
    agent::turn(&client, &ws, &mut session, &permissions(), "First issue", 2)
        .await
        .unwrap();
    let id = session.id.clone();
    let history = session.messages.clone();
    assert!(crate::switch_model(&mut client, &ws, &mut session, "invalid model").is_err());
    assert_eq!(session.model, "test/model");
    crate::switch_model(&mut client, &ws, &mut session, "other/model").unwrap();
    assert_eq!(session.messages, history);
    assert_eq!(session.id, id);
    assert_eq!(
        session::Session::load(&ws, "last").unwrap().model,
        "other/model"
    );
    agent::turn(
        &client,
        &ws,
        &mut session,
        &permissions(),
        "Second issue",
        2,
    )
    .await
    .unwrap();
    let requests = server.await.unwrap();
    assert_eq!(requests[0]["model"], "test/model");
    assert_eq!(requests[1]["model"], "other/model");
    assert!(requests[1]["messages"]
        .to_string()
        .contains("Persistent project design"));
    assert!(requests[1]["messages"]
        .to_string()
        .contains("Persistent saved plan"));
}
