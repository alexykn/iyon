use ratatui::{
    style::{Color, Style},
    text::Line,
};

#[derive(Debug, Clone)]
pub(crate) enum TimelineItem {
    UserMessage { text: String },
    AgentMessage { text: String },
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct TuiFormatter;

impl TuiFormatter {
    pub(crate) fn format(&self, item: &TimelineItem) -> Vec<Line<'static>> {
        let (text, style) = match item {
            TimelineItem::UserMessage { text } => (
                text.to_string(),
                Style::default().bg(Color::Rgb(45, 55, 72)),
            ),
            TimelineItem::AgentMessage { text } => (text.to_string(), Style::default()),
        };

        let mut rows = Vec::new();
        rows.push(Line::styled("", style));
        rows.extend(styled_lines_preserving_newlines(&text, style));
        rows.push(Line::styled("", style));
        rows
    }
}

fn styled_lines_preserving_newlines(text: &str, style: Style) -> Vec<Line<'static>> {
    text.split('\n')
        .map(|line| Line::styled(line.to_string(), style))
        .collect()
}

pub(crate) fn timeline_item_from_input(input: &str) -> TimelineItem {
    TimelineItem::UserMessage {
        text: input.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formatter_preserves_multiline_message_shape() {
        let formatter = TuiFormatter;
        let item = TimelineItem::UserMessage {
            text: "line1\nline2\n".to_string(),
        };

        let lines = formatter.format(&item);
        let text_rows = lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>();

        assert_eq!(text_rows, vec!["", "line1", "line2", "", ""]);
    }
}
