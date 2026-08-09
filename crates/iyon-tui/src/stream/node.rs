//! Semantic stream nodes and static lowering.

use crate::presentation::{
    api::{IntoView, TextSpan},
    ir::{View, WidthRule},
};

use super::{
    StreamOffset, StreamRange,
    projected::{ExactTerminator, ProjectedText, ProjectedTextLayout, slice_projected_text},
};

/// Semantic provenance attached to stream view nodes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum StreamProvenance {
    /// A source-mapped text flow whose physical rows can expose monotonic source
    /// checkpoints, including transformed Markdown text.
    Projected(StreamRange),

    /// Presentation is genuinely indivisible and must be transferred as one unit.
    Atomic(StreamRange),
}

/// A semantic presentation node with truthful provenance.
///
/// Exact text is statically constrained to [`TextView`] plus an optional typed
/// structural terminator, making arbitrary hidden source unrepresentable.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum StreamNode {
    Text(ProjectedText),

    Atomic { range: StreamRange, view: View },
}

impl StreamNode {
    pub(crate) fn projected_text(text: ProjectedText) -> Self {
        Self::Text(text)
    }

    pub(crate) fn exact_text(text_range: StreamRange, spans: Vec<TextSpan>) -> Self {
        Self::Text(ProjectedText::identity(
            text_range,
            ExactTerminator::None,
            spans,
        ))
    }

    pub(crate) fn exact_line(
        text_range: StreamRange,
        spans: Vec<TextSpan>,
        has_newline: bool,
    ) -> Self {
        Self::Text(ProjectedText::identity(
            text_range,
            if has_newline {
                ExactTerminator::HardNewline
            } else {
                ExactTerminator::None
            },
            spans,
        ))
    }

    pub(crate) fn with_width(mut self, width_rule: WidthRule) -> Self {
        if let Self::Text(text) = &mut self {
            text.width = width_rule;
        }
        self
    }

    pub(crate) fn atomic(range: StreamRange, view: View) -> Self {
        assert!(
            !view.contains_component_identity(),
            "stream atomic view cannot contain component identity"
        );
        Self::Atomic { range, view }
    }

    /// The full monotonic source range owned by this node (including any typed structural terminator).
    pub(crate) fn owned_range(&self) -> StreamRange {
        match self {
            Self::Text(text) => text.owned_range(),
            Self::Atomic { range, .. } => *range,
        }
    }

    pub(crate) fn source_range(&self) -> StreamRange {
        self.owned_range()
    }

    pub(crate) fn provenance(&self) -> StreamProvenance {
        match self {
            Self::Text(_) => StreamProvenance::Projected(self.owned_range()),
            Self::Atomic { range, .. } => StreamProvenance::Atomic(*range),
        }
    }
}

/// V1 linear stream view: an ordered sequence of provenance-bearing semantic blocks.
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct StreamView {
    pub(crate) nodes: Vec<StreamNode>,
}

impl StreamView {
    pub(crate) fn new(nodes: Vec<StreamNode>) -> Self {
        Self { nodes }
    }

    /// Lowers the stream's exact/atomic presentation into the ordinary static
    /// view vocabulary without changing visible content. Structural source
    /// terminators remain provenance metadata and are intentionally not emitted.
    pub(crate) fn into_static_view(self) -> View {
        let children = self
            .nodes
            .into_iter()
            .map(|node| match node {
                StreamNode::Text(text) => {
                    let body = View::styled_text(
                        text.runs
                            .iter()
                            .filter(|run| !run.display.is_empty())
                            .cloned()
                            .map(|run| TextSpan::styled(run.display, run.style)),
                    )
                    .width(match &text.layout {
                        ProjectedTextLayout::Plain => text.width,
                        ProjectedTextLayout::Hanging { .. } => WidthRule::Fill,
                    });
                    match &text.layout {
                        ProjectedTextLayout::Plain => body.into_view(),
                        ProjectedTextLayout::Hanging {
                            body_column,
                            prefix,
                            prefix_style,
                            show_prefix,
                            ..
                        } => View::horizontal(|row| {
                            row.fixed(
                                *body_column,
                                if *show_prefix {
                                    View::styled_text(vec![TextSpan::styled(
                                        prefix.clone(),
                                        prefix_style.clone(),
                                    )])
                                    .no_wrap()
                                } else {
                                    View::text("").width(WidthRule::Fill)
                                },
                            );
                            row.flex(body);
                        }),
                    }
                }
                StreamNode::Atomic { view, .. } => view,
            })
            .collect::<Vec<_>>();

        View::vertical(|column| {
            column.children(children);
        })
    }

    pub(crate) fn suffix_from(&self, offset: StreamOffset) -> Self {
        let mut nodes = Vec::new();
        for node in &self.nodes {
            let range = node.owned_range();
            if range.end <= offset {
                continue;
            }
            if range.start >= offset {
                nodes.push(node.clone());
                continue;
            }
            match node {
                StreamNode::Text(text) => {
                    nodes.push(StreamNode::Text(slice_projected_text(text, offset)));
                }
                StreamNode::Atomic { .. } => {
                    panic!("stream suffix cuts an indivisible atomic node")
                }
            }
        }
        Self::new(nodes)
    }

    pub(crate) fn empty() -> Self {
        Self { nodes: Vec::new() }
    }

    pub(crate) fn push(&mut self, node: StreamNode) {
        self.nodes.push(node);
    }

    /// Single exact text block.
    pub(crate) fn exact_text(range: StreamRange, spans: Vec<TextSpan>) -> Self {
        Self {
            nodes: vec![StreamNode::exact_text(range, spans)],
        }
    }

    /// Single atomic view.
    pub(crate) fn atomic(range: StreamRange, view: View) -> Self {
        Self {
            nodes: vec![StreamNode::atomic(range, view)],
        }
    }
}
