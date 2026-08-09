//! Backend-neutral semantic styling and decoration vocabulary.

/// FEATURE EXTENSION API. Vertical alignment for row children.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum VerticalAlign {
    #[default]
    Top,
    Center,
    Bottom,
}
/// FEATURE EXTENSION API. Sparse semantic text-style intent, independent of
/// Ratatui. Unspecified fields inherit from the preceding cascade layer.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct StyleSpec {
    pub(crate) foreground: Option<ColorSpec>,
    pub(crate) background: Option<ColorSpec>,
    pub(crate) attributes: TextAttributeSpec,
}

impl StyleSpec {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn foreground(mut self, color: ColorSpec) -> Self {
        self.foreground = Some(color);
        self
    }

    pub(crate) fn background(mut self, color: ColorSpec) -> Self {
        self.background = Some(color);
        self
    }

    pub(crate) fn bold(self) -> Self {
        self.attribute(TextAttribute::Bold, true)
    }

    pub(crate) fn dim(self) -> Self {
        self.attribute(TextAttribute::Dim, true)
    }

    pub(crate) fn italic(self) -> Self {
        self.attribute(TextAttribute::Italic, true)
    }

    pub(crate) fn underline(self) -> Self {
        self.attribute(TextAttribute::Underline, true)
    }

    pub(crate) fn reversed(self) -> Self {
        self.attribute(TextAttribute::Reversed, true)
    }

    pub(crate) fn attribute(mut self, attribute: TextAttribute, enabled: bool) -> Self {
        self.attributes.set(attribute, enabled);
        self
    }

    /// Applies the explicitly specified fields from `incoming`; it is the
    /// more-specific patch and never clears unspecified fields.
    pub(crate) fn overlay(&mut self, incoming: &Self) {
        if incoming.foreground.is_some() {
            self.foreground = incoming.foreground.clone();
        }
        if incoming.background.is_some() {
            self.background = incoming.background.clone();
        }
        self.attributes.overlay(incoming.attributes);
    }
}
/// FEATURE EXTENSION API. Node insets; padding belongs to the decorated View,
/// not its structural child.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Insets {
    pub(crate) top: u16,
    pub(crate) right: u16,
    pub(crate) bottom: u16,
    pub(crate) left: u16,
}

impl Insets {
    pub(crate) const ZERO: Self = Self {
        top: 0,
        right: 0,
        bottom: 0,
        left: 0,
    };

    pub(crate) const fn all(value: u16) -> Self {
        Self {
            top: value,
            right: value,
            bottom: value,
            left: value,
        }
    }

    pub(crate) const fn vertical(value: u16) -> Self {
        Self {
            top: value,
            bottom: value,
            ..Self::ZERO
        }
    }
}
/// FEATURE EXTENSION API. Theme-resolved or explicit terminal-compatible color.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ColorSpec {
    Theme(ThemeKey),
    Ansi(u8),
    Rgb { r: u8, g: u8, b: u8 },
}

/// FEATURE EXTENSION API. Opaque host theme token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ThemeKey(pub(crate) String);

impl From<&str> for ThemeKey {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}
/// FEATURE EXTENSION API. Sparse text-attribute intent.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct TextAttributeSpec {
    pub(crate) bold: Option<bool>,
    pub(crate) dim: Option<bool>,
    pub(crate) italic: Option<bool>,
    pub(crate) underline: Option<bool>,
    pub(crate) reversed: Option<bool>,
}

impl TextAttributeSpec {
    fn set(&mut self, attribute: TextAttribute, enabled: bool) {
        match attribute {
            TextAttribute::Bold => self.bold = Some(enabled),
            TextAttribute::Dim => self.dim = Some(enabled),
            TextAttribute::Italic => self.italic = Some(enabled),
            TextAttribute::Underline => self.underline = Some(enabled),
            TextAttribute::Reversed => self.reversed = Some(enabled),
        }
    }

    fn overlay(&mut self, incoming: Self) {
        if incoming.bold.is_some() {
            self.bold = incoming.bold;
        }
        if incoming.dim.is_some() {
            self.dim = incoming.dim;
        }
        if incoming.italic.is_some() {
            self.italic = incoming.italic;
        }
        if incoming.underline.is_some() {
            self.underline = incoming.underline;
        }
        if incoming.reversed.is_some() {
            self.reversed = incoming.reversed;
        }
    }
}

/// FEATURE EXTENSION API. Semantic text-attribute selector.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextAttribute {
    Bold,
    Dim,
    Italic,
    Underline,
    Reversed,
}

/// FEATURE EXTENSION API. Generic border description.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BorderSpec {
    pub(crate) style: BorderStyle,
    pub(crate) color: Option<ColorSpec>,
}

/// FEATURE EXTENSION API. Terminal border families, independent of Ratatui.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum BorderStyle {
    #[default]
    Plain,
    Rounded,
    Double,
}
/// FEATURE EXTENSION API. Indicator for a clamped view.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum OverflowIndicator {
    None,
    Ellipsis { style: StyleSpec },
    Footer { prefix: String, style: StyleSpec },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sparse_style_overlay_preserves_unspecified_fields_and_allows_false() {
        let mut existing = StyleSpec::new().foreground(ColorSpec::Ansi(1)).bold();
        existing.overlay(&StyleSpec::new().italic());
        assert_eq!(existing.foreground, Some(ColorSpec::Ansi(1)));
        assert_eq!(existing.attributes.bold, Some(true));
        assert_eq!(existing.attributes.italic, Some(true));

        existing.overlay(&StyleSpec::new().attribute(TextAttribute::Bold, false));
        assert_eq!(existing.attributes.bold, Some(false));
    }
}
