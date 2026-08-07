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
    pub(crate) kind: ViewKind,
}

impl View {
    pub(crate) fn text(text: impl Into<String>) -> Self {
        Self {
            width: WidthRule::Fit,
            kind: ViewKind::Text(TextView::plain(text)),
        }
    }

    pub(crate) fn styled_text(spans: Vec<TextSpan>) -> Self {
        Self {
            width: WidthRule::Fit,
            kind: ViewKind::Text(TextView {
                spans,
                wrap: WrapMode::WordThenGrapheme,
                align: HorizontalAlign::Start,
            }),
        }
    }

    pub(crate) fn markdown(source: impl Into<String>, style: StyleSpec) -> Self {
        Self {
            width: WidthRule::Fill,
            kind: ViewKind::Markdown(MarkdownView {
                source: source.into(),
                style,
            }),
        }
    }

    pub(crate) fn column(children: Vec<View>, gap: u16) -> Self {
        Self {
            width: WidthRule::Fit,
            kind: ViewKind::Column(ColumnView { children, gap }),
        }
    }

    pub(crate) fn row(children: Vec<RowChild>, gap: u16) -> Self {
        Self {
            width: WidthRule::Fill,
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
            kind: ViewKind::Box(BoxView {
                child: Box::new(child),
                decoration,
            }),
        }
    }

    pub(crate) fn spacer(rows: u16) -> Self {
        Self {
            width: WidthRule::Fit,
            kind: ViewKind::Spacer { rows },
        }
    }

    pub(crate) fn clamp_rows(child: View, max_rows: u16, overflow: OverflowIndicator) -> Self {
        Self {
            width: child.width,
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
        if let ViewKind::Text(text) = &mut self.kind {
            for span in &mut text.spans {
                span.style = style.clone();
            }
        }
        self
    }
}

/// FEATURE EXTENSION API. Generic view node kinds understood by the compiler.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ViewKind {
    Text(TextView),
    Markdown(MarkdownView),
    Column(ColumnView),
    Row(RowView),
    Box(BoxView),
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

/// FEATURE EXTENSION API. Generic semantic styling, independent of Ratatui.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct StyleSpec {
    pub(crate) foreground: Option<ColorSpec>,
    pub(crate) background: Option<ColorSpec>,
    pub(crate) attributes: TextAttributes,
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

/// FEATURE EXTENSION API. Markdown source accepted by the existing Markdown renderer.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MarkdownView {
    pub(crate) source: String,
    pub(crate) style: StyleSpec,
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

/// FEATURE EXTENSION API. Generic box decoration.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BoxView {
    pub(crate) child: Box<View>,
    pub(crate) decoration: Decoration,
}

/// FEATURE EXTENSION API. Generic box decoration.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Decoration {
    pub(crate) padding: Insets,
    pub(crate) foreground: Option<ColorSpec>,
    pub(crate) background: Option<ColorSpec>,
    pub(crate) border: Option<BorderSpec>,
    pub(crate) attributes: TextAttributes,
}

impl Decoration {
    pub(crate) fn background(color: ColorSpec) -> Self {
        Self {
            background: Some(color),
            ..Self::default()
        }
    }

    pub(crate) fn padding(mut self, padding: Insets) -> Self {
        self.padding = padding;
        self
    }
}

/// FEATURE EXTENSION API. Box insets; padding belongs to the box, not its child.
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

/// FEATURE EXTENSION API. Backend-neutral text attributes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct TextAttributes {
    pub(crate) bold: bool,
    pub(crate) dim: bool,
    pub(crate) italic: bool,
    pub(crate) underline: bool,
    pub(crate) reversed: bool,
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

/// FEATURE EXTENSION API. Semantic flow and grouping for durable content.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FlowSpec {
    pub(crate) units: Vec<FlowUnit>,
    pub(crate) gap: u16,
}

/// FEATURE EXTENSION API. A generic flow unit; the generic layer does not know
/// why a group is attached or which feature produced it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum FlowUnit {
    Item {
        view: View,
        boundary: FlowBoundary,
    },
    Group {
        children: Vec<FlowUnit>,
        gap: u16,
        decoration: Option<Decoration>,
    },
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
    fn row_and_box_are_owned_data() {
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
        assert!(matches!(view.kind, ViewKind::Box(_)));
    }

    #[test]
    fn flow_gap_is_separate_from_view_composition() {
        let flow = FlowSpec {
            units: vec![
                FlowUnit::Item {
                    view: View::text("one"),
                    boundary: FlowBoundary::Default,
                },
                FlowUnit::Item {
                    view: View::text("two"),
                    boundary: FlowBoundary::Default,
                },
            ],
            gap: 1,
        };
        assert_eq!(flow.gap, 1);
    }
}
