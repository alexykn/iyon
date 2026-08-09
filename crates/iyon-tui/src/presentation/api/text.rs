//! Typed semantic text construction backed by the canonical View IR.

use super::style::{BorderSpec, ColorSpec, Insets, OverflowIndicator, StyleSpec, TextAttribute};
use crate::presentation::ir::{Decoration, TextView, View, ViewKind, WidthRule};

/// A semantic text span with optional text-cell styling.
#[derive(Clone, Debug, PartialEq)]
pub struct TextSpan {
    pub(crate) text: String,
    pub(crate) style: StyleSpec,
}

impl TextSpan {
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            style: StyleSpec::default(),
        }
    }

    pub fn styled(text: impl Into<String>, style: StyleSpec) -> Self {
        Self {
            text: text.into(),
            style,
        }
    }
}

/// Text wrapping behavior for a typed text view.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WrapMode {
    #[default]
    WordThenGrapheme,
    Grapheme,
    NoWrap,
}

/// Horizontal alignment inside an allocated text track.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HorizontalAlign {
    #[default]
    Start,
    Center,
    End,
}

/// Typed backend-neutral text construction backed by the crate's owned
/// semantic [`View`]. Ordinary properties preserve `Text`; structural
/// transforms return a general `View`.
#[derive(Clone, Debug, PartialEq)]
pub struct Text {
    view: View,
}

impl Text {
    pub(super) fn plain(text: impl Into<String>) -> Self {
        Self::from_text_view(TextView::plain(text))
    }

    pub(super) fn styled(spans: impl IntoIterator<Item = TextSpan>) -> Self {
        Self::from_text_view(TextView {
            spans: spans.into_iter().collect(),
            wrap: WrapMode::WordThenGrapheme,
            align: HorizontalAlign::Start,
        })
    }

    fn from_text_view(text: TextView) -> Self {
        Self {
            view: View {
                width: WidthRule::Fit,
                decoration: Decoration::default(),
                kind: ViewKind::Text(text),
            },
        }
    }

    pub(super) fn into_canonical_view(self) -> View {
        self.view
    }

    fn text_mut(&mut self) -> &mut TextView {
        match &mut self.view.kind {
            ViewKind::Text(text) => text,
            ViewKind::Column(_)
            | ViewKind::Row(_)
            | ViewKind::Container(_)
            | ViewKind::Spacer { .. }
            | ViewKind::ClampRows(_) => {
                unreachable!("Text wrapper must always contain ViewKind::Text")
            }
        }
    }

    pub fn wrap(mut self, wrap: WrapMode) -> Self {
        self.text_mut().wrap = wrap;
        self
    }

    pub fn no_wrap(mut self) -> Self {
        self.text_mut().wrap = WrapMode::NoWrap;
        self
    }

    pub fn text_align(mut self, align: HorizontalAlign) -> Self {
        self.text_mut().align = align;
        self
    }

    pub fn style(mut self, style: StyleSpec) -> Self {
        self.view.decoration.text_style.overlay(&style);
        self
    }

    /// Sets the current text node's padding; repeated calls replace the prior value.
    pub fn padding(mut self, padding: impl Into<Insets>) -> Self {
        self.view.decoration.padding = padding.into();
        self
    }

    /// Paints the text node's allocated surface, not its text-cell style.
    pub fn background(mut self, color: ColorSpec) -> Self {
        self.view.decoration.surface_background = Some(color);
        self
    }

    /// Sets inherited foreground intent for this text node.
    pub fn foreground(mut self, color: ColorSpec) -> Self {
        self.view.decoration.text_style.foreground = Some(color);
        self
    }

    /// Replaces the text node's complete border specification.
    pub fn border(mut self, border: BorderSpec) -> Self {
        self.view.decoration.border = Some(border);
        self
    }

    /// Sets sparse text-attribute intent, including explicit false.
    pub fn text_attribute(mut self, attribute: TextAttribute, enabled: bool) -> Self {
        self.view
            .decoration
            .text_style
            .attributes
            .set(attribute, enabled);
        self
    }

    pub fn bold(self) -> Self {
        self.text_attribute(TextAttribute::Bold, true)
    }

    pub fn dim(self) -> Self {
        self.text_attribute(TextAttribute::Dim, true)
    }

    pub fn italic(self) -> Self {
        self.text_attribute(TextAttribute::Italic, true)
    }

    pub fn underline(self) -> Self {
        self.text_attribute(TextAttribute::Underline, true)
    }

    pub fn reversed(self) -> Self {
        self.text_attribute(TextAttribute::Reversed, true)
    }

    pub fn container(self) -> View {
        self.into_canonical_view().container()
    }

    pub fn clamp_rows(self, max_rows: u16, overflow: OverflowIndicator) -> View {
        self.into_canonical_view().clamp_rows(max_rows, overflow)
    }

    pub fn fit_width(mut self) -> Self {
        self.view.width = WidthRule::Fit;
        self
    }

    pub fn fill_width(mut self) -> Self {
        self.view.width = WidthRule::Fill;
        self
    }

    /// Migration-only internal sizing method. Replaced by `fit_width` and
    /// `fill_width` in the final semantic API.
    pub(crate) fn width(mut self, width: WidthRule) -> Self {
        self.view.width = width;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::super::view::IntoView;
    use super::*;
    use crate::presentation::api::style::{ColorSpec, OverflowIndicator, TextAttribute};

    #[test]
    fn text_style_merges_node_intent_without_rewriting_spans() {
        let mut text = View::styled_text([
            TextSpan::plain("plain"),
            TextSpan::styled("bold", StyleSpec::new().bold()),
        ])
        .style(StyleSpec::new().foreground(ColorSpec::Ansi(1)))
        .style(StyleSpec::new().italic());

        assert_eq!(
            text.view.decoration.text_style.foreground,
            Some(ColorSpec::Ansi(1))
        );
        assert_eq!(
            text.view.decoration.text_style.attributes.italic,
            Some(true)
        );
        let ViewKind::Text(text_view) = &mut text.view.kind else {
            panic!("expected text view");
        };
        assert_eq!(text_view.spans[0].style, StyleSpec::default());
        assert_eq!(text_view.spans[1].style.attributes.bold, Some(true));

        let converted = text.into_view();
        assert!(matches!(converted.kind, ViewKind::Text(_)));
    }

    #[test]
    fn typed_text_methods_update_only_canonical_text_payload() {
        let text = View::text("abcdef")
            .wrap(WrapMode::Grapheme)
            .text_align(HorizontalAlign::End)
            .style(StyleSpec::new().foreground(ColorSpec::Ansi(3)))
            .fill_width();

        assert_eq!(text.view.width, WidthRule::Fill);
        assert_eq!(
            text.view.decoration.text_style.foreground,
            Some(ColorSpec::Ansi(3))
        );
        let ViewKind::Text(text_view) = &text.view.kind else {
            panic!("expected text view");
        };
        assert_eq!(text_view.wrap, WrapMode::Grapheme);
        assert_eq!(text_view.align, HorizontalAlign::End);
        assert_eq!(text_view.spans[0].style, StyleSpec::default());
    }

    #[test]
    fn typed_width_modifiers_preserve_text_and_last_write_wins() {
        let text = View::text("x")
            .fill_width()
            .no_wrap()
            .text_align(HorizontalAlign::End)
            .fit_width();

        assert_eq!(text.view.width, WidthRule::Fit);
        assert!(matches!(text.view.kind, ViewKind::Text(_)));
        assert_eq!(text.view.decoration, Decoration::default());
    }

    #[test]
    fn style_and_specific_text_properties_merge_as_sparse_patches() {
        let text = View::text("x")
            .bold()
            .style(StyleSpec::new().attribute(TextAttribute::Bold, false))
            .foreground(ColorSpec::ansi(1))
            .style(StyleSpec::new().italic())
            .bold();

        assert_eq!(
            text.view.decoration.text_style.foreground,
            Some(ColorSpec::ansi(1))
        );
        assert_eq!(text.view.decoration.text_style.attributes.bold, Some(true));
        assert_eq!(
            text.view.decoration.text_style.attributes.italic,
            Some(true)
        );
    }

    #[test]
    fn structural_text_transforms_return_general_views() {
        let container = View::text("x").container();
        let clamp = View::text("x").clamp_rows(1, OverflowIndicator::None);

        assert!(matches!(container.kind, ViewKind::Container(_)));
        assert!(matches!(clamp.kind, ViewKind::ClampRows(_)));
    }
}
