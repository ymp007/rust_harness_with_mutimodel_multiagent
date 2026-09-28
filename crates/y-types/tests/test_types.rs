use y_types::*;

#[tokio::test]
async fn test_turn_and_session_id_generation() {
    let turn1 = TurnId::new();
    let turn2 = TurnId::new();
    assert_ne!(turn1, turn2);

    let session1 = SessionId::new();
    let session2 = SessionId::new();
    assert_ne!(session1, session2);
}

#[tokio::test]
async fn test_message_constructors() {
    let sys = Message::system("System prompt");
    assert_eq!(sys.role, Role::System);
    assert_eq!(sys.content, "System prompt");

    let usr = Message::user("Hello");
    assert_eq!(usr.role, Role::User);

    let asst = Message::assistant("Hi there");
    assert_eq!(asst.role, Role::Assistant);

    let tool = Message::tool("output data", "call_123");
    assert_eq!(tool.role, Role::Tool);
    assert_eq!(tool.tool_call_id.as_deref(), Some("call_123"));
}

#[tokio::test]
async fn test_noop_lifecycle_hook() {
    let hook = NoOpLifecycleHook;
    let turn_id = TurnId::new();
    let session = SessionState::new(SessionId::new());

    let action = hook.on_turn_start(&turn_id, &session).await.unwrap();
    assert!(matches!(action, TurnAction::Proceed));
}
