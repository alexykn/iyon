use std::borrow::Cow;

use crate::{
    presentation::{ColorSpec, Insets, IntoView, OverflowIndicator, ThemeKey, View},
    tools::{ToolCallRenderInput, ToolOutcome, ToolRendererRegistry, ToolResultRenderInput},
    transcript::markdown::{assistant_document_view, parse_assistant},
};

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

pub(crate) fn think_to_text_newline<'a>(
    segments: &[AssistantSegment],
    kind: SegmentKind,
    chunk: &'a str,
) -> Cow<'a, str> {
    if kind == SegmentKind::Text
        && !chunk.starts_with('\n')
        && matches!(
            segments.last(),
            Some(AssistantSegment::Thinking(text)) if !text.ends_with('\n')
        )
    {
        let mut value = String::with_capacity(chunk.len() + 2);
        value.push_str("\n\n");
        value.push_str(chunk);
        Cow::Owned(value)
    } else {
        Cow::Borrowed(chunk)
    }
}

/// Returns the sub-range of `segments` covering bytes `[from_byte, to_byte)`.
pub(crate) fn slice_segments(
    segments: &[AssistantSegment],
    from_byte: usize,
    to_byte: usize,
) -> Vec<AssistantSegment> {
    let from = from_byte.min(to_byte);
    let to = to_byte.max(from_byte);
    let mut out = Vec::new();
    let mut cursor = 0usize;

    for segment in segments {
        let text = segment.text();
        let segment_end = cursor + text.len();
        if segment_end <= from {
            cursor = segment_end;
            continue;
        }
        if cursor >= to {
            break;
        }

        let start = clamp_to_char_boundary(text, from.saturating_sub(cursor));
        let end = char_boundary_after(text, to.saturating_sub(cursor)).max(start);
        if end > start {
            let piece = &text[start..end];
            match segment {
                AssistantSegment::Text(_) => out.push(AssistantSegment::Text(piece.to_string())),
                AssistantSegment::Thinking(_) => {
                    out.push(AssistantSegment::Thinking(piece.to_string()))
                }
            }
        }
        if segment_end >= to {
            break;
        }
        cursor = segment_end;
    }
    out
}

fn clamp_to_char_boundary(text: &str, mut pos: usize) -> usize {
    pos = pos.min(text.len());
    while pos > 0 && !text.is_char_boundary(pos) {
        pos -= 1;
    }
    pos
}

fn char_boundary_after(text: &str, mut pos: usize) -> usize {
    pos = pos.min(text.len());
    while pos < text.len() && !text.is_char_boundary(pos) {
        pos += 1;
    }
    pos
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
    PendingApproval,
    Running,
    Approved,
    Rejected,
    Finished,
    Failed,
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
                assistant_document_view(&parse_assistant(segments))
            }
            TimelineItem::ErrorMessage { text } => self.format_error_message(text),
            TimelineItem::ToolCall {
                tool_name,
                arguments,
                status,
                ..
            } => self.format_tool_call(tool_name, arguments, *status),
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
        .padding(Insets::new(1, 0, 1, 0))
        .background(ColorSpec::Theme(ThemeKey::from("surface.user")))
    }

    fn format_error_message(&self, text: &str) -> View {
        View::text(text)
            .fill_width()
            .style(
                crate::presentation::StyleSpec::new()
                    .foreground(ColorSpec::Theme(ThemeKey::from("text.error"))),
            )
            .into_view()
    }

    fn format_tool_call(
        &self,
        tool_name: &str,
        arguments: &serde_json::Value,
        status: ToolTimelineStatus,
    ) -> View {
        let view = self.tool_renderers.render_call(ToolCallRenderInput {
            tool_name,
            arguments,
            status,
            show_arg_preview: self.show_arg_preview,
        });
        if self.show_arg_preview {
            view.clamp_rows(
                17,
                OverflowIndicator::Footer {
                    prefix: "… more argument lines".to_string(),
                    style: truncation_footer_style_spec(),
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
                16,
                OverflowIndicator::Footer {
                    prefix: "… more lines (full result retained)".to_string(),
                    style: truncation_footer_style_spec(),
                },
            )
        } else {
            view
        }
    }
}

fn truncation_footer_style_spec() -> crate::presentation::StyleSpec {
    crate::presentation::StyleSpec::new()
        .foreground(ColorSpec::Theme(ThemeKey::from("truncation_footer")))
        .italic()
        .dim()
}
