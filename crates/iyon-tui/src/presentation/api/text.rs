//! Typed semantic text construction backed by the canonical View IR.

use super::style::StyleSpec;
use crate::presentation::ir::{Decoration, TextView, View, ViewKind, WidthRule};

/// FEATURE EXTENSION API. A semantic text span.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextSpan {
    pub(crate) text: String,
    pub(crate) style: StyleSpec,
}

impl TextSpan {
    pub(crate) fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            style: StyleSpec::default(),
        }
    }

    pub(crate) fn styled(text: impl Into<String>, style: StyleSpec) -> Self {
        Self {
            text: text.into(),
            style,
        }
    }
}

/// FEATURE EXTENSION API. Generic text wrapping behavior.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum WrapMode {
    #[default]
    WordThenGrapheme,
    Grapheme,
    NoWrap,
}

/// FEATURE EXTENSION API. Horizontal alignment inside an allocated track.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum HorizontalAlign {
    #[default]
    Start,
    Center,
    End,
}

/// FEATURE EXTENSION API. Typed semantic text leaf backed by the canonical
/// [`View`] representation. Its private field preserves the `Text` invariant:
/// the wrapped view always contains `ViewKind::Text`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Text {
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

    pub(crate) fn wrap(mut self, wrap: WrapMode) -> Self {
        self.text_mut().wrap = wrap;
        self
    }

    pub(crate) fn no_wrap(mut self) -> Self {
        self.text_mut().wrap = WrapMode::NoWrap;
        self
    }

    pub(crate) fn text_align(mut self, align: HorizontalAlign) -> Self {
        self.text_mut().align = align;
        self
    }

    pub(crate) fn style(mut self, style: StyleSpec) -> Self {
        self.view.decoration.text_style.overlay(&style);
        self
    }

    pub(crate) fn fit_width(mut self) -> Self {
        self.view.width = WidthRule::Fit;
        self
    }

    pub(crate) fn fill_width(mut self) -> Self {
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
    use crate::presentation::api::style::ColorSpec;

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
}
