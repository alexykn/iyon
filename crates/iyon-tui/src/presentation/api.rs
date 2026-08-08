//! Generic semantic presentation vocabulary.
//!
//! This module must remain independent of Ratatui and Iyon feature concepts.
//! Features describe owned view trees; the presentation compiler owns width,
//! wrapping, coordinates, and terminal integration.

use std::{any::Any, fmt::Debug, time::Instant};

/// FEATURE EXTENSION API.
///
/// A pure, owned declarative presentation tree. It contains no terminal state,
/// callbacks, coordinates, or physical row calculations.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct View {
    pub(crate) width: WidthRule,
    pub(crate) decoration: Decoration,
    pub(crate) kind: ViewKind,
}

impl View {
    pub(crate) fn text(text: impl Into<String>) -> Self {
        Self {
            width: WidthRule::Fit,
            decoration: Decoration::default(),
            kind: ViewKind::Text(TextView::plain(text)),
        }
    }

    pub(crate) fn styled_text(spans: Vec<TextSpan>) -> Self {
        Self {
            width: WidthRule::Fit,
            decoration: Decoration::default(),
            kind: ViewKind::Text(TextView {
                spans,
                wrap: WrapMode::WordThenGrapheme,
                align: HorizontalAlign::Start,
            }),
        }
    }

    pub(crate) fn column(children: Vec<View>, gap: u16) -> Self {
        Self {
            width: WidthRule::Fit,
            decoration: Decoration::default(),
            kind: ViewKind::Column(ColumnView { children, gap }),
        }
    }

    pub(crate) fn row(children: Vec<RowChild>, gap: u16) -> Self {
        Self {
            width: WidthRule::Fill,
            decoration: Decoration::default(),
            kind: ViewKind::Row(RowView {
                children,
                gap,
                vertical_align: VerticalAlign::Top,
            }),
        }
    }

    pub(crate) fn box_(child: View, decoration: Decoration) -> Self {
        let width = child.width;
        Self {
            width,
            decoration,
            kind: ViewKind::Container(ContainerNode {
                child: Box::new(child),
            }),
        }
    }

    pub(crate) fn spacer(rows: u16) -> Self {
        Self {
            width: WidthRule::Fit,
            decoration: Decoration::default(),
            kind: ViewKind::Spacer { rows },
        }
    }

    pub(crate) fn clamp_rows(child: View, max_rows: u16, overflow: OverflowIndicator) -> Self {
        Self {
            width: child.width,
            decoration: Decoration::default(),
            kind: ViewKind::ClampRows(ClampRowsView {
                child: Box::new(child),
                max_rows,
                overflow,
            }),
        }
    }

    pub(crate) fn width(mut self, width: WidthRule) -> Self {
        self.width = width;
        self
    }

    pub(crate) fn no_wrap(mut self) -> Self {
        if let ViewKind::Text(text) = &mut self.kind {
            text.wrap = WrapMode::NoWrap;
        }
        self
    }

    pub(crate) fn wrap(mut self, wrap: WrapMode) -> Self {
        if let ViewKind::Text(text) = &mut self.kind {
            text.wrap = wrap;
        }
        self
    }

    pub(crate) fn style(mut self, style: StyleSpec) -> Self {
        if matches!(self.kind, ViewKind::Text(_)) {
            self.decoration.text_style.overlay(&style);
        }
        self
    }
}

/// FEATURE EXTENSION API. Generic view node kinds understood by the compiler.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ViewKind {
    Text(TextView),
    Column(ColumnView),
    Row(RowView),
    Container(ContainerNode),
    Spacer { rows: u16 },
    ClampRows(ClampRowsView),
}

/// FEATURE EXTENSION API. Width allocation requested from a parent.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum WidthRule {
    #[default]
    Fit,
    Fill,
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

/// FEATURE EXTENSION API. Styled text, represented without terminal types.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextView {
    pub(crate) spans: Vec<TextSpan>,
    pub(crate) wrap: WrapMode,
    pub(crate) align: HorizontalAlign,
}

impl TextView {
    pub(crate) fn plain(text: impl Into<String>) -> Self {
        Self {
            spans: vec![TextSpan::plain(text)],
            wrap: WrapMode::WordThenGrapheme,
            align: HorizontalAlign::Start,
        }
    }
}

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

/// FEATURE EXTENSION API. Vertical composition. The parent owns sibling gaps.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ColumnView {
    pub(crate) children: Vec<View>,
    pub(crate) gap: u16,
}

/// FEATURE EXTENSION API. Horizontal composition. The parent owns sibling gaps.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RowView {
    pub(crate) children: Vec<RowChild>,
    pub(crate) gap: u16,
    pub(crate) vertical_align: VerticalAlign,
}

/// FEATURE EXTENSION API. One row child and its width track.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RowChild {
    pub(crate) track: TrackSize,
    pub(crate) view: View,
}

impl RowChild {
    pub(crate) fn content(view: View) -> Self {
        Self {
            track: TrackSize::Content { max: None },
            view,
        }
    }

    pub(crate) fn fixed(width: u16, view: View) -> Self {
        Self {
            track: TrackSize::Fixed(width),
            view,
        }
    }

    pub(crate) fn flex(view: View) -> Self {
        Self {
            track: TrackSize::Flex { min: 1 },
            view,
        }
    }
}

/// FEATURE EXTENSION API. Width allocation for a row child.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TrackSize {
    Content { max: Option<u16> },
    Fixed(u16),
    Flex { min: u16 },
}

/// FEATURE EXTENSION API. Vertical alignment for row children.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum VerticalAlign {
    #[default]
    Top,
    Center,
    Bottom,
}

/// FEATURE EXTENSION API. Structural container holding one semantic child.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ContainerNode {
    pub(crate) child: Box<View>,
}

/// FEATURE EXTENSION API. Common semantic decoration applied by the compiler
/// around a View node.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Decoration {
    pub(crate) padding: Insets,
    /// Paints the allocated physical surface, including transparent geometry.
    pub(crate) surface_background: Option<ColorSpec>,
    pub(crate) border: Option<BorderSpec>,
    /// Sparse text intent inherited by descendants and text spans.
    pub(crate) text_style: StyleSpec,
}

impl Decoration {
    pub(crate) fn background(color: ColorSpec) -> Self {
        Self {
            surface_background: Some(color),
            ..Self::default()
        }
    }

    pub(crate) fn padding(mut self, padding: Insets) -> Self {
        self.padding = padding;
        self
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

/// FEATURE EXTENSION API. Truncation behavior after physical layout.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ClampRowsView {
    pub(crate) child: Box<View>,
    pub(crate) max_rows: u16,
    pub(crate) overflow: OverflowIndicator,
}

/// FEATURE EXTENSION API. Indicator for a clamped view.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum OverflowIndicator {
    None,
    Ellipsis { style: StyleSpec },
    Footer { prefix: String, style: StyleSpec },
}

/// FEATURE EXTENSION API. Generic attachment relationship decided by a feature.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum FlowBoundary {
    #[default]
    Default,
    AttachToPrevious,
}

/// FEATURE EXTENSION API. Backend-neutral key semantics for interaction surfaces.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UiKey {
    Char(char),
    Enter,
    Escape,
    Backspace,
    Tab,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    Unknown,
}

/// FEATURE EXTENSION API. Result returned by a dock or modal interaction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum InteractionResult {
    Ignored,
    Consumed,
    Action(HostAction),
}

/// FEATURE EXTENSION API. Host-owned action emitted by an interaction surface.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HostAction(pub(crate) String);

/// FEATURE EXTENSION API. A stateful surface outside durable conversation history.
/// Its view is semantic; physical size and clipping belong to the host.
pub(crate) trait DockPanel: Debug + Any {
    fn view(&self) -> View;
    fn size_policy(&self) -> DockSizePolicy;
    fn handle_key(&mut self, key: UiKey) -> InteractionResult;
    fn focus(&mut self) {}
    fn blur(&mut self) {}
}

/// FEATURE EXTENSION API. Generic dock sizing policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DockSizePolicy {
    HiddenWhenEmpty,
    Content { max_rows: Option<u16> },
    Fixed(u16),
}

/// FEATURE EXTENSION API. A focused interaction surface with priority input.
pub(crate) trait Modal: Debug {
    fn view(&self) -> View;
    fn handle_key(&mut self, key: UiKey) -> InteractionResult;
}

/// FEATURE EXTENSION API. Mutable live-region content. It does not expose
/// wrapping, terminal coordinates, spill, or commit operations.
pub(crate) trait ActiveContent: Debug {
    fn view(&self) -> View;

    fn boundary(&self) -> FlowBoundary {
        FlowBoundary::Default
    }

    fn tick(&mut self, _now: Instant) -> bool {
        false
    }

    fn finish(self: Box<Self>) -> Vec<Box<dyn TranscriptBlock>> {
        Vec::new()
    }
}

/// FEATURE EXTENSION API. Durable Iyon content implemented above the generic
/// presentation boundary.
pub(crate) trait TranscriptBlock: Debug {
    fn view(&self) -> View;
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
    fn view_style_merges_node_intent_without_rewriting_spans() {
        let mut view = View::styled_text(vec![
            TextSpan::plain("plain"),
            TextSpan::styled("bold", StyleSpec::new().bold()),
        ])
        .style(StyleSpec::new().foreground(ColorSpec::Ansi(1)))
        .style(StyleSpec::new().italic());

        assert_eq!(
            view.decoration.text_style.foreground,
            Some(ColorSpec::Ansi(1))
        );
        assert_eq!(view.decoration.text_style.attributes.italic, Some(true));
        let ViewKind::Text(text) = &mut view.kind else {
            panic!("expected text view");
        };
        assert_eq!(text.spans[0].style, StyleSpec::default());
        assert_eq!(text.spans[1].style.attributes.bold, Some(true));
    }

    #[test]
    fn row_and_container_are_owned_data() {
        let view = View::box_(
            View::row(
                vec![
                    RowChild::content(View::text("●").no_wrap()),
                    RowChild::flex(View::text("long command").width(WidthRule::Fill)),
                ],
                1,
            ),
            Decoration::background(ColorSpec::Theme("tool.running".into())).padding(Insets::all(1)),
        );
        assert!(matches!(view.kind, ViewKind::Container(_)));
        assert!(!view.decoration.padding.eq(&Insets::ZERO));
    }
}
