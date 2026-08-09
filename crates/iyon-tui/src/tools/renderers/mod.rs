use crate::{ColorSpec, StyleSpec, Text, TextSpan, View};

pub(super) const TOOL_BODY_OFFSET: u16 = 2;
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

pub(super) fn tool_style(status: ToolTimelineStatus) -> StyleSpec {
    let key = match status {
        ToolTimelineStatus::Failed | ToolTimelineStatus::Rejected => "tool.error",
        ToolTimelineStatus::Finished | ToolTimelineStatus::Approved => "tool.finished",
        ToolTimelineStatus::PendingApproval => "text.warning",
        ToolTimelineStatus::Running => "tool.running",
    };
    StyleSpec::new().foreground(ColorSpec::theme(key))
}

pub(super) fn result_style(is_error: bool) -> StyleSpec {
    if is_error {
        tool_style(ToolTimelineStatus::Failed)
    } else {
        StyleSpec::new().foreground(ColorSpec::theme("text.muted"))
    }
}

pub(super) fn text(text: impl Into<String>, style: StyleSpec) -> Text {
    View::styled_text([TextSpan::styled(text, style)])
}

pub(super) fn tool_call(text_value: String, style: StyleSpec) -> View {
    View::horizontal(|row| {
        row.child(text("●", style.clone()).no_wrap());
        row.flex(text(text_value, style).fill_width());
        row.gap(1);
    })
    .fill_width()
}

pub(super) fn tool_result_line(text_value: impl Into<String>, style: StyleSpec) -> View {
    View::horizontal(|row| {
        row.fixed(TOOL_BODY_OFFSET, View::text("").fill_width());
        row.flex(text(text_value, style).fill_width());
    })
    .fill_width()
}

pub(super) fn result_lines(text_value: &str, style: StyleSpec) -> Vec<View> {
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
    use crate::IntoView;

    #[test]
    fn tool_layout_helpers_lower_to_final_semantic_composition() {
        let style = StyleSpec::new().foreground(ColorSpec::ansi(1));
        let call = tool_call("ready".to_string(), style.clone());
        let expected_call = View::horizontal(|row| {
            row.child(text("●", style.clone()).no_wrap());
            row.flex(text("ready", style.clone()).fill_width());
            row.gap(1);
        })
        .fill_width();
        assert_eq!(call, expected_call);

        let result = tool_result_line("result", style.clone());
        let expected_result = View::horizontal(|row| {
            row.fixed(TOOL_BODY_OFFSET, View::text("").fill_width());
            row.flex(text("result", style.clone()).fill_width());
        })
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
