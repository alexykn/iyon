//! Framework-owned low-priority paint defaults.
//!
//! This layer is resolved before the application Theme. It may provide
//! portable framework semantic defaults, but must not contain product-specific
//! application vocabulary or layout/geometry policy.
//!
//! Generic structured-text defaults live here and are expressed through the
//! public [`TextSelector`] vocabulary. Applications override them via
//! [`Theme::with_text_style`]; the complete application layer outranks this
//! framework layer.

use crate::content::text::{HeadingLevel, TextSelector, TextTableSection};
use crate::{AnsiColor, ColorSpec, StyleSpec, ThemeColor};

use super::Theme;

pub(crate) fn framework_theme() -> Theme {
    Theme::new()
        .with_text_style(TextSelector::heading(), StyleSpec::new().bold())
        .with_text_style(
            TextSelector::heading().level(HeadingLevel::H1),
            StyleSpec::new().underline(),
        )
        .with_text_style(TextSelector::strong(), StyleSpec::new().bold())
        .with_text_style(TextSelector::emphasis(), StyleSpec::new().italic())
        .with_text_style(TextSelector::underline(), StyleSpec::new().underline())
        .with_text_style(TextSelector::link(), StyleSpec::new().underline())
        .with_text_style(
            TextSelector::strikethrough(),
            StyleSpec::new().strikethrough(),
        )
        .with_text_style(
            TextSelector::table_row().table_section(TextTableSection::Header),
            StyleSpec::new().bold(),
        )
        .with_color("diff.addition", ThemeColor::Named(AnsiColor::Green))
        .with_color("diff.deletion", ThemeColor::Named(AnsiColor::Red))
        .with_color("diff.context", ThemeColor::Named(AnsiColor::Gray))
        .with_color("diff.header", ThemeColor::Named(AnsiColor::Cyan))
        .with_color("diff.meta", ThemeColor::Named(AnsiColor::Gray))
        .with_style(
            "diff.addition",
            StyleSpec::new().foreground(ColorSpec::theme("diff.addition")),
        )
        .with_style(
            "diff.deletion",
            StyleSpec::new().foreground(ColorSpec::theme("diff.deletion")),
        )
        .with_style(
            "diff.context",
            StyleSpec::new().foreground(ColorSpec::theme("diff.context")),
        )
        .with_style(
            "diff.header",
            StyleSpec::new().foreground(ColorSpec::theme("diff.header")),
        )
        .with_style(
            "diff.meta",
            StyleSpec::new().foreground(ColorSpec::theme("diff.meta")),
        )
}
