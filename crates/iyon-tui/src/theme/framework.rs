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

use crate::StyleSpec;
use crate::document::{HeadingLevel, TextSelector, TextTableSection};

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
}
