//! Centralized theme: every color / modifier used by the TUI lives here so the
//! palette can be tuned in one place and markdown accents stay coherent with the
//! tool / harness colors. All constructors are functions (ratatui `Style` is not
//! `const`-constructible) named for *what they style*, not *where* they're used.

use ratatui::style::{Color, Modifier, Style};

/// Reasoning ("thinking") segments: muted, italic background trace.
pub(crate) fn thinking() -> Style {
    Style::default()
        .fg(Color::Rgb(113, 128, 150))
        .add_modifier(Modifier::ITALIC)
}

/// User message bubble background.
pub(crate) fn user_bubble_bg() -> Color {
    Color::Rgb(45, 55, 72)
}

/// Neutral/muted text (e.g. tool result bodies, dim context).
pub(crate) fn muted() -> Style {
    Style::default().fg(Color::Rgb(113, 128, 150))
}

/// Truncation footer ("… N more lines").
pub(crate) fn truncation_footer() -> Style {
    Style::default()
        .fg(Color::Rgb(120, 122, 132))
        .add_modifier(Modifier::DIM | Modifier::ITALIC)
}

/// Tool / command finished (successful) accent.
pub(crate) fn tool_finished() -> Style {
    Style::default().fg(Color::Rgb(104, 211, 145))
}

/// Tool / command currently running (muted neutral).
pub(crate) fn tool_running() -> Style {
    Style::default().fg(Color::Rgb(160, 174, 192))
}

/// Tool / command failure or rejection.
pub(crate) fn tool_error() -> Style {
    Style::default().fg(Color::Red)
}

/// Composer input border.
pub(crate) fn input_border() -> Style {
    Style::default().fg(Color::Rgb(173, 216, 230))
}

// ---------------------------------------------------------------------------
// Markdown rendering accents. Kept in the theme so they harmonize with the
// tools above (note `tool_finished` is reused for list bullets).
// ---------------------------------------------------------------------------

/// Markdown heading marker (`##`) accent. Headings stay compact: the `#`s are
/// kept visible and tinted, not enlarged.
pub(crate) fn markdown_header() -> Style {
    Style::default().fg(Color::Rgb(255, 196, 87))
}

/// Markdown emphasis (bold) — brightened strong text.
pub(crate) fn markdown_bold() -> Style {
    Style::default()
        .fg(Color::Rgb(230, 230, 235))
        .add_modifier(Modifier::BOLD)
}

/// Markdown emphasis (italic) — distinct muted-cursive.
pub(crate) fn markdown_italic() -> Style {
    Style::default()
        .fg(Color::Rgb(150, 180, 220))
        .add_modifier(Modifier::ITALIC)
}

/// Markdown inline code / literal span accent.
pub(crate) fn markdown_code() -> Style {
    Style::default().fg(Color::Rgb(120, 200, 210))
}

/// Markdown list bullet accent (reuses the successful-tool green so lists read
/// as part of the same accent family).
pub(crate) fn markdown_list() -> Style {
    Style::default().fg(Color::Rgb(104, 211, 145))
}
