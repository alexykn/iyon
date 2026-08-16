use iyon_api::{ContentBlock, ModelStreamEvent, StopReason};
use serde_json::json;

use super::{
    AgentMessage, ApprovalDecision, ApprovalRequirement, Kernel, KernelConfig, KernelSession,
    ModelTurn, SessionEntry, ToolLifecycleHandle, ToolLifecycleResult,
};
use crate::{
    CoreEvent, MessageDelta, ToolCallDelta,
    ids::{MessageId, SessionId, ToolCallId, TurnId},
    tools::ToolRegistry,
};

#[test]
fn manual_kernel_session_model_turn_tool_lifecycle_transcript_flow() {
    let session = KernelSession::new(SessionId(42));
    let mut kernel = Kernel::new(KernelConfig::new(session, ToolRegistry::new()));
    kernel
        .session_mut()
        .append_message(AgentMessage::user(vec![ContentBlock::Text {
            text: "read README".into(),
        }]))
        .unwrap();
    kernel
        .session_mut()
        .append_entry(SessionEntry::Custom {
            namespace: "test.trace".into(),
            data: json!({"case": "flow"}),
        })
        .unwrap();

    let mut turn = ModelTurn::new(TurnId(7), MessageId(2));
    turn.push_many([
        ModelStreamEvent::Started,
        ModelStreamEvent::ToolCallStart {
            content_index: 0,
            id: None,
            name: Some("read".into()),
        },
        ModelStreamEvent::ToolCallDelta {
            content_index: 0,
            id: None,
            name: None,
            arguments_delta: "{\"path\":\"README\"}".into(),
        },
        ModelStreamEvent::ToolCallEnd {
            content_index: 0,
            id: "provider-call".into(),
            name: "read".into(),
            arguments: json!({"path": "README"}),
        },
        ModelStreamEvent::Done {
            stop_reason: StopReason::ToolUse,
        },
    ])
    .unwrap();
    let normalized = turn.events().to_vec();
    let result = turn.finish().unwrap();
    assert!(normalized.iter().any(|event| matches!(
        event,
        CoreEvent::MessageDelta {
            delta: MessageDelta::ToolCall(ToolCallDelta::Start { .. }),
            ..
        }
    )));
    assert!(result.tool_calls.iter().any(|call| matches!(call, crate::kernel::ToolCallRequest::Ready(call) if call.id.0 == "provider-call")));

    let assistant = result.assistant_message.clone();
    kernel.session_mut().append_message(assistant).unwrap();
    let crate::kernel::ToolCallRequest::Ready(call) = result.tool_calls.into_iter().next().unwrap()
    else {
        panic!("expected assembled call")
    };
    let mut lifecycle = ToolLifecycleHandle::new(call.clone());
    lifecycle.prepared(call.arguments.clone()).unwrap();
    lifecycle.start().unwrap();
    let approval_id = lifecycle
        .request_approval(ApprovalRequirement::Required {
            reason: Some("test input".into()),
        })
        .unwrap()
        .unwrap();
    lifecycle
        .resolve_approval(approval_id, ApprovalDecision::Approved)
        .unwrap();
    lifecycle
        .finish(ToolLifecycleResult {
            content: vec![ContentBlock::Text {
                text: "README contents".into(),
            }],
            details: json!({"bytes": 10}),
            is_error: false,
        })
        .unwrap();
    kernel
        .session_mut()
        .append_message(AgentMessage::tool_result(
            ToolCallId("provider-call".into()),
            "read".into(),
            vec![ContentBlock::Text {
                text: "README contents".into(),
            }],
            json!({"bytes": 10}),
            false,
        ))
        .unwrap();

    let snapshot = kernel.session().snapshot();
    assert_eq!(snapshot.session_id, SessionId(42));
    assert!(snapshot.entries.iter().any(
        |entry| matches!(entry, SessionEntry::Custom { namespace, .. } if namespace == "test.trace")
    ));
    assert_eq!(kernel.session().to_model_messages().len(), 3);
    assert_eq!(lifecycle.state(), super::ToolLifecycleState::Finished);
}
