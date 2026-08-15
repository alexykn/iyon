use iyon_tui::stream::StreamingSource;
use iyon_tui::text::SemanticTag;
use iyon_tui::{BorderEdges, BorderSpec, ColorSpec, IntoView, OverflowIndicator, View};

use crate::tools::{
    MAX_COLLAPSED_TOOL_ROWS, ToolCallRenderInput, ToolOutcome, ToolRendererRegistry,
    ToolResultRenderInput,
};
use crate::transcript::pipeline::{assistant_renderer, assistant_view};

pub(crate) fn thinking_tag() -> SemanticTag {
    SemanticTag::new("app", "thinking").expect("static semantic tag is valid")
}

/// The kind of an assistant-message segment. `Thinking` is streamed reasoning,
/// kept logically distinct from answer text so it can be styled independently.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SegmentKind {
    Text,
    Thinking,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AssistantSegment {
    Text(String),
    Thinking(String),
}

impl AssistantSegment {
    pub(crate) fn text(&self) -> &str {
        match self {
            Self::Text(text) | Self::Thinking(text) => text,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum TimelineItem {
    UserMessage {
        text: String,
    },
    AssistantMessage {
        segments: Vec<AssistantSegment>,
    },
    ErrorMessage {
        text: String,
    },
    ToolCall {
        tool_call_id: String,
        tool_name: String,
        arguments: serde_json::Value,
        status: ToolTimelineStatus,
        pulse: bool,
    },
    ToolResult {
        tool_call_id: String,
        tool_name: String,
        text: String,
        details: serde_json::Value,
        is_error: bool,
        collapsed: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ToolTimelineStatus {
    Preparing,
    Prepared,
    PendingApproval,
    Running,
    Approved,
    Rejected,
    Finished,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct TuiFormatter {
    tool_renderers: ToolRendererRegistry,
    pub(crate) show_arg_preview: bool,
}

impl TuiFormatter {
    pub(crate) fn format(&self, item: &TimelineItem) -> View {
        match item {
            TimelineItem::UserMessage { text } => Self::user_batch_view(std::slice::from_ref(text)),
            TimelineItem::AssistantMessage { segments } => {
                let mut stream = crate::transcript::assistant_stream::AssistantStream::new();
                for segment in segments {
                    let kind = match segment {
                        AssistantSegment::Text(_) => SegmentKind::Text,
                        AssistantSegment::Thinking(_) => SegmentKind::Thinking,
                    };
                    stream.push_delta_paced(kind, segment.text());
                }
                stream.seal();
                let semantic = stream.semantic_projection_for_view();
                assistant_view(&semantic, &assistant_renderer())
            }
            TimelineItem::ErrorMessage { text } => self.format_error_message(text),
            TimelineItem::ToolCall {
                tool_name,
                arguments,
                status,
                pulse,
                ..
            } => self.format_tool_call(tool_name, arguments, *status, *pulse),
            TimelineItem::ToolResult {
                tool_name,
                text,
                details,
                is_error,
                collapsed,
                ..
            } => self.format_tool_result(tool_name, text, details, *is_error, *collapsed),
        }
    }

    pub(crate) fn user_batch_view(messages: &[String]) -> View {
        View::vertical(|column| {
            column.children(
                messages
                    .iter()
                    .map(|text| View::text(text.clone()).fill_width()),
            );
        })
        .fill_width()
        .border(
            BorderSpec::plain()
                .edges(BorderEdges::TOP_BOTTOM)
                .color(ColorSpec::theme("input.border")),
        )
    }

    fn format_error_message(&self, text: &str) -> View {
        View::text(text)
            .fill_width()
            .style(iyon_tui::StyleSpec::new().foreground(ColorSpec::theme("text.error")))
            .into_view()
            .padding(crate::tui::viewport_gutter())
    }

    fn format_tool_call(
        &self,
        tool_name: &str,
        arguments: &serde_json::Value,
        status: ToolTimelineStatus,
        pulse: bool,
    ) -> View {
        let view = self.tool_renderers.render_call(ToolCallRenderInput {
            tool_name,
            arguments,
            status,
            show_arg_preview: self.show_arg_preview,
            pulse,
        });
        if self.show_arg_preview {
            view.clamp_rows(
                MAX_COLLAPSED_TOOL_ROWS,
                OverflowIndicator::Footer {
                    prefix: "… more argument lines".to_string(),
                    style: truncation_footer_style_spec().into(),
                },
            )
        } else {
            view
        }
    }

    fn format_tool_result(
        &self,
        tool_name: &str,
        text: &str,
        details: &serde_json::Value,
        is_error: bool,
        collapsed: bool,
    ) -> View {
        let view = self.tool_renderers.render_result(ToolResultRenderInput {
            tool_name,
            text,
            details,
            outcome: if is_error {
                ToolOutcome::Error
            } else {
                ToolOutcome::Success
            },
        });
        if collapsed {
            view.clamp_rows(
                MAX_COLLAPSED_TOOL_ROWS,
                OverflowIndicator::Footer {
                    prefix: "… more lines (full result retained)".to_string(),
                    style: truncation_footer_style_spec().into(),
                },
            )
        } else {
            view
        }
    }
}

fn truncation_footer_style_spec() -> iyon_tui::StyleSpec {
    iyon_tui::StyleSpec::new()
        .foreground(ColorSpec::theme("truncation_footer"))
        .italic()
        .dim()
}
