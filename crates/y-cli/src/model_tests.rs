use crate::{
    models::{load_mode, save_mode, Profile},
    session::Session,
    team_board::Role,
    tools::Workspace,
};

#[test]
fn profile_toggle_keeps_manager_and_configuration_while_changing_worker_routing() {
    let mut profile = Profile {
        enabled: true,
        developer: Some("small/dev".into()),
        reviewer: Some("small/review".into()),
        qa: Some("small/qa".into()),
        ..Default::default()
    };
    assert_eq!(
        profile
            .resolve(Role::Developer, "big/manager", None)
            .unwrap(),
        "small/dev"
    );
    assert!(profile
        .resolve(Role::Developer, "big/manager", Some("unconfigured/model"))
        .is_err());
    profile.enabled = false;
    for role in [Role::Developer, Role::Reviewer, Role::Qa] {
        assert_eq!(
            profile
                .resolve(role, "big/manager", Some("small/dev"))
                .unwrap(),
            "big/manager"
        );
    }
    assert_eq!(profile.developer.as_deref(), Some("small/dev"));
    profile.enabled = true;
    assert_eq!(
        profile
            .resolve(Role::Reviewer, "big/manager", None)
            .unwrap(),
        "small/review"
    );
}

#[tokio::test]
async fn profile_can_be_configured_live_without_calls_and_persists_on_resume() {
    let dir = tempfile::tempdir().unwrap();
    let ws = Workspace::new(dir.path()).unwrap();
    let (client, server) = crate::mode_tests::provider(vec![]).await;
    let mut session = Session::new(&ws, "big/manager").unwrap();
    let mut profile = session.profile();
    profile.configure(&client, "worker small/general").unwrap();
    profile.configure(&client, "reviewer other/review").unwrap();
    assert!(profile
        .configure(&client, "developer invalid model")
        .is_err());
    profile.enabled = false;
    session.set_profile(profile);
    session.save(&ws).unwrap();
    let loaded = Session::load(&ws, "last").unwrap();
    assert_eq!(loaded.model, "big/manager");
    assert!(!loaded.multi_model);
    assert_eq!(loaded.developer_model.as_deref(), Some("small/general"));
    assert_eq!(loaded.reviewer_model.as_deref(), Some("other/review"));
    save_mode(&ws, &session.id, true).unwrap();
    assert_eq!(load_mode(&ws, &session.id).unwrap(), Some(true));
    assert_eq!(
        load_mode(&ws, &y_types::SessionId::new().to_string()).unwrap(),
        None
    );
    assert!(server.await.unwrap().is_empty());
}

#[tokio::test]
async fn profile_routes_provider_and_model_together_and_off_ignores_unused_credentials() {
    let (manager, server) = crate::mode_tests::provider(vec![]).await;
    let manager = manager.with_routes(std::collections::HashMap::from([
        (
            "local".into(),
            y_gateway::ApiEndpoint {
                url: "http://127.0.0.1:11434/v1/chat/completions".into(),
                format: y_gateway::ApiFormat::Openai,
                key_env: None,
            },
        ),
        (
            "missing".into(),
            y_gateway::ApiEndpoint {
                url: "https://example.com/messages".into(),
                format: y_gateway::ApiFormat::Anthropic,
                key_env: Some("YCODE_TEST_UNSET_PROVIDER_CREDENTIAL_12345".into()),
            },
        ),
    ]));
    let mut profile = Profile::default();
    profile
        .configure(&manager, "developer small/model local")
        .unwrap();
    let worker = profile
        .worker_client(&manager, Role::Developer, "small/model")
        .unwrap();
    assert_eq!(
        worker.endpoint(),
        "http://127.0.0.1:11434/v1/chat/completions"
    );
    profile.enabled = false;
    assert_eq!(
        profile
            .worker_client(&manager, Role::Developer, "big/manager")
            .unwrap()
            .endpoint(),
        manager.endpoint()
    );
    profile.developer_provider = Some("missing".into());
    assert!(profile
        .worker_client(&manager, Role::Developer, "big/manager")
        .is_ok());
    profile.enabled = true;
    assert!(profile
        .worker_client(&manager, Role::Developer, "small/model")
        .is_err());
    let dir = tempfile::tempdir().unwrap();
    let ws = Workspace::new(dir.path()).unwrap();
    let mut saved = Session::new(&ws, "big/manager").unwrap();
    saved.manager_provider = Some("manager-api".into());
    saved.set_profile(profile);
    saved.save(&ws).unwrap();
    let resumed = Session::load(&ws, "last").unwrap();
    assert_eq!(resumed.manager_provider.as_deref(), Some("manager-api"));
    assert_eq!(resumed.developer_provider.as_deref(), Some("missing"));
    assert!(server.await.unwrap().is_empty());
}

#[test]
fn command_line_worker_defaults_role_overrides_and_off_switch_are_explicit() {
    let dir = tempfile::tempdir().unwrap();
    let ws = Workspace::new(dir.path()).unwrap();
    let mut session = Session::new(&ws, "big/manager").unwrap();
    let matches = crate::cli()
        .try_get_matches_from([
            "ycode",
            "--worker-model",
            "small/general",
            "--reviewer-model",
            "other/review",
            "--no-multi-model",
        ])
        .unwrap();
    crate::apply_modes(&matches, &mut session, true);
    assert!(!session.multi_model);
    assert_eq!(session.developer_model.as_deref(), Some("small/general"));
    assert_eq!(session.reviewer_model.as_deref(), Some("other/review"));
    assert!(crate::cli()
        .try_get_matches_from(["ycode", "--multi-model", "--no-multi-model"])
        .is_err());
    let alias = crate::cli()
        .try_get_matches_from(["ycode", "--manager-model", "big/manager"])
        .unwrap();
    assert_eq!(
        alias.get_one::<String>("model").map(String::as_str),
        Some("big/manager")
    );
}
