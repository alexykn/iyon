//! Backend-neutral semantic styling and decoration vocabulary.

use std::{error::Error, fmt};

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// Vertical alignment for children in a horizontal composition.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum VerticalAlign {
    #[default]
    Top,
    Center,
    Bottom,
}
/// Sparse backend-neutral text-style intent. Unspecified fields inherit from
/// the preceding cascade layer.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StyleSpec {
    pub(crate) foreground: Option<ColorSpec>,
    pub(crate) background: Option<ColorSpec>,
    pub(crate) attributes: TextAttributeSpec,
}

impl StyleSpec {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn foreground(mut self, color: ColorSpec) -> Self {
        self.foreground = Some(color);
        self
    }

    pub fn background(mut self, color: ColorSpec) -> Self {
        self.background = Some(color);
        self
    }

    pub fn bold(self) -> Self {
        self.attribute(TextAttribute::Bold, true)
    }

    pub fn dim(self) -> Self {
        self.attribute(TextAttribute::Dim, true)
    }

    pub fn italic(self) -> Self {
        self.attribute(TextAttribute::Italic, true)
    }

    pub fn underline(self) -> Self {
        self.attribute(TextAttribute::Underline, true)
    }

    pub fn reversed(self) -> Self {
        self.attribute(TextAttribute::Reversed, true)
    }

    pub fn attribute(mut self, attribute: TextAttribute, enabled: bool) -> Self {
        self.attributes.set(attribute, enabled);
        self
    }

    /// Applies the explicitly specified fields from `incoming`; it is the
    /// more-specific patch and never clears unspecified fields.
    pub(in crate::presentation::api) fn overlay(&mut self, incoming: &Self) {
        if incoming.foreground.is_some() {
            self.foreground = incoming.foreground.clone();
        }
        if incoming.background.is_some() {
            self.background = incoming.background.clone();
        }
        self.attributes.overlay(incoming.attributes);
    }
}
/// Insets applied to a semantic view's surface. Padding belongs to the
/// decorated view, not its structural child.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Insets {
    pub(crate) top: u16,
    pub(crate) right: u16,
    pub(crate) bottom: u16,
    pub(crate) left: u16,
}

impl Insets {
    pub const ZERO: Self = Self {
        top: 0,
        right: 0,
        bottom: 0,
        left: 0,
    };

    pub const fn all(value: u16) -> Self {
        Self {
            top: value,
            right: value,
            bottom: value,
            left: value,
        }
    }

    pub const fn vertical(value: u16) -> Self {
        Self {
            top: value,
            bottom: value,
            ..Self::ZERO
        }
    }

    pub const fn horizontal(value: u16) -> Self {
        Self {
            right: value,
            left: value,
            ..Self::ZERO
        }
    }

    /// Creates insets in top, right, bottom, left order.
    pub const fn new(top: u16, right: u16, bottom: u16, left: u16) -> Self {
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
/// Backend-neutral theme, ANSI, or RGB color specification.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ColorSpec {
    Theme(ThemeKey),
    Ansi(u8),
    Rgb { r: u8, g: u8, b: u8 },
}

impl ColorSpec {
    pub fn theme(key: impl Into<ThemeKey>) -> Self {
        Self::Theme(key.into())
    }

    pub const fn ansi(value: u8) -> Self {
        Self::Ansi(value)
    }

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self::Rgb { r, g, b }
    }
}

/// Opaque semantic key resolved by the host theme.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThemeKey(pub(crate) String);

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
/// Sparse text-attribute intent used by semantic style patches.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextAttributeSpec {
    pub(crate) bold: Option<bool>,
    pub(crate) dim: Option<bool>,
    pub(crate) italic: Option<bool>,
    pub(crate) underline: Option<bool>,
    pub(crate) reversed: Option<bool>,
}

impl TextAttributeSpec {
    pub(in crate::presentation::api) fn set(&mut self, attribute: TextAttribute, enabled: bool) {
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

/// Selects a sparse semantic text attribute.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextAttribute {
    Bold,
    Dim,
    Italic,
    Underline,
    Reversed,
}

/// Which sides of a semantic border are painted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BorderEdges {
    pub(crate) top: bool,
    pub(crate) right: bool,
    pub(crate) bottom: bool,
    pub(crate) left: bool,
}

impl BorderEdges {
    pub const NONE: Self = Self {
        top: false,
        right: false,
        bottom: false,
        left: false,
    };

    pub const ALL: Self = Self {
        top: true,
        right: true,
        bottom: true,
        left: true,
    };

    pub const TOP_BOTTOM: Self = Self {
        top: true,
        right: false,
        bottom: true,
        left: false,
    };

    pub const fn new(top: bool, right: bool, bottom: bool, left: bool) -> Self {
        Self {
            top,
            right,
            bottom,
            left,
        }
    }
}

/// Failure to construct a border glyph that occupies exactly one cell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BorderGlyphError {
    pub field: &'static str,
    pub width: usize,
    pub graphemes: usize,
}

impl fmt::Display for BorderGlyphError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "border glyph `{}` must contain one grapheme with width one (got {} graphemes, width {})",
            self.field, self.graphemes, self.width
        )
    }
}

impl Error for BorderGlyphError {}

/// Custom one-cell border glyphs. Applications can use ASCII, Unicode box
/// drawing, or another backend-supported pattern.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BorderGlyphs {
    pub(crate) top: String,
    pub(crate) right: String,
    pub(crate) bottom: String,
    pub(crate) left: String,
    pub(crate) top_left: String,
    pub(crate) top_right: String,
    pub(crate) bottom_left: String,
    pub(crate) bottom_right: String,
}

impl BorderGlyphs {
    pub fn new(
        top: impl Into<String>,
        right: impl Into<String>,
        bottom: impl Into<String>,
        left: impl Into<String>,
        top_left: impl Into<String>,
        top_right: impl Into<String>,
        bottom_left: impl Into<String>,
        bottom_right: impl Into<String>,
    ) -> Result<Self, BorderGlyphError> {
        let top = top.into();
        let right = right.into();
        let bottom = bottom.into();
        let left = left.into();
        let top_left = top_left.into();
        let top_right = top_right.into();
        let bottom_left = bottom_left.into();
        let bottom_right = bottom_right.into();
        for (field, glyph) in [
            ("top", &top),
            ("right", &right),
            ("bottom", &bottom),
            ("left", &left),
            ("top_left", &top_left),
            ("top_right", &top_right),
            ("bottom_left", &bottom_left),
            ("bottom_right", &bottom_right),
        ] {
            let graphemes = glyph.graphemes(true).count();
            let width = UnicodeWidthStr::width(glyph.as_str());
            if graphemes != 1 || width != 1 {
                return Err(BorderGlyphError {
                    field,
                    width,
                    graphemes,
                });
            }
        }
        Ok(Self {
            top,
            right,
            bottom,
            left,
            top_left,
            top_right,
            bottom_left,
            bottom_right,
        })
    }

    pub(crate) fn plain() -> Self {
        Self::new("─", "│", "─", "│", "┌", "┐", "└", "┘")
            .expect("built-in border glyphs are one-cell")
    }

    pub(crate) fn rounded() -> Self {
        Self::new("─", "│", "─", "│", "╭", "╮", "╰", "╯")
            .expect("built-in border glyphs are one-cell")
    }

    pub(crate) fn double() -> Self {
        Self::new("═", "║", "═", "║", "╔", "╗", "╚", "╝")
            .expect("built-in border glyphs are one-cell")
    }
}

/// Backend-neutral border description. Edges are independently optional;
/// corners are painted only when both adjacent edges are enabled.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BorderSpec {
    pub(crate) style: BorderStyle,
    pub(crate) color: Option<ColorSpec>,
    pub(crate) edges: BorderEdges,
    pub(crate) glyphs: BorderGlyphs,
    pub(crate) top_label: Option<String>,
}

impl BorderSpec {
    pub fn plain() -> Self {
        Self {
            style: BorderStyle::Plain,
            color: None,
            edges: BorderEdges::ALL,
            glyphs: BorderGlyphs::plain(),
            top_label: None,
        }
    }

    pub fn rounded() -> Self {
        Self {
            style: BorderStyle::Rounded,
            color: None,
            edges: BorderEdges::ALL,
            glyphs: BorderGlyphs::rounded(),
            top_label: None,
        }
    }

    pub fn double() -> Self {
        Self {
            style: BorderStyle::Double,
            color: None,
            edges: BorderEdges::ALL,
            glyphs: BorderGlyphs::double(),
            top_label: None,
        }
    }

    pub fn custom(glyphs: BorderGlyphs) -> Self {
        Self {
            style: BorderStyle::Plain,
            color: None,
            edges: BorderEdges::ALL,
            glyphs,
            top_label: None,
        }
    }

    pub fn edges(mut self, edges: BorderEdges) -> Self {
        self.edges = edges;
        self
    }

    pub fn color(mut self, color: ColorSpec) -> Self {
        self.color = Some(color);
        self
    }

    /// Places a semantic label over the top edge without changing geometry.
    pub fn top_label(mut self, label: impl Into<String>) -> Self {
        self.top_label = Some(label.into());
        self
    }

    pub(crate) fn left_width(&self) -> u16 {
        u16::from(self.edges.left)
    }

    pub(crate) fn right_width(&self) -> u16 {
        u16::from(self.edges.right)
    }

    pub(crate) fn top_height(&self) -> u16 {
        u16::from(self.edges.top)
    }

    pub(crate) fn bottom_height(&self) -> u16 {
        u16::from(self.edges.bottom)
    }
}

/// Terminal-independent border family used by the convenience constructors.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BorderStyle {
    #[default]
    Plain,
    Rounded,
    Double,
}
/// Overflow treatment for a structurally clamped view.
#[derive(Clone, Debug, PartialEq)]
pub enum OverflowIndicator {
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
                color: None,
                edges: BorderEdges::ALL,
                glyphs: BorderGlyphs::plain(),
                top_label: None,
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
                color: Some(ColorSpec::ansi(3)),
                edges: BorderEdges::ALL,
                glyphs: BorderGlyphs::rounded(),
                top_label: None,
            },
        );
    }
}
