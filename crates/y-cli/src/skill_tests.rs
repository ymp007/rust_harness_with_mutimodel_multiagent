use crate::{
    skills::Catalog,
    tools::{Permissions, Workspace},
};
use serde_json::json;
use std::{fs, path::Path, sync::Arc};
fn package(root: &Path, folder: &str, text: &str) {
    let dir = root.join(folder);
    fs::create_dir_all(dir.join("references")).unwrap();
    fs::write(dir.join("SKILL.md"), text).unwrap();
    fs::write(
        dir.join("references/guide.md"),
        "Targeted reference content",
    )
    .unwrap();
}
fn workspace(dir: &Path, root: &Path) -> Workspace {
    let mut ws = Workspace::new(dir).unwrap();
    ws.skills = Arc::new(Catalog::discover(&[root.to_path_buf()]));
    ws
}
#[test]
fn external_project_locations_and_positional_arguments_work_without_conversion() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join(".git")).unwrap();
    for (folder, name) in [
        (".agents/skills", "codex"),
        (".claude/skills", "claude"),
        (".agent/skills", "antigravity"),
    ] {
        package(&dir.path().join(folder), name, &format!("---\nname: {name}\ndescription: Portable instructions\n---\n$ARGUMENTS[0]|$0|$1|$10|$ARGUMENTS"));
    }
    let mut ws = Workspace::new(dir.path()).unwrap();
    let roots = crate::skills::roots(&ws, &crate::skills::Settings::default(), std::iter::empty());
    ws.skills = Arc::new(Catalog::discover(&roots));
    for name in ["codex", "claude", "antigravity"] {
        let loaded = ws.skills.load(name, "first second", &[]).unwrap();
        assert!(loaded.contains("first|first|second|$10|first second"));
    }
    let settings = crate::skills::Settings {
        discover_external: Some(false),
        ..Default::default()
    };
    let roots = crate::skills::roots(&ws, &settings, std::iter::empty());
    let catalog = Catalog::discover(&roots);
    assert!(catalog.resolve("codex").is_err());
    assert!(catalog.resolve("claude").is_err());
    assert!(catalog.resolve("antigravity").is_err());
}
#[test]
fn yaml_metadata_and_claude_extensions_have_clear_compatibility_diagnostics() {
    let dir = tempfile::tempdir().unwrap();
    package(dir.path(),"portable","---\r\nname: portable\r\ndescription: >-\r\n  Review source\r\n  with targeted checks.\r\ncontext: fork\r\nallowed-tools: [Read, Bash]\r\nhooks: {}\r\n---\r\nInspect $ARGUMENTS at ${CLAUDE_SKILL_DIR}. !`echo must-not-run`\r\n");
    let ws = workspace(dir.path(), dir.path());
    let s = &ws.skills.entries[0];
    assert!(s.description.contains("targeted checks"));
    assert_eq!(s.diagnostics.len(), 4);
    assert!(ws
        .skills
        .load("portable", "feature", &[])
        .unwrap()
        .contains("Inspect feature"));
    assert!(ws
        .skills
        .load("portable", "feature", &[])
        .unwrap()
        .contains("!`echo must-not-run`"));
}
#[test]
fn discovery_deduplicates_paths_and_prefers_project_names_without_merging_packages() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("project");
    let global = dir.path().join("global");
    package(
        &project,
        "same",
        "---\nname: same\ndescription: Project version\n---\nProject instructions",
    );
    package(
        &global,
        "same",
        "---\nname: same\ndescription: Global version\n---\nGlobal instructions",
    );
    let catalog = Catalog::discover(&[project.clone(), global, project]);
    assert_eq!(catalog.entries.len(), 2);
    assert_eq!(
        catalog.resolve("same").unwrap().description,
        "Project version"
    );
    assert!(catalog
        .load(&catalog.entries[1].id, "", &[])
        .unwrap()
        .contains("Global instructions"));
}
#[test]
fn manual_only_and_model_only_flags_are_enforced_and_explicit_choices_resume() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("skills");
    package(&root,"deploy","---\nname: deploy\ndescription: Deploy manually\ndisable-model-invocation: true\n---\nDeploy $ARGUMENTS");
    package(&root,"background","---\nname: background\ndescription: Background knowledge\nuser-invocable: false\n---\nBackground notes");
    let ws = workspace(dir.path(), &root);
    let mut messages = vec![
        json!({"role":"system","content":"policy"}),
        json!({"role":"user","content":"task"}),
    ];
    crate::skills::augment(&ws, &mut messages).unwrap();
    assert!(!messages[1]["content"]
        .as_str()
        .unwrap()
        .contains("Deploy manually"));
    assert!(ws.skills.load("deploy", "", &[]).is_err());
    assert!(ws.skills.resource("deploy", "SKILL.md", &[]).is_err());
    assert!(crate::skills::activate(&ws, "background", "").is_err());
    crate::skills::prepare(&ws, "/skill deploy staging").unwrap();
    assert!(ws
        .skills
        .load("deploy", "staging", &ws.active_skills.lock().unwrap())
        .unwrap()
        .contains("Deploy staging"));
    let mut session = crate::session::Session::new(&ws, "test/model").unwrap();
    session.active_skills = ws.active_skills.lock().unwrap().clone();
    session.save(&ws).unwrap();
    assert_eq!(
        crate::session::Session::load(&ws, "last")
            .unwrap()
            .active_skills,
        session.active_skills
    );
}
#[tokio::test]
async fn skill_resources_cannot_escape_read_secrets_or_grant_shell_permission() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("skills");
    package(&root,"safe","---\nname: safe\ndescription: Test resource access\nallowed-tools: Bash\n---\nRun commands only when approved");
    let ws = workspace(dir.path(), &root);
    fs::write(root.join("safe/.env"), "private-value").unwrap();
    for path in [
        "../safe/SKILL.md",
        "/etc/passwd",
        "C:\\secret",
        ".env",
        "references/../../secret",
    ] {
        assert!(ws.skills.resource("safe", path, &[]).is_err());
    }
    let permissions = Permissions {
        read_only: true,
        allow_write: false,
        allow_shell: false,
    };
    let text = crate::tools::execute(
        &ws,
        &permissions,
        "read_skill_resource",
        &json!({"name":"safe","path":"references/guide.md"}),
    )
    .await
    .unwrap();
    assert_eq!(text, "Targeted reference content");
    assert!(crate::tools::execute(
        &ws,
        &permissions,
        "run_command",
        &json!({"command":"echo forbidden"})
    )
    .await
    .unwrap()
    .contains("Permission denied"));
}
#[tokio::test]
async fn real_tool_roundtrip_loads_only_selected_skill_and_reads_reference_in_design_mode() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("skills");
    package(
        &root,
        "review",
        "---\nname: review\ndescription: Source review workflow\n---\nUNIQUE_BODY_READ_ON_DEMAND",
    );
    let ws = workspace(dir.path(), &root);
    let responses = vec![
        json!({"role":"assistant","content":null,"tool_calls":[{"id":"skill1","type":"function","function":{"name":"load_skill","arguments":"{\"name\":\"review\"}"}}]}),
        json!({"role":"assistant","content":null,"tool_calls":[{"id":"ref1","type":"function","function":{"name":"read_skill_resource","arguments":"{\"name\":\"review\",\"path\":\"references/guide.md\"}"}}]}),
        json!({"role":"assistant","content":"Review finished"}),
    ];
    let (client, server) = crate::mode_tests::provider(responses).await;
    let mut session = crate::session::Session::new(&ws, "test/model").unwrap();
    session.plan_mode = true;
    session.sudo_mode = true;
    crate::design::DesignContext::capture(&ws, "test/model", "Saved design".into())
        .unwrap()
        .save(&ws)
        .unwrap();
    let output = crate::agent::turn(
        &client,
        &ws,
        &mut session,
        &Permissions {
            read_only: true,
            allow_write: false,
            allow_shell: false,
        },
        "Review using a relevant skill",
        5,
    )
    .await
    .unwrap();
    assert_eq!(output, "Review finished");
    let requests = server.await.unwrap();
    assert!(!requests[0]["messages"]
        .to_string()
        .contains("UNIQUE_BODY_READ_ON_DEMAND"));
    assert!(requests[1]["messages"]
        .to_string()
        .contains("UNIQUE_BODY_READ_ON_DEMAND"));
    assert!(requests[2]["messages"]
        .to_string()
        .contains("Targeted reference content"));
}
#[test]
fn malformed_manifest_and_oversized_resources_are_diagnosed() {
    let dir = tempfile::tempdir().unwrap();
    package(
        dir.path(),
        "bad",
        "---\nname: [wrong-type]\ndescription: Test\n---\nbody",
    );
    package(
        dir.path(),
        "good",
        "---\nname: good\ndescription: Works\n---\nbody",
    );
    let catalog = Catalog::discover(&[dir.path().into()]);
    assert_eq!(catalog.entries.len(), 1);
    assert!(!catalog.diagnostics.is_empty());
    fs::write(
        dir.path().join("good/references/huge.md"),
        vec![b'a'; 32769],
    )
    .unwrap();
    assert!(catalog.resource("good", "references/huge.md", &[]).is_err());
}

#[tokio::test]
async fn portable_script_runs_in_project_only_with_existing_shell_approval() {
    let project = tempfile::tempdir().unwrap();
    let packages = tempfile::tempdir().unwrap();
    package(packages.path(),"scripts","---\nname: scripts\ndescription: Script workflow\n---\nRead and run scripts using approved commands");
    let ws = workspace(project.path(), packages.path());
    #[cfg(windows)]
    let (name, body) = ("test.ps1", "Write-Output 'PORTABLE_SCRIPT_EXECUTED'");
    #[cfg(unix)]
    let (name, body) = ("test.sh", "printf 'PORTABLE_SCRIPT_EXECUTED\\n'");
    let path = packages.path().join("scripts").join(name);
    fs::write(&path, body).unwrap();
    assert!(ws
        .skills
        .resource("scripts", name, &[])
        .unwrap()
        .contains("PORTABLE_SCRIPT_EXECUTED"));
    #[cfg(windows)]
    let command = format!("& '{}'", path.to_string_lossy().replace('\'', "''"));
    #[cfg(unix)]
    let command = format!("sh '{}'", path.to_string_lossy().replace('\'', "'\"'\"'"));
    let result = crate::tools::execute(
        &ws,
        &Permissions {
            read_only: false,
            allow_write: false,
            allow_shell: true,
        },
        "run_command",
        &json!({"command":command}),
    )
    .await
    .unwrap();
    assert!(result.starts_with("Exit code: Some(0)"), "{result}");
    assert!(result.contains("PORTABLE_SCRIPT_EXECUTED"));
}
