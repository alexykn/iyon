use iyon_api::ReasoningLevel;
use iyon_tui::{
    ColorSpec, StyleSelector, StyleSpec, StyleStateKey, StyleStateValue, TextSelector, Theme,
};

use super::transcript::thinking_tag;

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
            "text.heading",
            iyon_tui::ThemeColor::Rgb {
                r: 255,
                g: 196,
                b: 87,
            },
        )
        .with_color(
            "text.code",
            iyon_tui::ThemeColor::Rgb {
                r: 120,
                g: 200,
                b: 210,
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
        .with_text_style(
            TextSelector::heading(),
            StyleSpec::new().foreground(ColorSpec::theme("text.heading")),
        )
        .with_text_style(
            TextSelector::inline_code(),
            StyleSpec::new().foreground(ColorSpec::theme("text.code")),
        )
        .with_text_style(
            TextSelector::code_block(),
            StyleSpec::new().foreground(ColorSpec::theme("text.code")),
        )
        .with_text_style(
            TextSelector::annotation(&thinking_tag()),
            StyleSpec::new()
                .foreground(ColorSpec::theme("text.muted"))
                .italic(),
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
            theme.color("text.heading"),
            Some(iyon_tui::ThemeColor::Rgb {
                r: 255,
                g: 196,
                b: 87
            })
        );
        assert_eq!(
            theme.color("text.code"),
            Some(iyon_tui::ThemeColor::Rgb {
                r: 120,
                g: 200,
                b: 210
            })
        );
        assert!(theme.style("markdown.header").is_none());
        assert!(theme.style("markdown.code").is_none());
    }

    #[test]
    fn assistant_text_styles_come_from_theme_not_the_renderer() {
        use crate::tui::transcript::pipeline::assistant_renderer;
        use iyon_tui::text::{CodeBlock, Inline, InlineContent, LanguageId, LiteralText, Renderer};
        use iyon_tui::{Block, HeadingLevel, MarkdownOptions, MarkdownProjector};

        let renderer = assistant_renderer();
        assert_eq!(
            renderer.policy().soft_break(),
            iyon_tui::SoftBreakPolicy::LineBreak
        );
        assert_eq!(
            MarkdownProjector::default().options(),
            MarkdownOptions::commonmark()
        );

        let theme = iyon_theme();
        let heading = renderer.render(&Block::heading(HeadingLevel::H1, "Title"));
        let heading_style = iyon_tui::testing::style_at_text(&heading, 40, &theme, "Title");
        assert_eq!(heading_style.fg_rgb, Some((255, 196, 87)));
        assert!(heading_style.bold);
        assert!(heading_style.underline);

        let code = renderer.render(&Block::paragraph(InlineContent::new([Inline::text(
            "code",
        )
        .code()])));
        assert_eq!(
            iyon_tui::testing::style_at_text(&code, 40, &theme, "code").fg_rgb,
            Some((120, 200, 210))
        );

        let rust = LanguageId::new("rust").unwrap();
        let block = renderer.render(&Block::code(CodeBlock::new(
            Some(rust),
            Some("rust"),
            LiteralText::from("fn"),
        )));
        assert_eq!(
            iyon_tui::testing::style_at_text(&block, 40, &theme, "fn").fg_rgb,
            Some((120, 200, 210))
        );

        let think = iyon_tui::text::TextRun::from("think")
            .map_annotations(|annotations| annotations.with_tag(crate::transcript::thinking_tag()));
        let thinking_view =
            renderer.render(&Block::paragraph(InlineContent::new([Inline::text(think)])));
        let muted = iyon_tui::testing::style_at_text(&thinking_view, 40, &theme, "think");
        assert_eq!(muted.fg_rgb, Some((113, 128, 150)));
        assert!(muted.italic);

        let think_code = iyon_tui::text::TextRun::from("fnx")
            .map_annotations(|annotations| annotations.with_tag(crate::transcript::thinking_tag()));
        let thinking_code = renderer.render(&Block::paragraph(InlineContent::new([Inline::text(
            think_code,
        )
        .code()])));
        let overlay = iyon_tui::testing::style_at_text(&thinking_code, 40, &theme, "fnx");
        assert_eq!(overlay.fg_rgb, Some((113, 128, 150)));
        assert!(overlay.italic);
    }
}
