//! Semantic style cascade and theme-key resolution into physical styles.

use crate::{
    Theme,
    physical::{AnsiColor as PhysicalAnsiColor, PhysicalColor, PhysicalStyle},
    presentation::api::{AnsiColor, ColorSpec, StyleRef, StyleSpec, ThemeColor},
};

#[derive(Debug, Clone)]
pub(crate) struct ThemeResolver {
    pub(crate) theme: Theme,
}

impl Default for ThemeResolver {
    fn default() -> Self {
        Self::new(&Theme::default())
    }
}

impl ThemeResolver {
    pub(crate) fn new(theme: &Theme) -> Self {
        Self {
            theme: theme.clone(),
        }
    }

    pub(crate) fn resolve_text_style(
        &self,
        inherited: PhysicalStyle,
        patch: &StyleRef,
    ) -> PhysicalStyle {
        let mut resolved = inherited;
        if let Some(key) = &patch.theme
            && let Some(named) = self.theme.style(key.as_str())
        {
            resolved = self.apply_style(resolved, named);
        }
        self.apply_style(resolved, &patch.local)
    }

    fn apply_style(&self, mut resolved: PhysicalStyle, patch: &StyleSpec) -> PhysicalStyle {
        if let Some(foreground) = &patch.foreground {
            resolved.foreground = Some(self.resolve_color(foreground));
        }
        if let Some(background) = &patch.background {
            resolved.background = Some(self.resolve_color(background));
        }
        if let Some(value) = patch.attributes.bold {
            resolved.bold = value;
        }
        if let Some(value) = patch.attributes.dim {
            resolved.dim = value;
        }
        if let Some(value) = patch.attributes.italic {
            resolved.italic = value;
        }
        if let Some(value) = patch.attributes.underline {
            resolved.underline = value;
        }
        if let Some(value) = patch.attributes.reversed {
            resolved.reversed = value;
        }
        resolved
    }

    pub(crate) fn resolve_color(&self, color: &ColorSpec) -> PhysicalColor {
        match color {
            ColorSpec::Ansi(value) => PhysicalColor::Indexed(*value),
            ColorSpec::Named(color) => PhysicalColor::Named(to_physical_ansi(*color)),
            ColorSpec::Rgb { r, g, b } => PhysicalColor::Rgb {
                r: *r,
                g: *g,
                b: *b,
            },
            ColorSpec::Theme(key) => self
                .theme
                .color(key.as_str())
                .map_or(PhysicalColor::Default, to_physical_color),
        }
    }
}

fn to_physical_color(color: ThemeColor) -> PhysicalColor {
    match color {
        ThemeColor::Default => PhysicalColor::Default,
        ThemeColor::Named(color) => PhysicalColor::Named(to_physical_ansi(color)),
        ThemeColor::Indexed(value) => PhysicalColor::Indexed(value),
        ThemeColor::Rgb { r, g, b } => PhysicalColor::Rgb { r, g, b },
    }
}

fn to_physical_ansi(color: AnsiColor) -> PhysicalAnsiColor {
    match color {
        AnsiColor::Black => PhysicalAnsiColor::Black,
        AnsiColor::Red => PhysicalAnsiColor::Red,
        AnsiColor::Green => PhysicalAnsiColor::Green,
        AnsiColor::Yellow => PhysicalAnsiColor::Yellow,
        AnsiColor::Blue => PhysicalAnsiColor::Blue,
        AnsiColor::Magenta => PhysicalAnsiColor::Magenta,
        AnsiColor::Cyan => PhysicalAnsiColor::Cyan,
        AnsiColor::Gray => PhysicalAnsiColor::Gray,
        AnsiColor::DarkGray => PhysicalAnsiColor::DarkGray,
        AnsiColor::LightRed => PhysicalAnsiColor::LightRed,
        AnsiColor::LightGreen => PhysicalAnsiColor::LightGreen,
        AnsiColor::LightYellow => PhysicalAnsiColor::LightYellow,
        AnsiColor::LightBlue => PhysicalAnsiColor::LightBlue,
        AnsiColor::LightMagenta => PhysicalAnsiColor::LightMagenta,
        AnsiColor::LightCyan => PhysicalAnsiColor::LightCyan,
        AnsiColor::White => PhysicalAnsiColor::White,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{IntoView, View, presentation::layout::compile_view_with_theme};

    #[test]
    fn named_color_and_style_resolve_with_local_override_precedence() {
        let theme = Theme::new()
            .with_color("accent", ThemeColor::Named(AnsiColor::Green))
            .with_style(
                "heading",
                StyleSpec::new()
                    .foreground(ColorSpec::theme("accent"))
                    .bold(),
            );
        let view = View::text("x")
            .style(StyleRef::themed(
                "heading",
                StyleSpec::new().foreground(ColorSpec::Named(AnsiColor::Red)),
            ))
            .into_view();
        let rows = compile_view_with_theme(&view, 10, &theme).rows;
        let style = rows[0].style_at(0).expect("painted style");
        assert_eq!(
            style.foreground,
            Some(PhysicalColor::Named(PhysicalAnsiColor::Red))
        );
        assert!(style.bold);
    }

    #[test]
    fn missing_tokens_lower_to_terminal_defaults_and_empty_named_styles() {
        let resolver = ThemeResolver::default();
        assert_eq!(
            resolver.resolve_color(&ColorSpec::theme("missing")),
            PhysicalColor::Default
        );
        assert_eq!(
            resolver.resolve_text_style(PhysicalStyle::default(), &StyleRef::theme("missing")),
            PhysicalStyle::default()
        );
    }

    #[test]
    fn theme_is_paint_only() {
        let view = View::text("x")
            .style(StyleSpec::new().foreground(ColorSpec::theme("accent")))
            .into_view();
        let plain = compile_view_with_theme(&view, 1, &Theme::default());
        let colored = compile_view_with_theme(
            &view,
            1,
            &Theme::new().with_color("accent", ThemeColor::Indexed(42)),
        );
        assert_eq!(plain.rows.len(), colored.rows.len());
        assert_eq!(plain.rows[0].plain_text(), colored.rows[0].plain_text());
        assert_ne!(plain.rows[0].style_at(0), colored.rows[0].style_at(0));
    }
}
