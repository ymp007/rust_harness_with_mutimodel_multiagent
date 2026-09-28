use y_mesh::{ActorMesh, BoundedMailbox};

#[test]
fn test_bounded_mailbox_backpressure_thresholds() {
    let mailbox = BoundedMailbox::new(10);
    assert!(!mailbox.is_backpressured());

    // Send 7 messages (< 80%)
    for i in 0..7 {
        mailbox.send("agent_1", format!("msg_{i}")).unwrap();
    }
    assert!(!mailbox.is_backpressured());

    // 8th message reaches 80% watermark (8/10)
    mailbox.send("agent_1", "msg_7".to_string()).unwrap();
    assert!(mailbox.is_backpressured());

    // Drain down to 2 messages (<= 20% watermark)
    for _ in 0..6 {
        let _ = mailbox.recv().unwrap();
    }
    assert_eq!(mailbox.len(), 2);
    assert!(!mailbox.is_backpressured()); // Released!
}

#[test]
fn test_actor_mesh_task_submission_and_stealing() {
    let mesh = ActorMesh::new();

    mesh.submit_task(101, "Build AST");
    mesh.submit_task(102, "Generate Diff");

    let task1 = mesh.pull_task().expect("Expected task 1");
    assert_eq!(task1.id, 101);

    let task2 = mesh.pull_task().expect("Expected task 2");
    assert_eq!(task2.id, 102);

    assert!(mesh.pull_task().is_none());
}
