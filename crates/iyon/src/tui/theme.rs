use iyon_api::ReasoningLevel;
use iyon_tui::{StyleSelector, StyleStateKey, StyleStateValue, Theme};

pub(crate) const AGENT_EFFORT: StyleStateKey = StyleStateKey::from_static("iyon.agent.effort");
pub(crate) const EFFORT_NONE: StyleStateValue = StyleStateValue::from_static("none");
pub(crate) const EFFORT_MINIMAL: StyleStateValue = StyleStateValue::from_static("minimal");
pub(crate) const EFFORT_LOW: StyleStateValue = StyleStateValue::from_static("low");
pub(crate) const EFFORT_MEDIUM: StyleStateValue = StyleStateValue::from_static("medium");
pub(crate) const EFFORT_HIGH: StyleStateValue = StyleStateValue::from_static("high");
pub(crate) const EFFORT_XHIGH: StyleStateValue = StyleStateValue::from_static("xhigh");
pub(crate) const EFFORT_MAX: StyleStateValue = StyleStateValue::from_static("max");

pub(crate) fn effort_style_value(level: ReasoningLevel) -> StyleStateValue {
    match level {
        ReasoningLevel::None => EFFORT_NONE.clone(),
        ReasoningLevel::Minimal => EFFORT_MINIMAL.clone(),
        ReasoningLevel::Low => EFFORT_LOW.clone(),
        ReasoningLevel::Medium => EFFORT_MEDIUM.clone(),
        ReasoningLevel::High => EFFORT_HIGH.clone(),
        ReasoningLevel::XHigh => EFFORT_XHIGH.clone(),
        ReasoningLevel::Max => EFFORT_MAX.clone(),
    }
}

pub(crate) fn iyon_theme() -> Theme {
    Theme::new()
        .with_color(
            "surface.user",
            iyon_tui::ThemeColor::Rgb {
                r: 45,
                g: 55,
                b: 72,
            },
        )
        .with_color(
            "text.muted",
            iyon_tui::ThemeColor::Rgb {
                r: 113,
                g: 128,
                b: 150,
            },
        )
        .with_color(
            "surface.default",
            iyon_tui::ThemeColor::Rgb {
                r: 113,
                g: 128,
                b: 150,
            },
        )
        .with_color(
            "tool.running",
            iyon_tui::ThemeColor::Rgb {
                r: 160,
                g: 174,
                b: 192,
            },
        )
        .with_color(
            "tool.finished",
            iyon_tui::ThemeColor::Rgb {
                r: 104,
                g: 211,
                b: 145,
            },
        )
        .with_color(
            "tool.error",
            iyon_tui::ThemeColor::Named(iyon_tui::AnsiColor::Red),
        )
        .with_color(
            "text.error",
            iyon_tui::ThemeColor::Named(iyon_tui::AnsiColor::Red),
        )
        .with_color(
            "text.warning",
            iyon_tui::ThemeColor::Named(iyon_tui::AnsiColor::Yellow),
        )
        .with_color(
            "markdown.header",
            iyon_tui::ThemeColor::Rgb {
                r: 255,
                g: 196,
                b: 87,
            },
        )
        .with_color(
            "markdown.bold",
            iyon_tui::ThemeColor::Rgb {
                r: 230,
                g: 230,
                b: 235,
            },
        )
        .with_color(
            "markdown.italic",
            iyon_tui::ThemeColor::Rgb {
                r: 150,
                g: 180,
                b: 220,
            },
        )
        .with_color(
            "markdown.code",
            iyon_tui::ThemeColor::Rgb {
                r: 120,
                g: 200,
                b: 210,
            },
        )
        .with_color(
            "markdown.list",
            iyon_tui::ThemeColor::Rgb {
                r: 104,
                g: 211,
                b: 145,
            },
        )
        .with_color(
            "truncation_footer",
            iyon_tui::ThemeColor::Rgb {
                r: 120,
                g: 122,
                b: 132,
            },
        )
        .with_color(
            "input.border",
            iyon_tui::ThemeColor::Rgb {
                r: 173,
                g: 216,
                b: 230,
            },
        )
        .with_color_variant(
            "input.border",
            StyleSelector::state(AGENT_EFFORT.clone(), EFFORT_LOW.clone()),
            iyon_tui::ThemeColor::Named(iyon_tui::AnsiColor::Green),
        )
        .with_color_variant(
            "input.border",
            StyleSelector::state(AGENT_EFFORT.clone(), EFFORT_MEDIUM.clone()),
            iyon_tui::ThemeColor::Named(iyon_tui::AnsiColor::Yellow),
        )
        .with_color_variant(
            "input.border",
            StyleSelector::state(AGENT_EFFORT.clone(), EFFORT_HIGH.clone()),
            iyon_tui::ThemeColor::Named(iyon_tui::AnsiColor::Magenta),
        )
        .with_color_variant(
            "input.border",
            StyleSelector::state(AGENT_EFFORT.clone(), EFFORT_LOW.clone()).and_focused(),
            iyon_tui::ThemeColor::Named(iyon_tui::AnsiColor::LightGreen),
        )
        .with_color_variant(
            "input.border",
            StyleSelector::state(AGENT_EFFORT.clone(), EFFORT_HIGH.clone()).and_focused(),
            iyon_tui::ThemeColor::Named(iyon_tui::AnsiColor::LightMagenta),
        )
        .with_style(
            "tool.running",
            iyon_tui::StyleSpec::new().foreground(iyon_tui::ColorSpec::theme("tool.running")),
        )
        .with_style(
            "tool.finished",
            iyon_tui::StyleSpec::new().foreground(iyon_tui::ColorSpec::theme("tool.finished")),
        )
        .with_style(
            "tool.error",
            iyon_tui::StyleSpec::new().foreground(iyon_tui::ColorSpec::theme("tool.error")),
        )
        .with_style(
            "text.muted",
            iyon_tui::StyleSpec::new().foreground(iyon_tui::ColorSpec::theme("text.muted")),
        )
        .with_style(
            "text.error",
            iyon_tui::StyleSpec::new().foreground(iyon_tui::ColorSpec::theme("text.error")),
        )
        .with_style(
            "text.warning",
            iyon_tui::StyleSpec::new().foreground(iyon_tui::ColorSpec::theme("text.warning")),
        )
        .with_style(
            "markdown.header",
            iyon_tui::StyleSpec::new()
                .foreground(iyon_tui::ColorSpec::theme("markdown.header"))
                .bold(),
        )
        .with_style(
            "markdown.bold",
            iyon_tui::StyleSpec::new()
                .foreground(iyon_tui::ColorSpec::theme("markdown.bold"))
                .bold(),
        )
        .with_style(
            "markdown.italic",
            iyon_tui::StyleSpec::new()
                .foreground(iyon_tui::ColorSpec::theme("markdown.italic"))
                .italic(),
        )
        .with_style(
            "markdown.code",
            iyon_tui::StyleSpec::new().foreground(iyon_tui::ColorSpec::theme("markdown.code")),
        )
        .with_style(
            "markdown.list",
            iyon_tui::StyleSpec::new().foreground(iyon_tui::ColorSpec::theme("markdown.list")),
        )
        .with_style(
            "truncation_footer",
            iyon_tui::StyleSpec::new().foreground(iyon_tui::ColorSpec::theme("truncation_footer")),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_preserves_exact_named_and_rgb_values() {
        let theme = iyon_theme();
        assert_eq!(
            theme.color("surface.user"),
            Some(iyon_tui::ThemeColor::Rgb {
                r: 45,
                g: 55,
                b: 72
            })
        );
        assert_eq!(
            theme.color("tool.finished"),
            Some(iyon_tui::ThemeColor::Rgb {
                r: 104,
                g: 211,
                b: 145
            })
        );
        assert_eq!(
            theme.color("tool.error"),
            Some(iyon_tui::ThemeColor::Named(iyon_tui::AnsiColor::Red))
        );
        assert_eq!(
            theme.color("text.warning"),
            Some(iyon_tui::ThemeColor::Named(iyon_tui::AnsiColor::Yellow))
        );
        assert_eq!(
            theme
                .style("markdown.header")
                .unwrap()
                .attribute_value(iyon_tui::TextAttribute::Bold),
            Some(true)
        );
        assert_eq!(
            theme
                .style("markdown.italic")
                .unwrap()
                .attribute_value(iyon_tui::TextAttribute::Italic),
            Some(true)
        );
    }
}
