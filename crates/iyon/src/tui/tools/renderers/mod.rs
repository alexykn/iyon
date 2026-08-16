use iyon_tui::{DiffRenderer, IntoView, Renderer, StyleRef, StyleSpec, Text, TextSpan, View};

use crate::tools::types::ToolCallRenderInput;
use crate::transcript::ToolTimelineStatus;

mod generic;

pub(crate) use generic::GenericRenderer;

pub(super) fn tool_style(status: ToolTimelineStatus) -> StyleRef {
    let key = match status {
        ToolTimelineStatus::Preparing | ToolTimelineStatus::Running => "tool.running",
        ToolTimelineStatus::Prepared
        | ToolTimelineStatus::Approved
        | ToolTimelineStatus::Finished => "tool.finished",
        ToolTimelineStatus::PendingApproval => "text.warning",
        ToolTimelineStatus::Failed
        | ToolTimelineStatus::Rejected
        | ToolTimelineStatus::Cancelled => "tool.error",
    };
    StyleRef::theme(key)
}

pub(super) fn result_style(is_error: bool) -> StyleRef {
    if is_error {
        tool_style(ToolTimelineStatus::Failed)
    } else {
        StyleRef::theme("text.muted")
    }
}

pub(super) fn text(text: impl Into<String>, style: impl Into<StyleRef>) -> Text {
    View::styled_text([TextSpan::styled(text, style)])
}

pub(super) fn tool_call(text_value: String, style: impl Into<StyleRef>) -> View {
    hanging_tool_call(text_value, style.into(), None)
}

pub(super) fn tool_call_line(text_value: String, input: ToolCallRenderInput<'_>) -> View {
    let style = tool_style(input.status);
    let bullet_style = pulse_bullet_style(style.clone(), input);
    hanging_tool_call(text_value, style, Some(bullet_style))
}

fn pulse_bullet_style(style: StyleRef, input: ToolCallRenderInput<'_>) -> StyleRef {
    if input.pulse {
        style.overrides(StyleSpec::new().dim())
    } else {
        style
    }
}

fn hanging_tool_call(text_value: String, style: StyleRef, bullet_style: Option<StyleRef>) -> View {
    let bullet_style = bullet_style.unwrap_or_else(|| style.clone());
    View::hanging(
        text("● ", bullet_style).no_wrap(),
        View::text("  ").no_wrap(),
        text(text_value, style).fill_width(),
    )
    .fill_width()
}

pub(super) fn tool_result_block(body: impl IntoView) -> View {
    View::hanging(View::text("  ").no_wrap(), View::text("  ").no_wrap(), body).fill_width()
}

pub(super) fn tool_result_line(text_value: impl Into<String>, style: impl Into<StyleRef>) -> View {
    tool_result_block(text(text_value, style).fill_width())
}

pub(super) fn result_lines(text_value: &str, style: impl Into<StyleRef>) -> Vec<View> {
    let style = style.into();
    text_value
        .split('\n')
        .map(|line| tool_result_line(line, style.clone()))
        .collect()
}

pub(super) fn column(children: Vec<View>) -> View {
    View::vertical(|column| {
        column.children(children);
    })
    .fill_width()
}

pub(super) fn structured_diff_view(details: &serde_json::Value) -> Option<View> {
    let diff = details.get("diff").and_then(serde_json::Value::as_str)?;
    if diff.is_empty() {
        return None;
    }
    Some(match crate::tools::unified_diff::parse_unified_diff(diff) {
        Ok(hunks) => DiffRenderer::new().render(hunks.as_slice()),
        Err(_) => {
            let style = result_style(false);
            View::vertical(|column| {
                column.children(
                    diff.split('\n')
                        .map(|line| text(line, style.clone()).fill_width().into_view()),
                );
            })
            .fill_width()
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use iyon_tui::IntoView;

    #[test]
    fn tool_layout_helpers_lower_to_final_semantic_composition() {
        let style =
            StyleRef::direct(iyon_tui::StyleSpec::new().foreground(iyon_tui::ColorSpec::ansi(1)));
        let call = tool_call("ready".to_string(), style.clone());
        let expected_call = View::hanging(
            text("● ", style.clone()).no_wrap(),
            View::text("  ").no_wrap(),
            text("ready", style.clone()).fill_width(),
        )
        .fill_width();
        assert_eq!(call, expected_call);

        let result = tool_result_line("result", style.clone());
        let expected_result = View::hanging(
            View::text("  ").no_wrap(),
            View::text("  ").no_wrap(),
            text("result", style.clone()).fill_width(),
        )
        .fill_width();
        assert_eq!(result, expected_result);

        let children = vec![View::text("a").into_view(), View::text("b").into_view()];
        let expected_column = View::vertical(|column| {
            column.children(children.clone());
        })
        .fill_width();
        assert_eq!(column(children), expected_column);
    }
}
