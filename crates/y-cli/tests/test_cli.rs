use std::process::Command;

fn cli() -> Command {
    let mut command = clean_command();
    command.arg("--no-global-config");
    command
}
fn clean_command() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ycode"));
    for name in [
        "OPENROUTER_API_KEY",
        "OPENROUTER_MODEL",
        "OPENROUTER_TIMEOUT_SECS",
        "OPENROUTER_MAX_TOKENS",
        "YCODE_CONFIG",
    ] {
        command.env_remove(name);
    }
    command
}

#[test]
fn mcp_commands_need_no_llm_key_enforce_approval_and_disable_without_launching() {
    let temp = tempfile::tempdir().unwrap();
    let script = format!(
        "{}/tests/fixtures/mcp-stdio.cjs",
        env!("CARGO_MANIFEST_DIR")
    );
    let marker = temp.path().join("rpc.jsonl");
    std::fs::write(temp.path().join(".mcp.json"),serde_json::json!({"mcpServers":{"test":{"command":"node","args":[script,"2025-11-25",marker],"read_only_tools":["echo"]}}}).to_string()).unwrap();
    let output = cli().current_dir(temp.path()).arg("mcp").output().unwrap();
    assert!(output.status.success());
    assert!(!marker.exists());
    let output = cli()
        .current_dir(temp.path())
        .args(["--no-mcp", "mcp"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "[]");
    assert!(!marker.exists());
    let output = cli()
        .current_dir(temp.path())
        .args(["mcp", "check", "test"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("permission denied"));
    assert!(!marker.exists());
    let output = cli()
        .current_dir(temp.path())
        .args([
            "--read-only",
            "--allow-mcp",
            "mcp",
            "call",
            "test",
            "mutate",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(!marker.exists());
    let output = cli()
        .current_dir(temp.path())
        .args(["--allow-mcp", "mcp", "check", "test"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap()["catalogs"]
            ["templates"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let output = cli()
        .current_dir(temp.path())
        .args(["--allow-mcp", "mcp", "call", "test", "mutate"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("\"isError\": true"));
    let output = cli()
        .current_dir(temp.path())
        .args([
            "--allow-mcp",
            "mcp",
            "call",
            "test",
            "echo",
            "--arguments",
            r#"{"text":"CLI_MCP_OK"}"#,
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("CLI_MCP_OK"));
}
#[test]
fn global_config_is_reused_in_different_projects_and_overrides_are_explicit() {
    let global = tempfile::tempdir().unwrap();
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    let path = global.path().join("config.json");
    std::fs::write(&path,r#"{"defaults":{"provider":"local","model":"global-manager","worker_model":"small-worker","plan":false,"workers":2},"providers":{"local":{"url":"http://127.0.0.1:11434/v1/chat/completions","key_env":null}}}"#).unwrap();
    for project in [&first, &second] {
        let output = clean_command()
            .current_dir(project.path())
            .env("YCODE_CONFIG", &path)
            .arg("doctor")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("global-manager"));
    }
    std::fs::write(
        second.path().join("ycode.config.json"),
        r#"{"defaults":{"model":"project-manager"}}"#,
    )
    .unwrap();
    let output = clean_command()
        .current_dir(second.path())
        .args(["--config", path.to_str().unwrap(), "doctor"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("project-manager"));
    let output = clean_command()
        .current_dir(second.path())
        .env("YCODE_CONFIG", &path)
        .args(["--model", "cli-manager", "doctor"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("cli-manager"));
    let output = cli()
        .current_dir(first.path())
        .env("YCODE_CONFIG", &path)
        .arg("doctor")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("OPENROUTER_API_KEY"));
}

#[test]
fn config_init_needs_no_api_key_and_never_overwrites_existing_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings/config.json");
    let output = clean_command()
        .args(["--config", path.to_str().unwrap(), "config", "init"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let before = std::fs::read(&path).unwrap();
    let output = clean_command()
        .args(["--config", path.to_str().unwrap(), "config", "init"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(std::fs::read(&path).unwrap(), before);
    let output = clean_command()
        .args(["--config", path.to_str().unwrap(), "config", "path"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("config.json"));
    std::fs::write(&path, r#"{"defaults":{"allow_shell":true}}"#).unwrap();
    let output = clean_command()
        .args(["--config", path.to_str().unwrap(), "doctor"])
        .output()
        .unwrap();
    assert!(!output.status.success());
}

#[test]
fn skills_cli_discovers_external_packages_from_global_config_without_an_api_key() {
    let global = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let package = global.path().join("reused-skills/review");
    std::fs::create_dir_all(&package).unwrap();
    std::fs::write(package.join("SKILL.md"),"---\nname: review\ndescription: Reusable review workflow\ncontext: fork\n---\nPortable review body").unwrap();
    let config = global.path().join("config.json");
    std::fs::write(
        &config,
        r#"{"skills":{"discover_external":false,"paths":["reused-skills"]}}"#,
    )
    .unwrap();
    let output = clean_command()
        .current_dir(project.path())
        .args(["--config", config.to_str().unwrap(), "skills"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("Reusable review workflow"));
    assert!(!String::from_utf8_lossy(&output.stdout).contains("Portable review body"));
    let output = clean_command()
        .current_dir(project.path())
        .args([
            "--config",
            config.to_str().unwrap(),
            "skills",
            "show",
            "review",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("Portable review body"));
    assert!(String::from_utf8_lossy(&output.stdout).contains("host-specific behavior"));
    let output = clean_command()
        .current_dir(project.path())
        .args([
            "--config",
            config.to_str().unwrap(),
            "--no-skills",
            "skills",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(!String::from_utf8_lossy(&output.stdout).contains("Reusable review workflow"));
}

#[test]
fn executable_loads_dotenv_and_environment_overrides_without_exposing_key() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join(".env"), "OPENROUTER_API_KEY=offline-private-test-key\nOPENROUTER_MODEL=test/dotenv\nOPENROUTER_TIMEOUT_SECS=90\n").unwrap();
    let output = cli()
        .current_dir(temp.path())
        .arg("doctor")
        .output()
        .unwrap();
    assert!(output.status.success());
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("test/dotenv"));
    assert!(text.contains("90s"));
    assert!(!text.contains("offline-private-test-key"));
    let output = cli()
        .current_dir(temp.path())
        .env("OPENROUTER_MODEL", "test/environment")
        .args(["doctor", "--model", "test/cli"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("test/cli"));
}

#[test]
fn missing_key_and_invalid_settings_exit_nonzero() {
    let temp = tempfile::tempdir().unwrap();
    let output = cli()
        .current_dir(temp.path())
        .args(["-p", "hello"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("OPENROUTER_API_KEY"));
    assert!(!temp.path().join(".ycode").exists());
    let output = cli()
        .current_dir(temp.path())
        .env("OPENROUTER_API_KEY", "offline-private-test-key")
        .env("OPENROUTER_TIMEOUT_SECS", "0")
        .arg("doctor")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(!String::from_utf8_lossy(&output.stderr).contains("offline-private-test-key"));
}

#[test]
fn custom_provider_does_not_require_openrouter_and_rejects_secret_configuration() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("ycode.providers.json"),r#"{"local":{"url":"http://127.0.0.1:11434/v1/chat/completions","format":"openai","key_env":null}}"#).unwrap();
    let output = cli()
        .current_dir(temp.path())
        .args(["--provider", "local", "--model", "local-model", "doctor"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("local-model"));
    std::fs::write(temp.path().join("ycode.providers.json"),r#"{"local":{"url":"https://example.com/chat","format":"openai","api_key":"never-save-this-secret"}}"#).unwrap();
    let output = cli()
        .current_dir(temp.path())
        .args(["--provider", "local", "--model", "local-model", "doctor"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(!String::from_utf8_lossy(&output.stderr).contains("never-save-this-secret"));
}
