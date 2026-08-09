//! Semantic style cascade and theme-key resolution into physical styles.

use crate::{
    physical::{PhysicalColor, PhysicalStyle},
    presentation::api::{ColorSpec, StyleSpec},
    theme,
};

#[derive(Debug, Default)]
pub(crate) struct ThemeResolver;

impl ThemeResolver {
    pub(crate) fn resolve_text_style(
        &self,
        inherited: PhysicalStyle,
        patch: &StyleSpec,
    ) -> PhysicalStyle {
        let mut resolved = inherited;
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
            ColorSpec::Rgb { r, g, b } => PhysicalColor::Rgb {
                r: *r,
                g: *g,
                b: *b,
            },
            ColorSpec::Theme(key) => theme::physical_color(key.0.as_str()),
        }
    }
}
