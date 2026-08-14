use std::time::Duration;

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
    while harness.step().unwrap() {}
    assert!(
        harness
            .screen_lines()
            .iter()
            .any(|line| line.contains("echo hi"))
    );
    handle
        .send(IyonAction::Backend(FrontendEvent::ToolCallUpdated {
            tool_call_id: "tool-1".into(),
            update: ToolUpdatePresentation::Text("running\nsecond".into()),
        }))
        .unwrap();
    while harness.step().unwrap() {}
    assert!(
        harness
            .screen_lines()
            .iter()
            .any(|line| line.contains("running"))
    );
    assert!(
        harness
            .screen_lines()
            .iter()
            .any(|line| line.contains("second"))
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
async fn pending_assistant_smoothing_flushes_before_tool_call() {
    let core = IyonCore::spawn_default_on_current_runtime();
    let (commands, _events) = core.split();
    let mut harness = testing::start(build_app(commands, selection()), 60, 20).unwrap();
    let handle = harness.handle();
    handle
        .send(IyonAction::Backend(FrontendEvent::AssistantDelta {
            text: "assistant tail".into(),
        }))
        .unwrap();
    while harness.step().unwrap() {}
    handle
        .send(IyonAction::Backend(FrontendEvent::ToolCallStarted {
            tool_call_id: "boundary-tool".into(),
            tool_name: "bash".into(),
            arguments: serde_json::json!({"command":"true"}),
        }))
        .unwrap();
    while harness.step().unwrap() {}

    assert!(
        harness
            .screen_lines()
            .iter()
            .any(|line| line.contains("assistant tail"))
    );
    let after_tool = harness.screen_lines();
    harness
        .advance_time(std::time::Duration::from_secs(1))
        .unwrap();
    assert_eq!(harness.screen_lines(), after_tool);
}

#[tokio::test(flavor = "current_thread")]
async fn pending_assistant_smoothing_survives_turn_cancellation() {
    let core = IyonCore::spawn_default_on_current_runtime();
    let (commands, _events) = core.split();
    let mut harness = testing::start(build_app(commands, selection()), 60, 20).unwrap();
    let handle = harness.handle();
    handle
        .send(IyonAction::Backend(FrontendEvent::AssistantDelta {
            text: "cancelled assistant tail".into(),
        }))
        .unwrap();
    while harness.step().unwrap() {}
    handle
        .send(IyonAction::Backend(FrontendEvent::TurnCancelled))
        .unwrap();
    while harness.step().unwrap() {}

    assert!(
        harness
            .screen_lines()
            .iter()
            .any(|line| line.contains("cancelled assistant tail"))
    );
    let after_cancel = harness.screen_lines();
    harness
        .advance_time(std::time::Duration::from_secs(1))
        .unwrap();
    assert_eq!(harness.screen_lines(), after_cancel);
}

#[tokio::test(flavor = "current_thread")]
async fn completed_tool_keeps_composer_below_history() {
    let core = IyonCore::spawn_default_on_current_runtime();
    let (commands, _events) = core.split();
    let mut harness = testing::start(build_app(commands, selection()), 60, 12).unwrap();
    let handle = harness.handle();
    for event in [
        FrontendEvent::TurnStarted,
        FrontendEvent::ToolCallStarted {
            tool_call_id: "completed-tool".into(),
            tool_name: "bash".into(),
            arguments: serde_json::json!({"command":"printf output"}),
        },
        FrontendEvent::ToolCallUpdated {
            tool_call_id: "completed-tool".into(),
            update: ToolUpdatePresentation::Text(
                (1..=30)
                    .map(|row| format!("output {row}"))
                    .collect::<Vec<_>>()
                    .join("\n"),
            ),
        },
        FrontendEvent::ToolResult {
            tool_call_id: "completed-tool".into(),
            tool_name: "bash".into(),
            text: "final output".into(),
            details: serde_json::json!({}),
            is_error: false,
        },
    ] {
        handle.send(IyonAction::Backend(event)).unwrap();
        while harness.step().unwrap() {}
    }
    let lines = harness.screen_lines();
    assert!(lines.iter().any(|line| line.contains("final output")));
    assert!(lines.last().is_some_and(|line| line.contains("effort")));
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

const NUMBERED_GUIDE: &str = r#"Markdown: A Complete Guide

What is Markdown?

Markdown is a lightweight markup language created by John Gruber.

1. Headings

Use # followed by a space, up to six levels:

# Heading 1

## Heading 2

### Heading 3

2. Emphasis (Bold & Italic)

italic text — wrapped in single asterisks
**bold text** — wrapped in double asterisks

3. Lists

Unordered lists use -, *, or +:

- First item
- Second item
  - Nested item
- Third item

Ordered lists use numbers followed by a period:

1. First step
2. Second step
3. Third step

4. Links and Images

Links: [OpenAI](https://openai.com)

5. Code

Inline code uses single backticks: `print("hello")`

6. Blockquotes

> This is a blockquote.
> It can span multiple lines.

7. Horizontal Rules

Three or more hyphens:

---

8. Tables
"#;

const NUMBERED_GUIDE_TITLES: &[&str] = &[
    "1. Headings",
    "2. Emphasis",
    "3. Lists",
    "4. Links and Images",
    "5. Code",
    "6. Blockquotes",
    "7. Horizontal Rules",
    "8. Tables",
];

const TIGHT_NUMBERED_LIST: &str = "\
Intro to the guide.\n\
\n\
1. Headings\n\
Use hashes for headings.\n\
2. Emphasis\n\
Wrap words with asterisks.\n\
3. Lists\n\
Use dashes or numbers.\n\
4. Links and Images\n\
Use brackets for links.\n\
5. Code\n\
Use backticks for code.\n\
6. Blockquotes\n\
Use angle brackets.\n\
7. Horizontal Rules\n\
Use three dashes.\n\
8. Tables\n\
Use pipes for columns.\n";

fn titles_in(lines: &[String]) -> Vec<&'static str> {
    NUMBERED_GUIDE_TITLES
        .iter()
        .copied()
        .filter(|title| lines.iter().any(|line| line.contains(title)))
        .collect()
}

fn assert_no_title_gap(lines: &[String], native: &[String], screen: &[String]) {
    let present = titles_in(lines);
    if present.len() < 2 {
        return;
    }
    let first = NUMBERED_GUIDE_TITLES
        .iter()
        .position(|title| *title == present[0])
        .unwrap();
    let last = NUMBERED_GUIDE_TITLES
        .iter()
        .position(|title| *title == *present.last().unwrap())
        .unwrap();
    for title in &NUMBERED_GUIDE_TITLES[first..=last] {
        assert!(
            lines.iter().any(|line| line.contains(title)),
            "native history + screen jumped from {} to {} and lost {title:?}\n\
             native:\n{}\n\nscreen:\n{}",
            present[0],
            present.last().unwrap(),
            native.join("\n"),
            screen.join("\n")
        );
    }
}

#[tokio::test(flavor = "current_thread")]
async fn short_viewport_keeps_numbered_guide_sections_in_history_or_screen() {
    let core = IyonCore::spawn_default_on_current_runtime();
    let (commands, _events) = core.split();
    // Short live region so History must eject rows into native scrollback
    // while the assistant stream is still arriving — the 1.→7. drop showed
    // up after scrolling during a live numbered guide.
    let mut harness = testing::start(build_app(commands, selection()), 80, 12).unwrap();
    let handle = harness.handle();
    handle
        .send(IyonAction::Backend(FrontendEvent::TurnStarted))
        .unwrap();
    handle
        .send(IyonAction::Backend(FrontendEvent::UserMessage {
            text: "explain markdown".into(),
        }))
        .unwrap();
    while harness.step().unwrap() {}

    let transcript = |harness: &iyon_tui::testing::AppHarness<_, _, _, _, _>| {
        let mut lines = harness.native_history_lines();
        lines.extend(harness.screen_lines());
        lines
    };

    handle
        .send(IyonAction::Backend(FrontendEvent::AssistantDelta {
            text: NUMBERED_GUIDE.into(),
        }))
        .unwrap();

    let mut published = Vec::new();
    let mut saw_span = false;
    for _ in 0..400 {
        while harness.step().unwrap() {}
        harness.advance_time(Duration::from_millis(16)).unwrap();
        let native = harness.native_history_lines();
        let screen = harness.screen_lines();
        let lines = transcript(&harness);
        let joined = lines.join("\n");
        for title in NUMBERED_GUIDE_TITLES {
            if joined.contains(title) && !published.contains(title) {
                published.push(*title);
            }
        }
        for title in &published {
            assert!(
                joined.contains(title),
                "lost {title:?} from native history + screen after it had already been painted\n\
                 native:\n{}\n\nscreen:\n{}",
                native.join("\n"),
                screen.join("\n")
            );
        }
        if titles_in(&lines).len() >= 2 {
            saw_span = true;
            assert_no_title_gap(&lines, &native, &screen);
        }
    }

    handle
        .send(IyonAction::Backend(FrontendEvent::TurnFinished))
        .unwrap();
    for _ in 0..80 {
        while harness.step().unwrap() {}
        harness.advance_time(Duration::from_millis(16)).unwrap();
    }
    while harness.step().unwrap() {}

    let native = harness.native_history_lines();
    let screen = harness.screen_lines();
    let lines = transcript(&harness);
    assert!(
        saw_span,
        "pacing never showed two numbered sections at once\n{}",
        lines.join("\n")
    );
    assert_eq!(
        published.as_slice(),
        NUMBERED_GUIDE_TITLES,
        "stream never published every section\n{}",
        lines.join("\n")
    );
    assert_no_title_gap(&lines, &native, &screen);
}

#[tokio::test(flavor = "current_thread")]
async fn short_viewport_keeps_tight_numbered_list_once_painted() {
    let core = IyonCore::spawn_default_on_current_runtime();
    let (commands, _events) = core.split();
    let mut harness = testing::start(build_app(commands, selection()), 80, 8).unwrap();
    let handle = harness.handle();
    handle
        .send(IyonAction::Backend(FrontendEvent::TurnStarted))
        .unwrap();
    handle
        .send(IyonAction::Backend(FrontendEvent::UserMessage {
            text: "list".into(),
        }))
        .unwrap();
    while harness.step().unwrap() {}

    handle
        .send(IyonAction::Backend(FrontendEvent::AssistantDelta {
            text: TIGHT_NUMBERED_LIST.into(),
        }))
        .unwrap();

    let mut published = Vec::new();
    for _ in 0..400 {
        while harness.step().unwrap() {}
        harness.advance_time(Duration::from_millis(16)).unwrap();
        let native = harness.native_history_lines();
        let screen = harness.screen_lines();
        let mut lines = native.clone();
        lines.extend(screen.iter().cloned());
        let joined = lines.join("\n");
        for title in NUMBERED_GUIDE_TITLES {
            if joined.contains(title) && !published.contains(title) {
                published.push(*title);
            }
        }
        for title in &published {
            assert!(
                joined.contains(title),
                "tight list lost {title:?} after it had already been painted\n\
                 native:\n{}\n\nscreen:\n{}",
                native.join("\n"),
                screen.join("\n")
            );
        }
        assert_no_title_gap(&lines, &native, &screen);
    }

    handle
        .send(IyonAction::Backend(FrontendEvent::TurnFinished))
        .unwrap();
    for _ in 0..80 {
        while harness.step().unwrap() {}
        harness.advance_time(Duration::from_millis(16)).unwrap();
    }
    while harness.step().unwrap() {}
    assert_eq!(
        published.as_slice(),
        NUMBERED_GUIDE_TITLES,
        "tight list never published every section"
    );
}

const NESTED_LONG_PARAGRAPHS_AND_TABLE: &str = "\
# Why Markdown Seems Not to Die

You're right — Markdown has shown remarkable longevity. Created in 2004, it's now two decades old, yet it's more ubiquitous than ever.

1. It Solves a Timeless Problem

The need to write structured, portable, human-readable text is permanent. Word processors lock you into binary formats (.docx); Markdown stays as plain .txt with a few symbols.

2. No Vendor Lock-In

Proprietary formats die when companies do. Markdown is:

- An open convention, not a product
- Renderable by hundreds of independent tools
- Recoverable even if every Markdown app disappeared tomorrow

3. Network Effects & Critical Mass

Once GitHub, Reddit, Stack Overflow, and WhatsApp adopted it, it became a lingua franca.

---

- Fruits
  - Citrus
    - Oranges
    - Lemons
  - Berries
    - Strawberries
- Vegetables
  - Leafy greens
    - Spinach
    - Kale

1. Setup the environment
   1. Install dependencies
   2. Configure the settings file
2. Run the build
   1. Compile the source
   2. Run the test suite
      1. Unit tests
      2. Integration tests

- The Quick Brown Fox
  The quick brown fox jumps over the lazy dog. This sentence is famous because it contains every letter of the English alphabet at least once, making it useful for testing fonts, keyboards, and displays.

- Lorem Ipsum Origins
  Lorem ipsum dolor sit amet, consectetur adipiscing elit. It is a long-established fact that a reader will be distracted by the readable content of a page when looking at its layout.

| Format | Born | Status |
| --- | --- | --- |
| LaTeX | 1984 | Alive |
| HTML | 1993 | Alive |
| Markdown | 2004 | Thriving |

> Did you mean something else by \"it seems not to die\"?
> For example, were you referring to a background process, a script, or a specific Markdown renderer that kept running?
";

#[tokio::test(flavor = "current_thread")]
async fn short_viewport_streams_nested_lists_long_paragraphs_and_table() {
    let core = IyonCore::spawn_default_on_current_runtime();
    let (commands, _events) = core.split();
    let mut harness = testing::start(build_app(commands, selection()), 80, 16).unwrap();
    let handle = harness.handle();
    handle
        .send(IyonAction::Backend(FrontendEvent::TurnStarted))
        .unwrap();
    handle
        .send(IyonAction::Backend(FrontendEvent::UserMessage {
            text: "do some lists".into(),
        }))
        .unwrap();
    while harness.step().unwrap() {}

    handle
        .send(IyonAction::Backend(FrontendEvent::AssistantDelta {
            text: NESTED_LONG_PARAGRAPHS_AND_TABLE.into(),
        }))
        .unwrap();

    for step in 0..800 {
        while harness.step().unwrap_or_else(|error| {
            panic!("nested lists/table stream failed at step {step}: {error}")
        }) {}
        harness.advance_time(Duration::from_millis(16)).unwrap();
    }

    handle
        .send(IyonAction::Backend(FrontendEvent::TurnFinished))
        .unwrap();
    for step in 0..80 {
        while harness.step().unwrap_or_else(|error| {
            panic!("nested lists/table seal failed at step {step}: {error}")
        }) {}
        harness.advance_time(Duration::from_millis(16)).unwrap();
    }
    while harness.step().unwrap() {}
}
