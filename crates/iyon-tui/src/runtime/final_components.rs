use std::time::{Duration, Instant};

use crate::{
    Component, ComponentCx, EventCx, InteractionResult, Key, KeyStroke, Modifiers, Output, View,
    presentation::IntoView,
    transcript::{TimelineItem, ToolTimelineStatus, TuiFormatter},
};

const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

#[derive(Debug, Clone)]
pub(crate) enum ActivityState {
    Working {
        frame: usize,
    },
    Tool {
        tool_call_id: String,
        tool_name: String,
        arguments: serde_json::Value,
        status: ToolTimelineStatus,
        detail: Option<String>,
    },
}

#[derive(Debug, Clone)]
pub(crate) struct ApprovalDecision {
    pub(crate) approval_id: u64,
    pub(crate) tool_call_id: String,
    pub(crate) approved: bool,
}

#[derive(Debug)]
pub(crate) struct ConversationActivity {
    pub(crate) state: ActivityState,
    approval_id: Option<u64>,
    approval: Output<ApprovalDecision>,
}

impl ConversationActivity {
    pub(crate) fn working() -> Self {
        Self {
            state: ActivityState::Working { frame: 0 },
            approval_id: None,
            approval: Output::new(),
        }
    }

    pub(crate) fn tool(
        tool_call_id: String,
        tool_name: String,
        arguments: serde_json::Value,
        status: ToolTimelineStatus,
        approval_id: Option<u64>,
    ) -> Self {
        Self {
            state: ActivityState::Tool {
                tool_call_id,
                tool_name,
                arguments,
                status,
                detail: None,
            },
            approval_id,
            approval: Output::new(),
        }
    }

    pub(crate) fn approval_output(&self) -> Output<ApprovalDecision> {
        self.approval
    }

    pub(crate) fn transition_to_tool(
        &mut self,
        tool_call_id: String,
        tool_name: String,
        arguments: serde_json::Value,
        status: ToolTimelineStatus,
        approval_id: Option<u64>,
    ) {
        self.state = ActivityState::Tool {
            tool_call_id,
            tool_name,
            arguments,
            status,
            detail: None,
        };
        self.approval_id = approval_id;
    }

    pub(crate) fn update_tool(&mut self, update: Option<String>) {
        if let ActivityState::Tool { detail, .. } = &mut self.state {
            *detail = update;
        }
    }

    pub(crate) fn set_status(&mut self, status: ToolTimelineStatus, approval_id: Option<u64>) {
        if let ActivityState::Tool {
            status: current, ..
        } = &mut self.state
        {
            *current = status;
            self.approval_id = approval_id;
        }
    }

    pub(crate) fn final_item(&self, is_error: bool) -> Option<TimelineItem> {
        let ActivityState::Tool {
            tool_call_id,
            tool_name,
            arguments,
            ..
        } = &self.state
        else {
            return None;
        };
        Some(TimelineItem::ToolCall {
            tool_call_id: tool_call_id.clone(),
            tool_name: tool_name.clone(),
            arguments: arguments.clone(),
            status: if is_error {
                ToolTimelineStatus::Failed
            } else {
                ToolTimelineStatus::Finished
            },
        })
    }

    fn tick(&mut self, _now: Instant, _cx: &mut EventCx<'_>) -> bool {
        let ActivityState::Working { frame } = &mut self.state else {
            return false;
        };
        *frame = frame.wrapping_add(1);
        true
    }

    fn map_key(activity: &Self, key: KeyStroke) -> Option<ActivityCommand> {
        if activity.approval_id.is_none() {
            return None;
        }
        match (key.key(), key.modifiers()) {
            (Key::Enter, Modifiers::NONE) | (Key::Char('y'), Modifiers::NONE) => {
                Some(ActivityCommand::Approve)
            }
            (Key::Escape, Modifiers::NONE) | (Key::Char('n'), Modifiers::NONE) => {
                Some(ActivityCommand::Reject)
            }
            _ => None,
        }
    }

    fn handle_key(
        activity: &mut Self,
        command: ActivityCommand,
        cx: &mut EventCx<'_>,
    ) -> InteractionResult {
        let Some(approval_id) = activity.approval_id else {
            return InteractionResult::Ignored;
        };
        let ActivityState::Tool { tool_call_id, .. } = &activity.state else {
            return InteractionResult::Ignored;
        };
        let tool_call_id = tool_call_id.clone();
        activity.approval_id = None;
        activity.set_status(
            match command {
                ActivityCommand::Approve => ToolTimelineStatus::Approved,
                ActivityCommand::Reject => ToolTimelineStatus::Rejected,
            },
            None,
        );
        cx.emit(
            activity.approval,
            ApprovalDecision {
                approval_id,
                tool_call_id,
                approved: matches!(command, ActivityCommand::Approve),
            },
        );
        InteractionResult::Consumed
    }

    fn tick_callback(activity: &mut Self, now: Instant, cx: &mut EventCx<'_>) -> bool {
        activity.tick(now, cx)
    }
}

#[derive(Clone, Copy)]
enum ActivityCommand {
    Approve,
    Reject,
}

impl Component for ConversationActivity {
    fn view(&self) -> View {
        match &self.state {
            ActivityState::Working { frame } => View::text(format!(
                "{} Working",
                SPINNER_FRAMES[*frame % SPINNER_FRAMES.len()]
            ))
            .fill_width()
            .into_view(),
            ActivityState::Tool {
                tool_name,
                status,
                detail,
                ..
            } => {
                let status = match status {
                    ToolTimelineStatus::PendingApproval => "approval required",
                    ToolTimelineStatus::Running => "running",
                    ToolTimelineStatus::Approved => "approved",
                    ToolTimelineStatus::Rejected => "rejected",
                    ToolTimelineStatus::Finished => "finished",
                    ToolTimelineStatus::Failed => "failed",
                };
                View::vertical(|column| {
                    column.child(format!("tool {tool_name}: {status}"));
                    if let Some(detail) = detail {
                        column.child(detail.clone());
                    }
                })
                .fill_width()
            }
        }
    }

    fn capabilities(&self, cx: &mut ComponentCx<'_, Self>) {
        if matches!(self.state, ActivityState::Working { .. }) {
            cx.tick(Duration::from_millis(80), Self::tick_callback);
        }
        if self.approval_id.is_some() {
            cx.focusable();
            cx.modal_scope();
            cx.key_commands(Self::map_key, Self::handle_key);
        }
    }
}

#[derive(Debug)]
pub(crate) struct UserBatch {
    pub(crate) messages: Vec<String>,
}

impl UserBatch {
    pub(crate) fn new(text: String) -> Self {
        Self {
            messages: vec![text],
        }
    }

    pub(crate) fn push(&mut self, text: String) {
        self.messages.push(text);
    }
}

impl Component for UserBatch {
    fn view(&self) -> View {
        TuiFormatter::user_batch_view(&self.messages)
    }
}
