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

    pub(crate) const fn horizontal(value: u16) -> Self {
        Self {
            right: value,
            left: value,
            ..Self::ZERO
        }
    }

    /// Creates insets in top, right, bottom, left order.
    pub(crate) const fn new(top: u16, right: u16, bottom: u16, left: u16) -> Self {
        Self {
            top,
            right,
            bottom,
            left,
        }
    }
}

impl From<u16> for Insets {
    fn from(value: u16) -> Self {
        Self::all(value)
    }
}
/// FEATURE EXTENSION API. Theme-resolved or explicit terminal-compatible color.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ColorSpec {
    Theme(ThemeKey),
    Ansi(u8),
    Rgb { r: u8, g: u8, b: u8 },
}

impl ColorSpec {
    pub(crate) fn theme(key: impl Into<ThemeKey>) -> Self {
        Self::Theme(key.into())
    }

    pub(crate) const fn ansi(value: u8) -> Self {
        Self::Ansi(value)
    }

    pub(crate) const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self::Rgb { r, g, b }
    }
}

/// FEATURE EXTENSION API. Opaque host theme token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ThemeKey(pub(crate) String);

impl From<&str> for ThemeKey {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}

impl From<String> for ThemeKey {
    fn from(value: String) -> Self {
        Self(value)
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
    pub(crate) fn set(&mut self, attribute: TextAttribute, enabled: bool) {
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

impl BorderSpec {
    pub(crate) fn plain() -> Self {
        Self {
            style: BorderStyle::Plain,
            color: None,
        }
    }

    pub(crate) fn rounded() -> Self {
        Self {
            style: BorderStyle::Rounded,
            color: None,
        }
    }

    pub(crate) fn double() -> Self {
        Self {
            style: BorderStyle::Double,
            color: None,
        }
    }

    pub(crate) fn color(mut self, color: ColorSpec) -> Self {
        self.color = Some(color);
        self
    }
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

    #[test]
    fn semantic_style_primitive_constructors_lower_to_existing_values() {
        assert_eq!(Insets::horizontal(2), Insets::new(0, 2, 0, 2));
        assert_eq!(Insets::from(3), Insets::all(3));
        assert_eq!(ColorSpec::ansi(3), ColorSpec::Ansi(3));
        assert_eq!(ColorSpec::rgb(1, 2, 3), ColorSpec::Rgb { r: 1, g: 2, b: 3 });
        assert_eq!(
            ColorSpec::theme("text.muted"),
            ColorSpec::Theme(ThemeKey::from("text.muted"))
        );
        assert_eq!(
            ColorSpec::theme(String::from("text.muted")),
            ColorSpec::Theme(ThemeKey::from("text.muted")),
        );
    }

    #[test]
    fn border_constructors_set_style_and_replaceable_color() {
        assert_eq!(
            BorderSpec::plain(),
            BorderSpec {
                style: BorderStyle::Plain,
                color: None
            }
        );
        assert_eq!(BorderSpec::rounded().style, BorderStyle::Rounded);
        assert_eq!(BorderSpec::double().style, BorderStyle::Double);
        assert_eq!(
            BorderSpec::rounded()
                .color(ColorSpec::ansi(2))
                .color(ColorSpec::ansi(3)),
            BorderSpec {
                style: BorderStyle::Rounded,
                color: Some(ColorSpec::ansi(3))
            },
        );
    }
}
