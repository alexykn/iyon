use std::time::{Duration, Instant};

use iyon_tui::{
    Component, ComponentCx, EventCx, InteractionResult, IntoView, Key, KeyStroke, Modifiers,
    Output, View,
};

use crate::transcript::{TimelineItem, ToolTimelineStatus, TuiFormatter};

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
pub struct ApprovalDecision {
    pub(crate) approval_id: u64,
    pub(crate) tool_call_id: String,
    pub(crate) approved: bool,
}

#[derive(Debug)]
struct ToolResultState {
    text: String,
    details: serde_json::Value,
    is_error: bool,
}

#[derive(Debug)]
pub(crate) struct ConversationActivity {
    pub(crate) state: ActivityState,
    approval_id: Option<u64>,
    approval: Output<ApprovalDecision>,
    formatter: TuiFormatter,
    result: Option<ToolResultState>,
    finished: bool,
}

impl ConversationActivity {
    pub(crate) fn working(formatter: TuiFormatter) -> Self {
        Self {
            state: ActivityState::Working { frame: 0 },
            approval_id: None,
            approval: Output::new(),
            formatter,
            result: None,
            finished: false,
        }
    }

    pub(crate) fn tool(
        formatter: TuiFormatter,
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
            formatter,
            result: None,
            finished: false,
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
        self.result = None;
        self.finished = false;
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

    pub(crate) fn set_result(&mut self, text: String, details: serde_json::Value, is_error: bool) {
        self.result = Some(ToolResultState {
            text,
            details,
            is_error,
        });
    }

    pub(crate) fn complete(&mut self, is_error: bool) {
        if let ActivityState::Tool { status, .. } = &mut self.state {
            *status = if is_error {
                ToolTimelineStatus::Failed
            } else {
                ToolTimelineStatus::Finished
            };
        }
        self.finished = true;
    }

    pub(crate) fn is_finished(&self) -> bool {
        self.finished
    }

    pub(crate) fn has_result(&self) -> bool {
        self.result.is_some()
    }

    fn tool_view(&self) -> Option<View> {
        let ActivityState::Tool {
            tool_call_id,
            tool_name,
            arguments,
            status,
            detail,
        } = &self.state
        else {
            return None;
        };
        let call = self.formatter.format(&TimelineItem::ToolCall {
            tool_call_id: tool_call_id.clone(),
            tool_name: tool_name.clone(),
            arguments: arguments.clone(),
            status: *status,
        });
        let Some(result) = &self.result else {
            let Some(detail) = detail else {
                return Some(call);
            };
            let detail = View::hanging(
                View::text("  ").no_wrap(),
                View::text("  ").no_wrap(),
                View::text(detail.clone())
                    .foreground(iyon_tui::ColorSpec::theme("text.muted"))
                    .fill_width(),
            )
            .fill_width();
            return Some(
                View::vertical(|column| {
                    column.child(call);
                    column.child(detail);
                })
                .fill_width(),
            );
        };
        let result = self.formatter.format(&TimelineItem::ToolResult {
            tool_call_id: tool_call_id.clone(),
            tool_name: tool_name.clone(),
            text: result.text.clone(),
            details: result.details.clone(),
            is_error: result.is_error,
            collapsed: true,
        });
        Some(
            View::vertical(|column| {
                column.child(call);
                column.child(result);
            })
            .fill_width(),
        )
    }

    pub(crate) fn final_view(&self) -> Option<View> {
        self.tool_view()
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
            ActivityState::Tool { .. } => self.tool_view().unwrap_or_else(|| View::spacer(0)),
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
