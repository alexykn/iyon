use iyon::tui::{FrontendEvent, IyonAction, ToolUpdatePresentation, build_app};
use iyon_core::{IyonCore, ModelSelection};
use iyon_tui::{Key, KeyStroke, testing};

fn selection() -> ModelSelection {
    ModelSelection {
        provider: "mock".into(),
        model_id: "mock".into(),
    }
}

#[tokio::test(flavor = "current_thread")]
async fn iyon_app_is_drivable_through_public_tui_harness() {
    let core = IyonCore::spawn_default_on_current_runtime();
    let (commands, receiver) = core.split();
    drop(receiver);
    let mut harness = testing::start(build_app(commands, selection()), 60, 12).unwrap();
    assert!(
        harness
            .screen_lines()
            .iter()
            .any(|line| line.contains("effort"))
    );

    harness.key(KeyStroke::new(Key::Char('h'))).unwrap();
    harness.key(KeyStroke::new(Key::Char('i'))).unwrap();
    harness.paste(" small paste").unwrap();
    assert!(
        harness
            .screen_lines()
            .iter()
            .any(|line| line.contains("hi small paste"))
    );
    harness.paste(&"x".repeat(1001)).unwrap();
    assert!(
        harness
            .screen_lines()
            .iter()
            .any(|line| line.contains("Pasted Content 1001 chars"))
    );
    harness
        .key(KeyStroke::with_modifiers(
            Key::Char('c'),
            iyon_tui::Modifiers::CONTROL,
        ))
        .unwrap();
    assert!(
        !harness
            .screen_lines()
            .iter()
            .any(|line| line.contains("Pasted Content 1001 chars"))
    );

    harness.key(KeyStroke::new(Key::Enter)).unwrap();
    harness.step().unwrap();

    let handle = harness.handle();
    handle
        .send(IyonAction::Backend(FrontendEvent::TurnStarted))
        .unwrap();
    handle
        .send(IyonAction::Backend(FrontendEvent::UserMessage {
            text: "user".into(),
        }))
        .unwrap();
    while harness.step().unwrap() {}
    let working_before = harness.screen_lines();
    harness
        .advance_time(std::time::Duration::from_millis(80))
        .unwrap();
    let working_after = harness.screen_lines();
    assert_ne!(
        working_before, working_after,
        "working indicator did not tick"
    );

    handle
        .send(IyonAction::Backend(FrontendEvent::AssistantDelta {
            text: "assistant response".into(),
        }))
        .unwrap();
    while harness.step().unwrap() {}
    harness
        .advance_time(std::time::Duration::from_millis(16))
        .unwrap();
    assert!(harness.screen_lines().iter().any(|line| line.contains("a")));
    harness
        .advance_time(std::time::Duration::from_millis(1000))
        .unwrap();
    assert!(
        harness
            .screen_lines()
            .iter()
            .any(|line| line.contains("assistant")),
        "stream did not advance before turn completion"
    );
    handle
        .send(IyonAction::Backend(FrontendEvent::TurnFinished))
        .unwrap();
    while harness.step().unwrap() {}
    assert!(
        harness
            .screen_lines()
            .iter()
            .any(|line| line.contains("assistant"))
    );

    handle
        .send(IyonAction::Backend(FrontendEvent::ToolCallStarted {
            tool_call_id: "tool-1".into(),
            tool_name: "bash".into(),
            arguments: serde_json::json!({"command":"echo hi"}),
        }))
        .unwrap();
    handle
        .send(IyonAction::Backend(FrontendEvent::ToolCallUpdated {
            tool_call_id: "tool-1".into(),
            update: ToolUpdatePresentation::Text("running".into()),
        }))
        .unwrap();
    while harness.step().unwrap() {}
    assert!(
        harness
            .screen_lines()
            .iter()
            .any(|line| line.contains("echo hi"))
    );
    assert!(
        harness
            .screen_lines()
            .iter()
            .any(|line| line.contains("running"))
    );

    handle.send(IyonAction::RequestExit).unwrap();
    while harness.step().unwrap() {}
    assert!(harness.is_exiting());
    assert!(
        harness
            .screen_lines()
            .iter()
            .any(|line| line.contains("Goodbye"))
    );
}

#[tokio::test(flavor = "current_thread")]
async fn missing_tool_result_is_forced_finalized_at_turn_end() {
    let core = IyonCore::spawn_default_on_current_runtime();
    let (commands, _events) = core.split();
    let mut harness = testing::start(build_app(commands, selection()), 60, 20).unwrap();
    let handle = harness.handle();
    handle
        .send(IyonAction::Backend(FrontendEvent::TurnStarted))
        .unwrap();
    handle
        .send(IyonAction::Backend(FrontendEvent::ToolCallStarted {
            tool_call_id: "missing-result".into(),
            tool_name: "bash".into(),
            arguments: serde_json::json!({"command":"true"}),
        }))
        .unwrap();
    while harness.step().unwrap() {}
    handle
        .send(IyonAction::Backend(FrontendEvent::TurnFinished))
        .unwrap();
    while harness.step().unwrap() {}

    handle.send(IyonAction::CtrlC).unwrap();
    while harness.step().unwrap() {}
    assert!(harness.is_exiting());
    assert!(
        harness
            .screen_lines()
            .iter()
            .any(|line| line.contains("Goodbye"))
    );
}

#[tokio::test(flavor = "current_thread")]
async fn request_exit_flushes_buffered_assistant_text_before_goodbye() {
    let core = IyonCore::spawn_default_on_current_runtime();
    let (commands, _events) = core.split();
    let mut harness = testing::start(build_app(commands, selection()), 60, 20).unwrap();
    let handle = harness.handle();
    handle
        .send(IyonAction::Backend(FrontendEvent::TurnStarted))
        .unwrap();
    handle
        .send(IyonAction::Backend(FrontendEvent::UserMessage {
            text: "user".into(),
        }))
        .unwrap();
    handle
        .send(IyonAction::Backend(FrontendEvent::AssistantDelta {
            text: "buffered assistant".into(),
        }))
        .unwrap();
    while harness.step().unwrap() {}

    handle.send(IyonAction::RequestExit).unwrap();
    while harness.step().unwrap() {}
    let lines = harness.screen_lines();
    let assistant = lines
        .iter()
        .position(|line| line.contains("buffered assistant"))
        .expect("buffered assistant text");
    let goodbye = lines
        .iter()
        .position(|line| line.contains("Goodbye"))
        .expect("goodbye");
    assert!(assistant < goodbye);
}

#[tokio::test(flavor = "current_thread")]
async fn approval_freezes_a_user_batch_delivered_after_an_existing_tool() {
    let core = IyonCore::spawn_default_on_current_runtime();
    let (commands, _events) = core.split();
    let mut harness = testing::start(build_app(commands, selection()), 60, 20).unwrap();
    let handle = harness.handle();
    for event in [
        FrontendEvent::TurnStarted,
        FrontendEvent::UserMessage {
            text: "first".into(),
        },
        FrontendEvent::ToolCallStarted {
            tool_call_id: "approval-tool".into(),
            tool_name: "bash".into(),
            arguments: serde_json::json!({"command":"true"}),
        },
        FrontendEvent::UserMessage {
            text: "steered batch".into(),
        },
        FrontendEvent::ToolApprovalRequested {
            approval_id: 7,
            tool_call_id: "approval-tool".into(),
            tool_name: "bash".into(),
            arguments: serde_json::json!({"command":"true"}),
        },
    ] {
        handle.send(IyonAction::Backend(event)).unwrap();
        while harness.step().unwrap() {}
    }
    let lines = harness.screen_lines();
    let user = lines
        .iter()
        .position(|line| line.contains("steered batch"))
        .expect("steered user batch");
    let approval = lines
        .iter()
        .position(|line| line.contains("waiting for approval"))
        .expect("approval prompt");
    assert_ne!(user, approval);
}
