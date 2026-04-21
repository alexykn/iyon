use ratatui::{
    style::{Color, Style},
    text::Line,
};

#[derive(Debug, Clone)]
pub(crate) enum TimelineItem {
    UserMessage { text: String },
    AgentMessage { text: String },
    Read { text: String },
    Edit { text: String },
    Delete { text: String },
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct TuiFormatter;

impl TuiFormatter {
    pub(crate) fn format(&self, item: &TimelineItem) -> Vec<Line<'static>> {
        let (text, style) = match item {
            TimelineItem::UserMessage { text } => (
                format!(" {}", text),
                Style::default().bg(Color::Rgb(45, 55, 72)),
            ),
            TimelineItem::AgentMessage { text } => (text.to_string(), Style::default()),
            TimelineItem::Read { text } => (
                text.to_string(),
                Style::default().bg(Color::Rgb(42, 74, 74)),
            ),
            TimelineItem::Edit { text } => (
                text.to_string(),
                Style::default().bg(Color::Rgb(82, 72, 44)),
            ),
            TimelineItem::Delete { text } => (
                text.to_string(),
                Style::default().bg(Color::Rgb(84, 48, 48)),
            ),
        };

        vec![
            Line::styled("", style),
            Line::styled(text, style),
            Line::styled("", style),
        ]
    }
}

pub(crate) fn timeline_item_from_input(input: &str) -> TimelineItem {
    if let Some(text) = input.strip_prefix("/agent ") {
        return TimelineItem::AgentMessage {
            text: text.to_string(),
        };
    }
    if let Some(text) = input.strip_prefix("/read ") {
        return TimelineItem::Read {
            text: text.to_string(),
        };
    }
    if let Some(text) = input.strip_prefix("/edit ") {
        return TimelineItem::Edit {
            text: text.to_string(),
        };
    }
    if let Some(text) = input.strip_prefix("/delete ") {
        return TimelineItem::Delete {
            text: text.to_string(),
        };
    }

    TimelineItem::UserMessage {
        text: input.to_string(),
    }
}
