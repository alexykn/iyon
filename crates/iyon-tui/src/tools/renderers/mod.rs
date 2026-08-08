use crate::presentation::{
    ColorSpec, IntoView, RowChild, StyleSpec, Text, ThemeKey, View, WidthRule,
};

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
    StyleSpec {
        foreground: Some(ColorSpec::Theme(ThemeKey::from(key))),
        ..StyleSpec::default()
    }
}

pub(super) fn result_style(is_error: bool) -> StyleSpec {
    if is_error {
        tool_style(ToolTimelineStatus::Failed)
    } else {
        StyleSpec {
            foreground: Some(ColorSpec::Theme(ThemeKey::from("text.muted"))),
            ..StyleSpec::default()
        }
    }
}

pub(super) fn text(text: impl Into<String>, style: StyleSpec) -> Text {
    View::styled_text(vec![crate::presentation::TextSpan::styled(text, style)])
}

pub(super) fn tool_call(text_value: String, style: StyleSpec) -> View {
    View::row(
        vec![
            RowChild::content(text("●", style.clone()).no_wrap().into_view()),
            RowChild::flex(
                text(text_value, style)
                    .width(crate::presentation::WidthRule::Fill)
                    .into_view(),
            ),
        ],
        1,
    )
}

pub(super) fn tool_result_line(text_value: impl Into<String>, style: StyleSpec) -> View {
    View::row(
        vec![
            RowChild::fixed(
                TOOL_BODY_OFFSET,
                View::text("").width(WidthRule::Fill).into_view(),
            ),
            crate::presentation::RowChild::flex(
                text(text_value, style)
                    .width(crate::presentation::WidthRule::Fill)
                    .into_view(),
            ),
        ],
        0,
    )
}

pub(super) fn result_lines(text_value: &str, style: StyleSpec) -> Vec<View> {
    text_value
        .split('\n')
        .map(|line| tool_result_line(line, style.clone()))
        .collect()
}

pub(super) fn column(children: Vec<View>) -> View {
    View::column(children, 0).width(crate::presentation::WidthRule::Fill)
}

#[allow(dead_code)]
pub(super) fn dim_attributes() -> crate::presentation::api::TextAttributeSpec {
    crate::presentation::api::TextAttributeSpec {
        dim: Some(true),
        ..Default::default()
    }
}
