use iyon_tui::{StyleRef, Text, TextSpan, View};

use crate::transcript::ToolTimelineStatus;

mod bash;
mod edit;
mod find;
mod generic;
mod grep;
mod ls;
mod read;
mod write;

pub(crate) use bash::BashRenderer;
pub(crate) use edit::EditRenderer;
pub(crate) use find::FindRenderer;
pub(crate) use generic::GenericRenderer;
pub(crate) use grep::GrepRenderer;
pub(crate) use ls::LsRenderer;
pub(crate) use read::ReadRenderer;
pub(crate) use write::WriteRenderer;

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
    let style = style.into();
    View::hanging(
        text("● ", style.clone()).no_wrap(),
        View::text("  ").no_wrap(),
        text(text_value, style).fill_width(),
    )
    .fill_width()
}

pub(super) fn tool_result_line(text_value: impl Into<String>, style: impl Into<StyleRef>) -> View {
    let style = style.into();
    View::hanging(
        View::text("  ").no_wrap(),
        View::text("  ").no_wrap(),
        text(text_value, style).fill_width(),
    )
    .fill_width()
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
