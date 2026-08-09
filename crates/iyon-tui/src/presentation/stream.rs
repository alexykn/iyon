//! Generic stream vocabulary and contracts.
//!
//! A stream is an append-only semantic source with an explicit monotonic coordinate
//! space ([`StreamOffset`]) and a semantic immutability frontier ([`StreamSnapshot::stable_through`]).
//!
//! The presentation algebra attaches source provenance to semantic subtrees ([`StreamNode`]),
//! allowing a stream-aware compiler to translate semantic stability into physical commit
//! eligibility at actual layout width.

use std::{fmt::Debug, time::Instant};
use unicode_segmentation::UnicodeSegmentation;

use crate::presentation::{
    api::{
        IntoView,
        style::StyleSpec,
        text::{HorizontalAlign, TextSpan, WrapMode},
    },
    ir::{ColumnView, RowChild, View, ViewKind, WidthRule},
};

/// Opaque monotonic coordinate within a stream's source space.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub(crate) struct StreamOffset(pub(crate) u64);

impl StreamOffset {
    pub(crate) const ZERO: Self = Self(0);

    pub(crate) const fn new(offset: u64) -> Self {
        Self(offset)
    }

    pub(crate) const fn as_u64(self) -> u64 {
        self.0
    }

    pub(crate) fn saturating_add(self, rhs: u64) -> Self {
        Self(self.0.saturating_add(rhs))
    }
}

/// Monotonic revision counter for stream snapshots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub(crate) struct StreamRevision(pub(crate) u64);

impl StreamRevision {
    pub(crate) const ZERO: Self = Self(0);

    pub(crate) const fn new(rev: u64) -> Self {
        Self(rev)
    }

    pub(crate) fn next(self) -> Self {
        Self(self.0.wrapping_add(1))
    }
}

/// A half-open source range `[start, end)` in monotonic stream coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub(crate) struct StreamRange {
    pub(crate) start: StreamOffset,
    pub(crate) end: StreamOffset,
}

impl StreamRange {
    pub(crate) const fn new(start: StreamOffset, end: StreamOffset) -> Self {
        Self { start, end }
    }

    pub(crate) const fn empty(at: StreamOffset) -> Self {
        Self { start: at, end: at }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.start >= self.end
    }

    pub(crate) fn len(&self) -> u64 {
        self.end.0.saturating_sub(self.start.0)
    }

    pub(crate) fn contains_offset(&self, offset: StreamOffset) -> bool {
        offset >= self.start && offset < self.end
    }
}

/// Semantic provenance attached to stream view nodes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum StreamProvenance {
    /// A source-mapped text flow whose physical rows can expose monotonic source
    /// checkpoints, including transformed Markdown text.
    Projected(StreamRange),

    /// Presentation is genuinely indivisible and must be committed as one unit.
    Atomic(StreamRange),
}

/// Structural terminator owned by a projected text node after its visible text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ExactTerminator {
    #[default]
    None,
    HardNewline,
}

impl ExactTerminator {
    pub(crate) const fn source_len(self) -> u64 {
        match self {
            Self::None => 0,
            Self::HardNewline => 1,
        }
    }
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

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ProjectedText {
    pub(crate) content_range: StreamRange,
    pub(crate) terminator: ExactTerminator,
    pub(crate) width: WidthRule,
    pub(crate) wrap: WrapMode,
    pub(crate) align: HorizontalAlign,
    pub(crate) layout: ProjectedTextLayout,
    pub(crate) runs: Vec<ProjectedTextRun>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ProjectedTextLayout {
    Plain,
    Hanging {
        body_column: u16,
        prefix: String,
        prefix_style: StyleSpec,
        prefix_source: StreamRange,
        show_prefix: bool,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ProjectedTextRun {
    pub(crate) display: String,
    pub(crate) style: StyleSpec,
    pub(crate) owned: StreamRange,
    pub(crate) exact_visible: Option<StreamRange>,
}

impl ProjectedText {
    pub(crate) fn owned_range(&self) -> StreamRange {
        StreamRange::new(
            self.content_range.start,
            self.content_range
                .end
                .saturating_add(self.terminator.source_len()),
        )
    }

    pub(crate) fn identity(
        content_range: StreamRange,
        terminator: ExactTerminator,
        spans: Vec<TextSpan>,
    ) -> Self {
        let mut cursor = content_range.start;
        let mut runs: Vec<ProjectedTextRun> = Vec::new();
        for span in spans {
            if span.text.is_empty() {
                continue;
            }
            let start = cursor;
            cursor = cursor.saturating_add(span.text.len() as u64);
            if let Some(previous) = runs.last_mut()
                && previous.style == span.style
                && previous.owned.end == start
            {
                previous.display.push_str(&span.text);
                previous.owned.end = cursor;
                previous.exact_visible = Some(StreamRange::new(previous.owned.start, cursor));
            } else {
                runs.push(ProjectedTextRun {
                    display: span.text,
                    style: span.style,
                    owned: StreamRange::new(start, cursor),
                    exact_visible: Some(StreamRange::new(start, cursor)),
                });
            }
        }
        Self {
            content_range,
            terminator,
            width: WidthRule::Fit,
            wrap: WrapMode::WordThenGrapheme,
            align: HorizontalAlign::Start,
            layout: ProjectedTextLayout::Plain,
            runs,
        }
    }
}

fn slice_projected_text(text: &ProjectedText, offset: StreamOffset) -> ProjectedText {
    assert!(offset > text.content_range.start);
    assert!(offset < text.owned_range().end);
    assert!(
        offset <= text.content_range.end,
        "cannot slice inside a terminator"
    );

    let mut runs = Vec::new();
    for run in &text.runs {
        if run.owned.end <= offset {
            continue;
        }
        if run.owned.start >= offset {
            runs.push(run.clone());
            continue;
        }

        let Some(visible) = run.exact_visible else {
            panic!("projected replacement may only be sliced at run boundaries");
        };
        assert!(offset >= visible.start && offset <= visible.end);
        let relative = offset.as_u64().saturating_sub(visible.start.as_u64()) as usize;
        assert!(run.display.is_char_boundary(relative));
        assert!(
            run.display
                .grapheme_indices(true)
                .any(|(start, _)| start == relative)
        );
        let display = run.display[relative..].to_string();
        runs.push(ProjectedTextRun {
            display,
            style: run.style.clone(),
            owned: StreamRange::new(offset, run.owned.end),
            exact_visible: Some(StreamRange::new(offset, visible.end)),
        });
    }

    ProjectedText {
        content_range: StreamRange::new(offset, text.content_range.end),
        terminator: text.terminator,
        width: text.width,
        wrap: text.wrap,
        align: text.align,
        layout: match &text.layout {
            ProjectedTextLayout::Plain => ProjectedTextLayout::Plain,
            ProjectedTextLayout::Hanging {
                body_column,
                prefix,
                prefix_style,
                prefix_source,
                ..
            } => ProjectedTextLayout::Hanging {
                body_column: *body_column,
                prefix: prefix.clone(),
                prefix_style: prefix_style.clone(),
                prefix_source: *prefix_source,
                show_prefix: offset <= prefix_source.start,
            },
        },
        runs,
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
                        } => View::row(
                            vec![
                                RowChild::fixed(
                                    *body_column,
                                    if *show_prefix {
                                        View::styled_text(vec![TextSpan::styled(
                                            prefix.clone(),
                                            prefix_style.clone(),
                                        )])
                                        .no_wrap()
                                        .into_view()
                                    } else {
                                        View::text("").width(WidthRule::Fill).into_view()
                                    },
                                ),
                                RowChild::flex(body.into_view()),
                            ],
                            0,
                        ),
                    }
                }
                StreamNode::Atomic { view, .. } => view,
            })
            .collect();

        View {
            width: WidthRule::Fit,
            decoration: crate::presentation::ir::Decoration::default(),
            kind: ViewKind::Column(ColumnView { children, gap: 0 }),
        }
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

/// Width-independent snapshot of the current unacknowledged stream state.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct StreamSnapshot {
    pub(crate) revision: StreamRevision,

    /// Earliest source represented by this snapshot.
    pub(crate) source_base: StreamOffset,

    /// End of all source received/presented so far.
    pub(crate) source_end: StreamOffset,

    /// Largest semantic source prefix guaranteed never to change presentation
    /// because of future appends.
    pub(crate) stable_through: StreamOffset,

    pub(crate) view: StreamView,
}

impl StreamSnapshot {
    /// Validates snapshot ordering, provenance invariants, and source contiguity.
    pub(crate) fn validate(&self) -> bool {
        if self.source_base > self.stable_through || self.stable_through > self.source_end {
            return false;
        }

        let mut expected = self.source_base;
        for node in &self.view.nodes {
            let owned = node.owned_range();

            // Contiguity: each node must start exactly where the previous one ended.
            if owned.start != expected {
                return false;
            }

            if owned.end > self.source_end {
                return false;
            }

            match node {
                StreamNode::Text(text) => {
                    if !validate_projected_text(text) {
                        return false;
                    }
                }
                StreamNode::Atomic { .. } => {}
            }

            expected = owned.end;
        }

        // All source must be covered by nodes (no trailing uncovered source).
        if expected != self.source_end {
            return false;
        }

        true
    }
}

fn validate_projected_text(text: &ProjectedText) -> bool {
    if text.content_range.start > text.content_range.end {
        return false;
    }
    let mut expected = match &text.layout {
        ProjectedTextLayout::Plain => text.content_range.start,
        ProjectedTextLayout::Hanging {
            prefix_source,
            show_prefix,
            ..
        } => {
            if *show_prefix {
                if prefix_source.start != text.content_range.start
                    || prefix_source.end > text.content_range.end
                {
                    return false;
                }
                prefix_source.end
            } else {
                text.content_range.start
            }
        }
    };
    for run in &text.runs {
        if run.owned.start != expected || run.owned.start >= run.owned.end {
            return false;
        }
        if run.owned.end > text.content_range.end {
            return false;
        }
        if let Some(visible) = run.exact_visible {
            if visible.start < run.owned.start
                || visible.end > run.owned.end
                || visible.start > visible.end
            {
                return false;
            }
            if run.display.len() != visible.len() as usize {
                return false;
            }
        }
        expected = run.owned.end;
    }
    expected == text.content_range.end
}

/// Trait for append-only streaming content with explicit source coordinates and semantic stability.
pub(crate) trait StreamingContent: Debug + Send {
    /// Width-independent semantic presentation of the current unacknowledged stream state.
    fn snapshot(&self) -> StreamSnapshot;

    /// Called only AFTER native-history ownership has successfully advanced.
    /// The implementation may discard source/presentation state strictly before this cursor.
    fn compact_before(&mut self, _offset: StreamOffset) {}

    /// Animation or timer tick unrelated to incoming source.
    fn tick(&mut self, _now: Instant) -> bool {
        false
    }

    /// Seals the stream against future mutations, stabilizing the final semantic content in-place.
    fn seal(&mut self);

    /// Whether the stream has been sealed.
    fn is_sealed(&self) -> bool;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LeadingBoundaryState {
    None,
    Pending,
    Committed,
}

/// Host-managed conversation stream with explicit unit identity and commit cursor.
///
/// After sealing, the host either drains a pinned physical remainder or is
/// consumed into an immutable transcript resident tail.
#[derive(Debug)]
pub(crate) struct HostedStream<C>
where
    C: StreamingContent,
{
    pub(crate) unit_id: crate::transcript::model::TranscriptUnitId,
    content: C,
    pub(crate) leading_boundary: LeadingBoundaryState,
    pub(crate) committed_through: StreamOffset,
    pub(crate) partial: Option<StreamPartialCommit>,
    pub(crate) lifecycle: crate::transcript::model::EntryLifecycle,

    last_revision: StreamRevision,
    last_source_end: StreamOffset,
    last_stable_through: StreamOffset,
}

impl<C> HostedStream<C>
where
    C: StreamingContent,
{
    pub(crate) fn new(
        unit_id: crate::transcript::model::TranscriptUnitId,
        content: C,
        leading_boundary: LeadingBoundaryState,
    ) -> Self {
        Self {
            unit_id,
            content,
            leading_boundary,
            committed_through: StreamOffset::ZERO,
            partial: None,
            lifecycle: crate::transcript::model::EntryLifecycle::Open,
            last_revision: StreamRevision::ZERO,
            last_source_end: StreamOffset::ZERO,
            last_stable_through: StreamOffset::ZERO,
        }
    }

    pub(crate) fn content(&self) -> &C {
        &self.content
    }

    pub(crate) fn content_mut(&mut self) -> &mut C {
        &mut self.content
    }

    fn observe_snapshot(&mut self, snapshot: &StreamSnapshot) {
        debug_assert!(
            snapshot.validate(),
            "StreamSnapshot invariant violated: base={:?}, stable={:?}, end={:?}",
            snapshot.source_base,
            snapshot.stable_through,
            snapshot.source_end,
        );
        debug_assert!(
            snapshot.revision >= self.last_revision,
            "Stream revision decreased: prev={:?}, next={:?}",
            self.last_revision,
            snapshot.revision,
        );
        debug_assert!(
            snapshot.source_end >= self.last_source_end,
            "Stream source_end decreased: prev={:?}, next={:?}",
            self.last_source_end,
            snapshot.source_end,
        );
        debug_assert!(
            snapshot.stable_through >= self.last_stable_through,
            "Stream stable_through decreased: prev={:?}, next={:?}",
            self.last_stable_through,
            snapshot.stable_through,
        );
        debug_assert!(
            snapshot.source_base <= self.committed_through,
            "Stream source_base moved past committed_through: base={:?}, committed={:?}",
            snapshot.source_base,
            self.committed_through,
        );

        self.last_revision = snapshot.revision;
        self.last_source_end = snapshot.source_end;
        self.last_stable_through = snapshot.stable_through;
    }

    pub(crate) fn snapshot(&mut self) -> StreamSnapshot {
        let snapshot = self.content.snapshot();
        self.observe_snapshot(&snapshot);
        snapshot
    }

    /// Prepares physical compilation and plans commit eligibility at `width`.
    pub(crate) fn prepare_frame(
        &mut self,
        width: u16,
        desired_commit_rows: usize,
    ) -> PreparedStreamFrame {
        let snapshot = self.snapshot();
        let (live_rows, compiled) = match self.partial.as_ref() {
            Some(StreamPartialCommit::FrozenAtomic {
                source_end,
                rows,
                committed_rows,
                ..
            }) => {
                let suffix_view = snapshot.view.suffix_from(*source_end);
                let suffix = compile_stream(&suffix_view, width, snapshot.stable_through);
                (
                    rows.as_slice()[*committed_rows..]
                        .iter()
                        .cloned()
                        .chain(suffix.rows.iter().cloned())
                        .collect(),
                    suffix,
                )
            }
            _ => {
                let live_view = snapshot.view.suffix_from(self.committed_through);
                let compiled = compile_stream(&live_view, width, snapshot.stable_through);
                (compiled.rows.clone(), compiled)
            }
        };
        let mut live_rows = live_rows;
        if self.leading_boundary == LeadingBoundaryState::Pending {
            live_rows.insert(0, ratatui::text::Line::default());
        }

        // `desired_commit_rows` is measured against the complete conversation projection.
        // The leading boundary is one of those rows, but it may never be committed alone.
        let pending_boundary_rows = if self.leading_boundary == LeadingBoundaryState::Pending {
            1
        } else {
            0
        };
        let semantic_desired_rows = desired_commit_rows.saturating_sub(pending_boundary_rows);
        let semantic_plan = plan_commit(
            &compiled,
            semantic_desired_rows,
            self.partial.as_ref(),
            self.committed_through,
        );
        let semantic_rows = match &semantic_plan.payload {
            CommitPayload::Compiled { start, len } => {
                compiled.rows[*start..start.saturating_add(*len)].to_vec()
            }
            CommitPayload::Frozen { rows } => rows.as_slice().to_vec(),
        };
        let commit_leading_boundary =
            self.leading_boundary == LeadingBoundaryState::Pending && !semantic_rows.is_empty();
        let mut history_rows = Vec::with_capacity(
            semantic_rows
                .len()
                .saturating_add(usize::from(commit_leading_boundary)),
        );
        if commit_leading_boundary {
            history_rows.push(ratatui::text::Line::default());
        }
        history_rows.extend(semantic_rows);

        PreparedStreamFrame {
            live_rows,
            history: PreparedHistoryWrite {
                rows: FrozenPhysicalRows::new(history_rows),
                semantic_plan,
                commit_leading_boundary,
            },
        }
    }

    /// Confirms that the complete prepared history transaction was successfully written.
    pub(crate) fn apply_commit_success(&mut self, transaction: PreparedHistoryWrite) {
        if transaction.commit_leading_boundary {
            debug_assert_eq!(self.leading_boundary, LeadingBoundaryState::Pending);
            self.leading_boundary = LeadingBoundaryState::Committed;
        }

        let plan = transaction.semantic_plan;
        debug_assert!(
            plan.next_committed_through >= self.committed_through,
            "committed_through must advance monotonically: prev={:?}, next={:?}",
            self.committed_through,
            plan.next_committed_through
        );
        self.committed_through = plan.next_committed_through;
        self.partial = plan.next_partial;
        self.content.compact_before(self.committed_through);
    }

    pub(crate) fn tick(&mut self, now: Instant) -> bool {
        self.content.tick(now)
    }

    pub(crate) fn seal(&mut self) {
        if self.is_sealed() {
            return;
        }
        self.lifecycle = crate::transcript::model::EntryLifecycle::Sealed;
        self.content.seal();

        let snapshot = self.content.snapshot();
        self.observe_snapshot(&snapshot);
    }

    pub(crate) fn is_sealed(&self) -> bool {
        self.lifecycle == crate::transcript::model::EntryLifecycle::Sealed
            || self.content.is_sealed()
    }

    pub(crate) fn handoff_blocking_rows(&self) -> usize {
        match &self.partial {
            Some(StreamPartialCommit::FrozenAtomic {
                rows,
                committed_rows,
                ..
            }) => rows.len().saturating_sub(*committed_rows),
            None => 0,
        }
    }

    pub(crate) fn can_handoff(&self) -> bool {
        self.is_sealed() && self.partial.is_none()
    }

    /// Consumes the mutable host and transfers its immutable remaining source
    /// presentation to the transcript host.
    pub(crate) fn into_resident_handoff(self) -> ResidentStreamHandoff {
        assert!(
            self.can_handoff(),
            "stream is not ready for resident handoff"
        );

        let snapshot = self.content.snapshot();
        assert!(snapshot.validate(), "invalid final stream snapshot");
        assert!(
            snapshot.source_base <= self.committed_through,
            "resident handoff cannot retain a source base past the native-history cursor"
        );
        assert_eq!(
            snapshot.stable_through, snapshot.source_end,
            "sealed resident handoff must be semantically stable"
        );
        if snapshot.source_base > StreamOffset::ZERO {
            assert_ne!(
                self.leading_boundary,
                LeadingBoundaryState::Pending,
                "source cannot advance before its leading boundary is committed"
            );
        }

        ResidentStreamHandoff {
            unit_id: self.unit_id,
            source_base: self.committed_through,
            source_end: snapshot.source_end,
            view: snapshot
                .view
                .suffix_from(self.committed_through)
                .into_static_view(),
            leading_boundary: self.leading_boundary,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ResidentStreamHandoff {
    pub(crate) unit_id: crate::transcript::model::TranscriptUnitId,
    pub(crate) source_base: StreamOffset,
    pub(crate) source_end: StreamOffset,
    pub(crate) view: View,
    pub(crate) leading_boundary: LeadingBoundaryState,
}

/// The exact physical native-history write and the semantic acknowledgement it carries.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PreparedHistoryWrite {
    pub(crate) rows: FrozenPhysicalRows,
    pub(crate) semantic_plan: CommitPlan,
    pub(crate) commit_leading_boundary: bool,
}

/// A host-prepared live projection and its exact terminal-history transaction.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PreparedStreamFrame {
    pub(crate) live_rows: Vec<ratatui::text::Line<'static>>,
    pub(crate) history: PreparedHistoryWrite,
}

/// Unique identifier for an atomic presentation group within a stream snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub(crate) struct StreamAtomicId(pub(crate) usize);

/// Physical row commit metadata produced by the stream compiler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum StreamRowCommit {
    /// Entire physical row may advance directly to this source boundary.
    Exact(StreamOffset),

    /// Row belongs to a stable atomic semantic region.
    Atomic {
        group: StreamAtomicId,
        source_end: StreamOffset,
    },

    /// This row may not enter native history (source > stable_through,
    /// impossible-fit EGC, blocked).
    Blocked,
}

/// Renderer-owned physical rows for atomic freeze.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FrozenPhysicalRows(pub(crate) Vec<ratatui::text::Line<'static>>);

impl FrozenPhysicalRows {
    pub(crate) fn new(rows: Vec<ratatui::text::Line<'static>>) -> Self {
        Self(rows)
    }

    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub(crate) fn as_slice(&self) -> &[ratatui::text::Line<'static>] {
        &self.0
    }
}

/// Payload specifying the exact physical rows to write to the terminal.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum CommitPayload {
    /// Slice of rows from the compiled stream layout.
    Compiled { start: usize, len: usize },

    /// Slice of frozen physical rows from a prior atomic partial commit.
    Frozen { rows: FrozenPhysicalRows },
}

impl CommitPayload {
    pub(crate) fn rows_to_write(&self) -> usize {
        match self {
            Self::Compiled { len, .. } => *len,
            Self::Frozen { rows } => rows.len(),
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.rows_to_write() == 0
    }
}

/// A compiled stream layout at a specific physical width.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CompiledStream {
    pub(crate) rows: Vec<ratatui::text::Line<'static>>,
    pub(crate) commit: Vec<StreamRowCommit>,
    pub(crate) committable_prefix_rows: usize,
}

/// Persistent partial-commit state across frame / resize boundaries.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum StreamPartialCommit {
    /// Frozen physical rows of an atomic region that was partially committed to history.
    FrozenAtomic {
        group: StreamAtomicId,
        source_end: StreamOffset,
        rows: FrozenPhysicalRows,
        committed_rows: usize,
    },
}

/// Plan for promoting physical stream rows into native history.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CommitPlan {
    /// Exact physical rows to write to the terminal.
    pub(crate) payload: CommitPayload,

    /// Source offset confirmed by native history after rows are successfully written.
    pub(crate) next_committed_through: StreamOffset,

    /// Next partial commit state after rows are successfully written.
    pub(crate) next_partial: Option<StreamPartialCommit>,
}

/// Central commit planner below the host.
///
/// Determines how many physical rows may be safely promoted to native history,
/// planning cursor advancement and atomic row freezing without mutating state.
pub(crate) fn plan_commit(
    compiled: &CompiledStream,
    desired_rows: usize,
    partial: Option<&StreamPartialCommit>,
    current_committed_through: StreamOffset,
) -> CommitPlan {
    // If we are currently in a FrozenAtomic partial state, we must drain the frozen rows first.
    if let Some(StreamPartialCommit::FrozenAtomic {
        group,
        source_end,
        rows,
        committed_rows,
    }) = partial
    {
        let remaining_frozen = rows.len().saturating_sub(*committed_rows);
        let to_commit = desired_rows.min(remaining_frozen);
        if to_commit == 0 {
            return CommitPlan {
                payload: CommitPayload::Frozen {
                    rows: FrozenPhysicalRows::new(Vec::new()),
                },
                next_committed_through: current_committed_through,
                next_partial: Some(partial.cloned().unwrap()),
            };
        }

        let slice = rows.as_slice()[*committed_rows..*committed_rows + to_commit].to_vec();
        let new_committed = committed_rows.saturating_add(to_commit);

        if new_committed >= rows.len() {
            return CommitPlan {
                payload: CommitPayload::Frozen {
                    rows: FrozenPhysicalRows::new(slice),
                },
                next_committed_through: *source_end,
                next_partial: None,
            };
        } else {
            return CommitPlan {
                payload: CommitPayload::Frozen {
                    rows: FrozenPhysicalRows::new(slice),
                },
                next_committed_through: current_committed_through,
                next_partial: Some(StreamPartialCommit::FrozenAtomic {
                    group: *group,
                    source_end: *source_end,
                    rows: rows.clone(),
                    committed_rows: new_committed,
                }),
            };
        }
    }

    if desired_rows == 0 || compiled.committable_prefix_rows == 0 {
        return CommitPlan {
            payload: CommitPayload::Compiled { start: 0, len: 0 },
            next_committed_through: current_committed_through,
            next_partial: None,
        };
    }

    let mut cursor = current_committed_through;
    let mut row_idx = 0;

    // Skip rows that have already entered native history in earlier transactions.
    // Handles both individual Exact rows and entire fully-committed Atomic groups.
    while row_idx < compiled.committable_prefix_rows {
        match &compiled.commit[row_idx] {
            StreamRowCommit::Exact(end) if *end <= current_committed_through => {
                row_idx += 1;
            }
            StreamRowCommit::Atomic { group, source_end }
                if *source_end <= current_committed_through =>
            {
                let committed_group = *group;
                while row_idx < compiled.committable_prefix_rows {
                    match &compiled.commit[row_idx] {
                        StreamRowCommit::Atomic { group, source_end }
                            if *group == committed_group
                                && *source_end <= current_committed_through =>
                        {
                            row_idx += 1;
                        }
                        _ => break,
                    }
                }
            }
            _ => break,
        }
    }

    let uncommitted_start = row_idx;
    let mut rows_written = 0;

    while row_idx < compiled.committable_prefix_rows && rows_written < desired_rows {
        match &compiled.commit[row_idx] {
            StreamRowCommit::Exact(offset) => {
                cursor = *offset;
                row_idx += 1;
                rows_written += 1;
            }
            StreamRowCommit::Atomic { group, source_end } => {
                let current_group = *group;
                let target_source_end = *source_end;
                let group_start = row_idx;
                let mut group_end = row_idx;

                while group_end < compiled.commit.len()
                    && matches!(&compiled.commit[group_end], StreamRowCommit::Atomic { group: g, .. } if *g == current_group)
                {
                    group_end += 1;
                }

                let group_rows = group_end - group_start;
                let remaining_desired = desired_rows - rows_written;

                if group_rows <= remaining_desired && group_end <= compiled.committable_prefix_rows
                {
                    cursor = target_source_end;
                    row_idx = group_end;
                    rows_written += group_rows;
                } else {
                    let take_in_group = remaining_desired
                        .min(compiled.committable_prefix_rows.saturating_sub(group_start));
                    let frozen_rows = compiled.rows[group_start..group_end].to_vec();

                    return CommitPlan {
                        payload: CommitPayload::Compiled {
                            start: uncommitted_start,
                            len: rows_written + take_in_group,
                        },
                        next_committed_through: cursor,
                        next_partial: Some(StreamPartialCommit::FrozenAtomic {
                            group: current_group,
                            source_end: target_source_end,
                            rows: FrozenPhysicalRows::new(frozen_rows),
                            committed_rows: take_in_group,
                        }),
                    };
                }
            }
            StreamRowCommit::Blocked => {
                break;
            }
        }
    }

    CommitPlan {
        payload: CommitPayload::Compiled {
            start: uncommitted_start,
            len: rows_written,
        },
        next_committed_through: cursor,
        next_partial: None,
    }
}

/// Compiles a [`StreamView`] into physical lines with commit metadata at the specified width.
///
/// Uses the unified presentation text compiler to guarantee that styles, Unicode
/// graphemes, and wrapping agreement remain identical between stream and view paths.
pub(crate) fn compile_stream(
    view: &StreamView,
    max_width: u16,
    stable_through: StreamOffset,
) -> CompiledStream {
    use crate::presentation::internal::ViewCompiler;

    let width = max_width.max(1);
    let compiler = ViewCompiler::default();
    let mut rows = Vec::new();
    let mut commit = Vec::new();
    let mut committable_prefix_rows = 0;
    let mut blocked = false;
    let mut next_atomic_id = 0usize;

    for node in &view.nodes {
        match node {
            StreamNode::Text(text) => {
                let (_w, compiled_rows) =
                    compiler.compile_projected_text_with_metadata(text, width);
                let final_offset = text.owned_range().end;
                let row_count = compiled_rows.len();
                if row_count == 0 {
                    rows.push(ratatui::text::Line::default());
                    if !blocked && final_offset <= stable_through {
                        commit.push(StreamRowCommit::Exact(final_offset));
                        committable_prefix_rows += 1;
                    } else {
                        blocked = true;
                        commit.push(StreamRowCommit::Blocked);
                    }
                } else {
                    for (r_idx, row) in compiled_rows.into_iter().enumerate() {
                        let is_last = r_idx + 1 == row_count;
                        let offset = if is_last {
                            final_offset
                        } else {
                            row.source_end.map_or(text.content_range.start, |relative| {
                                text.content_range.start.saturating_add(relative as u64)
                            })
                        };
                        rows.push(row.line);
                        if !blocked && row.fits && offset <= stable_through {
                            commit.push(StreamRowCommit::Exact(offset));
                            committable_prefix_rows += 1;
                        } else {
                            blocked = true;
                            commit.push(StreamRowCommit::Blocked);
                        }
                    }
                }
            }
            StreamNode::Atomic { range, view } => {
                next_atomic_id += 1;
                let group = StreamAtomicId(next_atomic_id);
                let layout = compiler.compile(view, width);
                let all_safe =
                    !blocked && range.end <= stable_through && layout.physically_complete;

                for row in layout.rows {
                    rows.push(row);
                    if all_safe {
                        commit.push(StreamRowCommit::Atomic {
                            group,
                            source_end: range.end,
                        });
                        committable_prefix_rows += 1;
                    } else {
                        blocked = true;
                        commit.push(StreamRowCommit::Blocked);
                    }
                }
            }
        }
    }

    CompiledStream {
        rows,
        commit,
        committable_prefix_rows,
    }
}

/// Conservative stability helper for plain append-only text.
///
/// When sealed, the entire text is stable.
/// When open, holds back the trailing extended grapheme cluster so that partial
/// UTF-8 or combining sequences are never committed before completion.
pub(crate) fn append_only_text_stable_frontier(
    source: &str,
    base: StreamOffset,
    sealed: bool,
) -> StreamOffset {
    if sealed || source.is_empty() {
        return base.saturating_add(source.len() as u64);
    }

    let mut last_offset = 0;
    for (offset, _grapheme) in source.grapheme_indices(true) {
        last_offset = offset;
    }

    base.saturating_add(last_offset as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presentation::api::style::{ColorSpec, StyleSpec, ThemeKey};
    use crate::presentation::internal::ViewCompiler;
    use ratatui::style::Modifier;

    #[test]
    fn stream_offset_ordering() {
        let o0 = StreamOffset::new(0);
        let o1 = StreamOffset::new(10);
        let o2 = StreamOffset::new(20);
        assert!(o0 < o1);
        assert!(o1 < o2);
        assert_eq!(o0.saturating_add(10), o1);
    }

    #[test]
    fn stream_range_helpers() {
        let r = StreamRange::new(StreamOffset::new(5), StreamOffset::new(15));
        assert_eq!(r.len(), 10);
        assert!(!r.is_empty());
        assert!(r.contains_offset(StreamOffset::new(5)));
        assert!(r.contains_offset(StreamOffset::new(14)));
        assert!(!r.contains_offset(StreamOffset::new(15)));
        assert!(!r.contains_offset(StreamOffset::new(4)));
    }

    #[test]
    fn snapshot_validation_rejects_invalid_visible_source_length() {
        let node = StreamNode::projected_text(ProjectedText {
            content_range: StreamRange::new(StreamOffset::new(0), StreamOffset::new(100)),
            terminator: ExactTerminator::None,
            width: WidthRule::Fit,
            wrap: WrapMode::WordThenGrapheme,
            align: HorizontalAlign::Start,
            layout: ProjectedTextLayout::Plain,
            runs: vec![ProjectedTextRun {
                display: "abc".to_string(),
                style: StyleSpec::default(),
                owned: StreamRange::new(StreamOffset::new(0), StreamOffset::new(100)),
                exact_visible: Some(StreamRange::new(
                    StreamOffset::new(0),
                    StreamOffset::new(100),
                )),
            }],
        });

        let snapshot = StreamSnapshot {
            revision: StreamRevision::new(1),
            source_base: StreamOffset::ZERO,
            source_end: StreamOffset::new(100),
            stable_through: StreamOffset::new(100),
            view: StreamView::new(vec![node]),
        };

        assert!(!snapshot.validate());
    }

    #[test]
    fn snapshot_validation_normal_exact_and_hard_newline() {
        // Normal Exact: visible = "abc", text_range = 0..3, terminator = None, owned = 0..3
        let node1 = StreamNode::exact_text(
            StreamRange::new(StreamOffset::new(0), StreamOffset::new(3)),
            vec![TextSpan::plain("abc")],
        );
        assert_eq!(
            node1.owned_range(),
            StreamRange::new(StreamOffset::new(0), StreamOffset::new(3))
        );

        // Exact with HardNewline: visible = "def", text_range = 3..6, terminator = HardNewline, owned = 3..7
        let node2 = StreamNode::exact_line(
            StreamRange::new(StreamOffset::new(3), StreamOffset::new(6)),
            vec![TextSpan::plain("def")],
            true,
        );
        assert_eq!(
            node2.owned_range(),
            StreamRange::new(StreamOffset::new(3), StreamOffset::new(7))
        );

        let snapshot = StreamSnapshot {
            revision: StreamRevision::new(1),
            source_base: StreamOffset::ZERO,
            source_end: StreamOffset::new(7),
            stable_through: StreamOffset::new(7),
            view: StreamView::new(vec![node1, node2]),
        };
        assert!(snapshot.validate());
    }

    #[test]
    fn compile_stream_empty_hard_newline_row_produces_one_physical_blank_row() {
        // Empty hard-newline row: visible = "", text_range = 2..2, terminator = HardNewline, owned = 2..3
        let node = StreamNode::exact_line(
            StreamRange::new(StreamOffset::new(2), StreamOffset::new(2)),
            Vec::new(),
            true,
        );
        assert_eq!(
            node.owned_range(),
            StreamRange::new(StreamOffset::new(2), StreamOffset::new(3))
        );

        let view = StreamView::new(vec![node]);
        let compiled = compile_stream(&view, 80, StreamOffset::new(3));
        assert_eq!(compiled.rows.len(), 1);
        assert_eq!(compiled.committable_prefix_rows, 1);
        assert_eq!(
            compiled.commit[0],
            StreamRowCommit::Exact(StreamOffset::new(3))
        );
    }

    // --- Fix 2: Atomic must not commit physically clipped content ---

    #[test]
    fn atomic_wide_grapheme_at_width_one_is_not_committable() {
        let view = StreamView::atomic(
            StreamRange::new(StreamOffset::ZERO, StreamOffset::new("漢".len() as u64)),
            View::text("漢").into_view(),
        );

        let compiled = compile_stream(&view, 1, StreamOffset::new("漢".len() as u64));
        assert_eq!(compiled.committable_prefix_rows, 0);
        assert!(matches!(compiled.commit[0], StreamRowCommit::Blocked));
    }

    #[test]
    fn atomic_wide_grapheme_at_width_two_is_committable() {
        let view = StreamView::atomic(
            StreamRange::new(StreamOffset::ZERO, StreamOffset::new("漢".len() as u64)),
            View::text("漢").into_view(),
        );

        let compiled = compile_stream(&view, 2, StreamOffset::new("漢".len() as u64));
        assert_eq!(compiled.committable_prefix_rows, 1);
    }

    #[test]
    fn atomic_view_uses_the_ordinary_view_style_compiler() {
        let mut view = View::text("atomic").into_view();
        view.decoration.text_style = StyleSpec::new().bold();
        let stream = StreamView::atomic(
            StreamRange::new(StreamOffset::ZERO, StreamOffset::new(6)),
            view.clone(),
        );
        let compiled = compile_stream(&stream, 20, StreamOffset::new(6));
        let ordinary = ViewCompiler::default().compile(&view, 20);

        assert_eq!(compiled.rows, ordinary.rows);
        assert!(
            compiled.rows[0].spans[0]
                .style
                .add_modifier
                .contains(Modifier::BOLD)
        );
    }

    #[test]
    fn atomic_nested_wide_grapheme_propagates_incompleteness() {
        // Atomic(Box(Column(Text("before"), Text("漢")))) at width 1
        let inner = View::column(
            vec![
                View::text("before").into_view(),
                View::text("漢").into_view(),
            ],
            0,
        );
        let boxed = View::box_(inner, crate::presentation::ir::Decoration::default());
        let view = StreamView::atomic(
            StreamRange::new(StreamOffset::ZERO, StreamOffset::new(10)),
            boxed,
        );

        let compiled = compile_stream(&view, 1, StreamOffset::new(10));
        // Physical incompleteness from "漢" must propagate through Column -> Box
        assert_eq!(compiled.committable_prefix_rows, 0);
        for c in &compiled.commit {
            assert!(matches!(c, StreamRowCommit::Blocked));
        }
    }

    #[test]
    fn append_only_text_frontier() {
        let text = "hello 🌍";
        let base = StreamOffset::new(10);

        // Open: leaves trailing grapheme cluster out of stable frontier
        let open_frontier = append_only_text_stable_frontier(text, base, false);
        // "hello " is 6 bytes. 🌍 is 4 bytes at offset 6.
        assert_eq!(open_frontier, StreamOffset::new(16));

        // Sealed: covers all bytes
        let sealed_frontier = append_only_text_stable_frontier(text, base, true);
        assert_eq!(sealed_frontier, StreamOffset::new(10 + text.len() as u64));
    }

    #[test]
    fn exact_text_accumulates_source_offsets_across_multiple_spans_and_preserves_styles() {
        let span1 = TextSpan::plain("hello "); // 6 bytes (0..6)
        let span2 = TextSpan::styled("world", StyleSpec::new().bold()); // 5 bytes (6..11)

        let view = StreamView::exact_text(
            StreamRange::new(StreamOffset::ZERO, StreamOffset::new(11)),
            vec![span1, span2],
        );

        let compiled = compile_stream(&view, 20, StreamOffset::new(11));
        assert_eq!(compiled.rows.len(), 1);
        assert_eq!(compiled.committable_prefix_rows, 1);

        // Row spans preserve bold style on "world"
        let line = &compiled.rows[0];
        assert_eq!(line.spans.len(), 2);
        assert_eq!(line.spans[0].content, "hello ");
        assert_eq!(line.spans[1].content, "world");
        assert!(line.spans[1].style.add_modifier.contains(Modifier::BOLD));

        // Row commit offset is the accumulated exact end (11)
        assert_eq!(
            compiled.commit[0],
            StreamRowCommit::Exact(StreamOffset::new(11))
        );
    }

    #[test]
    fn exact_text_combining_mark_across_spans() {
        // 'e' (1 byte) + combining acute (2 bytes) = 3 bytes
        let span1 = TextSpan::plain("e");
        let span2 = TextSpan::plain("\u{0301}");

        let view = StreamView::exact_text(
            StreamRange::new(StreamOffset::ZERO, StreamOffset::new(3)),
            vec![span1, span2],
        );

        let compiled = compile_stream(&view, 20, StreamOffset::new(3));
        assert_eq!(compiled.rows.len(), 1);
        assert_eq!(
            compiled.commit[0],
            StreamRowCommit::Exact(StreamOffset::new(3))
        );
    }

    #[test]
    fn exact_text_zwj_split_across_spans() {
        // 👩 (4 bytes) + ZWJ (3 bytes) + 💻 (4 bytes) = 11 bytes
        let span1 = TextSpan::plain("👩");
        let span2 = TextSpan::plain("\u{200D}");
        let span3 = TextSpan::plain("💻");

        let view = StreamView::exact_text(
            StreamRange::new(StreamOffset::ZERO, StreamOffset::new(11)),
            vec![span1, span2, span3],
        );

        let compiled = compile_stream(&view, 20, StreamOffset::new(11));
        assert_eq!(compiled.rows.len(), 1);
        assert_eq!(compiled.rows[0].width(), 2);
        assert_eq!(
            compiled.commit[0],
            StreamRowCommit::Exact(StreamOffset::new(11))
        );
    }

    #[test]
    fn thinking_style_transition_preserves_muted_and_italic() {
        let span_thinking = TextSpan::styled(
            "reasoning\n",
            StyleSpec::new()
                .foreground(ColorSpec::Theme(ThemeKey::from("text.muted")))
                .italic()
                .dim(),
        );
        let span_text = TextSpan::plain("answer");

        let view = StreamView::exact_text(
            StreamRange::new(StreamOffset::ZERO, StreamOffset::new(16)),
            vec![span_thinking, span_text],
        );

        let compiled = compile_stream(&view, 20, StreamOffset::new(16));
        assert_eq!(compiled.rows.len(), 2);

        // Row 0 has thinking styling (italic & dim)
        assert!(
            compiled.rows[0].spans[0]
                .style
                .add_modifier
                .contains(Modifier::ITALIC)
        );
        assert!(
            compiled.rows[0].spans[0]
                .style
                .add_modifier
                .contains(Modifier::DIM)
        );
        assert_eq!(
            compiled.commit[0],
            StreamRowCommit::Exact(StreamOffset::new(9))
        ); // "reasoning"

        // Row 1 is plain text
        assert_eq!(
            compiled.commit[1],
            StreamRowCommit::Exact(StreamOffset::new(16))
        ); // "reasoning\nanswer"
    }

    #[test]
    fn plan_commit_skips_fully_committed_atomic_group() {
        // Test A: atomic 3 physical rows, commit all 3, plan again from same compiled object => 0 rows
        let atomic_view = View::column(
            vec![
                View::text("heading").into_view(),
                View::text("body row 1").into_view(),
                View::text("body row 2").into_view(),
            ],
            0,
        );

        let view = StreamView::atomic(
            StreamRange::new(StreamOffset::ZERO, StreamOffset::new(30)),
            atomic_view,
        );

        let compiled = compile_stream(&view, 20, StreamOffset::new(30));
        assert_eq!(compiled.rows.len(), 3);
        assert_eq!(compiled.committable_prefix_rows, 3);

        // First commit: commit all 3 rows
        let plan1 = plan_commit(&compiled, 3, None, StreamOffset::ZERO);
        assert_eq!(plan1.payload.rows_to_write(), 3);
        assert_eq!(plan1.next_committed_through, StreamOffset::new(30));
        assert_eq!(plan1.next_partial, None);

        // Plan again from same compiled object without compacting
        let plan2 = plan_commit(&compiled, 3, None, StreamOffset::new(30));
        assert_eq!(plan2.payload.rows_to_write(), 0);
        assert_eq!(plan2.next_committed_through, StreamOffset::new(30));
    }

    #[test]
    fn plan_commit_mixed_provenance_skips_exact_and_atomic_groups() {
        // Test B: Exact 0..5, Atomic 5..20, Exact 20..24, commit through 20, plan again
        let mut view = StreamView::empty();
        view.push(StreamNode::exact_text(
            StreamRange::new(StreamOffset::new(0), StreamOffset::new(5)),
            vec![TextSpan::plain("first")],
        ));
        view.push(StreamNode::atomic(
            StreamRange::new(StreamOffset::new(5), StreamOffset::new(20)),
            View::column(
                vec![
                    View::text("mid 1").into_view(),
                    View::text("mid 2").into_view(),
                ],
                0,
            ),
        ));
        view.push(StreamNode::exact_text(
            StreamRange::new(StreamOffset::new(20), StreamOffset::new(24)),
            vec![TextSpan::plain("last")],
        ));

        let compiled = compile_stream(&view, 20, StreamOffset::new(24));
        assert_eq!(compiled.rows.len(), 4);

        // Commit through source 20 (skips first Exact and whole Atomic group)
        let plan = plan_commit(&compiled, 10, None, StreamOffset::new(20));
        // Only the final Exact row is written
        assert_eq!(plan.payload.rows_to_write(), 1);
        assert_eq!(plan.next_committed_through, StreamOffset::new(24));
    }

    #[test]
    fn compile_stream_exact_text_matches_view_compiler_identically() {
        let compiler = ViewCompiler::default();

        let cases = vec![
            (
                "plain wrapped text",
                vec![TextSpan::plain("hello world from exact stream")],
            ),
            (
                "styled spans",
                vec![
                    TextSpan::plain("plain "),
                    TextSpan::styled("bold italic", StyleSpec::new().bold().italic()),
                ],
            ),
            (
                "combining mark across spans",
                vec![TextSpan::plain("e"), TextSpan::plain("\u{0301}")],
            ),
            (
                "ZWJ emoji across spans",
                vec![
                    TextSpan::plain("👩"),
                    TextSpan::plain("\u{200D}"),
                    TextSpan::plain("💻"),
                ],
            ),
            (
                "hard newline",
                vec![TextSpan::plain("line 1\nline 2\nline 3")],
            ),
        ];

        for width in [1u16, 2, 3, 10, 80] {
            for (label, spans) in &cases {
                let text_view = View::styled_text(spans.clone()).into_view();
                let layout_block = compiler.compile(&text_view, width);

                let total_len = spans.iter().map(|s| s.text.len() as u64).sum();
                let stream_view = StreamView::exact_text(
                    StreamRange::new(StreamOffset::ZERO, StreamOffset::new(total_len)),
                    spans.clone(),
                );
                let compiled_stream =
                    compile_stream(&stream_view, width, StreamOffset::new(total_len));

                // If the entire row cannot fit within width (e.g. 2-cell wide emoji in width 1),
                // layout_text correctly clips the wide grapheme while stream compilation retains
                // the physical line marked as fits: false (non-committable).
                if !compiled_stream.rows.is_empty()
                    && compiled_stream.rows[0].width() > usize::from(width)
                {
                    continue;
                }

                assert_eq!(
                    layout_block.rows.len(),
                    compiled_stream.rows.len(),
                    "Row count mismatch for '{}' at width {}",
                    label,
                    width
                );

                for (r_idx, (layout_row, stream_row)) in layout_block
                    .rows
                    .iter()
                    .zip(&compiled_stream.rows)
                    .enumerate()
                {
                    assert_eq!(
                        layout_row.spans.len(),
                        stream_row.spans.len(),
                        "Span count mismatch for '{}' at width {}, row {}",
                        label,
                        width,
                        r_idx
                    );
                    for (s_idx, (ls, ss)) in
                        layout_row.spans.iter().zip(&stream_row.spans).enumerate()
                    {
                        assert_eq!(
                            ls.content, ss.content,
                            "Content mismatch for '{}' at width {}, row {}, span {}",
                            label, width, r_idx, s_idx
                        );
                        assert_eq!(
                            ls.style, ss.style,
                            "Style mismatch for '{}' at width {}, row {}, span {}",
                            label, width, r_idx, s_idx
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn sealed_stream_static_lowering_matches_stream_compilation() {
        let mut stream = crate::transcript::AssistantStream::new();
        stream.push_delta(crate::transcript::SegmentKind::Thinking, "thinking\n");
        stream.push_delta(
            crate::transcript::SegmentKind::Text,
            "# Heading\n- list item\n\nwide 漢 e\u{301}\n**bold**",
        );
        stream.seal();
        let snapshot = stream.snapshot();
        assert!(snapshot.validate());

        for width in [6u16, 20, 80] {
            let compiled = compile_stream(&snapshot.view, width, snapshot.source_end);
            let lowered =
                ViewCompiler::default().compile(&snapshot.view.clone().into_static_view(), width);
            assert_eq!(compiled.rows, lowered.rows, "row mismatch at width {width}");
        }
    }

    #[test]
    fn compile_stream_atomic_partial_freezes_physical_rows() {
        let atomic_view = View::column(
            vec![
                View::text("heading").into_view(),
                View::text("body row 1").into_view(),
                View::text("body row 2").into_view(),
            ],
            0,
        );

        let view = StreamView::atomic(
            StreamRange::new(StreamOffset::new(0), StreamOffset::new(30)),
            atomic_view,
        );

        let compiled = compile_stream(&view, 20, StreamOffset::new(30));
        assert_eq!(compiled.rows.len(), 3);
        assert_eq!(compiled.committable_prefix_rows, 3);

        // Request 1 row of the 3-row atomic group
        let plan = plan_commit(&compiled, 1, None, StreamOffset::new(0));
        assert_eq!(plan.payload.rows_to_write(), 1);
        // Source offset does not advance yet!
        assert_eq!(plan.next_committed_through, StreamOffset::new(0));
        assert!(matches!(
            plan.next_partial,
            Some(StreamPartialCommit::FrozenAtomic {
                committed_rows: 1,
                source_end,
                ..
            }) if source_end == StreamOffset::new(30)
        ));

        // Commit remaining 2 rows from partial state
        let plan2 = plan_commit(
            &compiled,
            2,
            plan.next_partial.as_ref(),
            StreamOffset::new(0),
        );
        assert_eq!(plan2.payload.rows_to_write(), 2);
        assert_eq!(plan2.next_committed_through, StreamOffset::new(30));
        assert_eq!(plan2.next_partial, None);
    }

    #[derive(Debug)]
    struct MockStream {
        source: String,
        stable_len: usize,
        sealed: bool,
        revision: StreamRevision,
    }

    impl StreamingContent for MockStream {
        fn snapshot(&self) -> StreamSnapshot {
            let len = self.source.len() as u64;
            let stable = if self.sealed {
                len
            } else {
                self.stable_len as u64
            };
            StreamSnapshot {
                revision: self.revision,
                source_base: StreamOffset::ZERO,
                source_end: StreamOffset::new(len),
                stable_through: StreamOffset::new(stable),
                view: StreamView::exact_text(
                    StreamRange::new(StreamOffset::ZERO, StreamOffset::new(len)),
                    vec![TextSpan::plain(&self.source)],
                ),
            }
        }

        fn seal(&mut self) {
            if self.sealed {
                return;
            }
            self.sealed = true;
            self.revision = self.revision.next();
        }

        fn is_sealed(&self) -> bool {
            self.sealed
        }
    }

    #[derive(Debug)]
    struct AtomicMockStream {
        source_base: StreamOffset,
        sealed: bool,
        revision: StreamRevision,
    }

    impl StreamingContent for AtomicMockStream {
        fn snapshot(&self) -> StreamSnapshot {
            let source_end = StreamOffset::new(42);
            let suffix = StreamNode::exact_text(
                StreamRange::new(StreamOffset::new(30), source_end),
                vec![TextSpan::plain("suffix-after")],
            );
            let view = if self.source_base < StreamOffset::new(30) {
                StreamView::new(vec![
                    StreamNode::atomic(
                        StreamRange::new(StreamOffset::ZERO, StreamOffset::new(30)),
                        View::column(
                            vec![
                                View::text("A0").into_view(),
                                View::text("A1").into_view(),
                                View::text("A2").into_view(),
                            ],
                            0,
                        ),
                    ),
                    suffix,
                ])
            } else if self.source_base < source_end {
                StreamView::new(vec![suffix])
            } else {
                StreamView::empty()
            };

            StreamSnapshot {
                revision: self.revision,
                source_base: self.source_base,
                source_end,
                stable_through: source_end,
                view,
            }
        }

        fn compact_before(&mut self, offset: StreamOffset) {
            self.source_base = offset;
            self.revision = self.revision.next();
        }

        fn seal(&mut self) {
            self.sealed = true;
            self.revision = self.revision.next();
        }

        fn is_sealed(&self) -> bool {
            self.sealed
        }
    }

    #[test]
    fn frozen_atomic_live_projection_survives_resize() {
        let mut hosted = HostedStream::new(
            crate::transcript::model::TranscriptId(42),
            AtomicMockStream {
                source_base: StreamOffset::ZERO,
                sealed: false,
                revision: StreamRevision::ZERO,
            },
            LeadingBoundaryState::None,
        );

        let prepared1 = hosted.prepare_frame(10, 1);
        assert!(prepared1.live_rows.len() >= 4);
        assert_eq!(prepared1.history.semantic_plan.payload.rows_to_write(), 1);
        assert_eq!(
            prepared1.history.semantic_plan.next_committed_through,
            StreamOffset::ZERO
        );
        hosted.apply_commit_success(prepared1.history);

        let (frozen_rows, committed_rows, source_end) = match hosted.partial.as_ref() {
            Some(StreamPartialCommit::FrozenAtomic {
                rows,
                committed_rows,
                source_end,
                ..
            }) => (rows.clone(), *committed_rows, *source_end),
            other => panic!("expected FrozenAtomic partial, got {other:?}"),
        };
        assert_eq!(committed_rows, 1);
        assert_eq!(hosted.committed_through, StreamOffset::ZERO);

        hosted.seal();
        let prepared2 = hosted.prepare_frame(80, 10);
        assert_eq!(&prepared2.live_rows[..2], &frozen_rows.as_slice()[1..]);
        assert_eq!(prepared2.live_rows[2].spans[0].content, "suffix-after");
        assert!(matches!(
            prepared2.history.semantic_plan.payload,
            CommitPayload::Frozen { .. }
        ));
        hosted.apply_commit_success(prepared2.history);

        assert_eq!(hosted.committed_through, source_end);
        assert_eq!(hosted.partial, None);
        assert_eq!(hosted.snapshot().source_base, source_end);

        let prepared3 = hosted.prepare_frame(80, 10);
        assert_eq!(
            prepared3.history.semantic_plan.next_committed_through,
            StreamOffset::new(42)
        );
        hosted.apply_commit_success(prepared3.history);
        assert!(hosted.can_handoff());
    }

    #[test]
    fn leading_boundary_commits_with_first_atomic_body_row() {
        let mut hosted = HostedStream::new(
            crate::transcript::model::TranscriptId(7),
            AtomicMockStream {
                source_base: StreamOffset::ZERO,
                sealed: false,
                revision: StreamRevision::ZERO,
            },
            LeadingBoundaryState::Pending,
        );

        let prepared = hosted.prepare_frame(80, 2);
        assert_eq!(prepared.history.rows.len(), 2);
        assert!(prepared.history.rows.as_slice()[0].spans.is_empty());
        assert_eq!(prepared.history.rows.as_slice()[1].spans[0].content, "A0");
        assert!(prepared.history.commit_leading_boundary);
        hosted.apply_commit_success(prepared.history);

        assert_eq!(hosted.leading_boundary, LeadingBoundaryState::Committed);
        assert_eq!(hosted.committed_through, StreamOffset::ZERO);
        assert!(matches!(
            hosted.partial,
            Some(StreamPartialCommit::FrozenAtomic {
                committed_rows: 1,
                ..
            })
        ));

        let reprepared = hosted.prepare_frame(20, 0);
        assert_eq!(reprepared.live_rows[0].spans[0].content, "A1");
        assert_ne!(reprepared.live_rows[0], ratatui::text::Line::default());
        assert!(!reprepared.history.commit_leading_boundary);
    }

    #[test]
    fn first_hosted_unit_has_no_leading_boundary_transaction() {
        let mut hosted = HostedStream::new(
            crate::transcript::model::TranscriptId(8),
            MockStream {
                source: "hello world".to_string(),
                stable_len: 11,
                sealed: false,
                revision: StreamRevision::ZERO,
            },
            LeadingBoundaryState::None,
        );

        let prepared = hosted.prepare_frame(80, 1);
        assert_eq!(prepared.history.rows.len(), 1);
        assert!(!prepared.history.commit_leading_boundary);
        assert_ne!(
            prepared.history.rows.as_slice()[0],
            ratatui::text::Line::default()
        );
    }

    #[test]
    fn leading_boundary_failure_is_transactional() {
        let mut hosted = HostedStream::new(
            crate::transcript::model::TranscriptId(10),
            MockStream {
                source: "hello world".to_string(),
                stable_len: 11,
                sealed: false,
                revision: StreamRevision::ZERO,
            },
            LeadingBoundaryState::Pending,
        );

        let prepared = hosted.prepare_frame(6, 2);
        let expected_rows = prepared.history.rows.clone();
        // Simulate terminal failure by dropping the prepared transaction.
        let reprepared = hosted.prepare_frame(6, 2);
        assert_eq!(reprepared.history.rows, expected_rows);
        assert_eq!(hosted.leading_boundary, LeadingBoundaryState::Pending);
        assert_eq!(hosted.committed_through, StreamOffset::ZERO);
        assert!(hosted.partial.is_none());
    }

    #[test]
    fn leading_boundary_exact_first_spill_is_committed_once() {
        let mut hosted = HostedStream::new(
            crate::transcript::model::TranscriptId(9),
            MockStream {
                source: "hello world".to_string(),
                stable_len: 11,
                sealed: false,
                revision: StreamRevision::ZERO,
            },
            LeadingBoundaryState::Pending,
        );

        let first = hosted.prepare_frame(6, 2);
        assert_eq!(first.history.rows.len(), 2);
        assert!(first.history.rows.as_slice()[0].spans.is_empty());
        assert!(
            first.history.rows.as_slice()[1]
                .spans
                .iter()
                .any(|span| span.content.contains("hello"))
        );
        hosted.apply_commit_success(first.history);
        assert_eq!(hosted.leading_boundary, LeadingBoundaryState::Committed);

        let second = hosted.prepare_frame(6, 1);
        assert!(!second.history.commit_leading_boundary);
        assert!(
            second
                .history
                .rows
                .as_slice()
                .iter()
                .all(|row| { row.spans.iter().all(|span| !span.content.is_empty()) })
        );
    }

    #[test]
    fn hosted_assistant_stream_compacts_after_success_and_preserves_atomic_transition() {
        use crate::transcript::model::TranscriptId;
        use crate::transcript::{AssistantStream, SegmentKind};

        let unit_id = TranscriptId(42);
        let mut hosted =
            HostedStream::new(unit_id, AssistantStream::new(), LeadingBoundaryState::None);

        hosted
            .content_mut()
            .push_delta(SegmentKind::Text, "hello **bo");

        let prepared1 = hosted.prepare_frame(6, 1);
        let plan1 = prepared1.history.semantic_plan.clone();
        assert_eq!(plan1.next_committed_through, StreamOffset::new(6));
        assert_eq!(plan1.payload.rows_to_write(), 1);

        assert_eq!(hosted.committed_through, StreamOffset::ZERO);
        assert_eq!(hosted.snapshot().source_base, StreamOffset::ZERO);

        hosted.apply_commit_success(prepared1.history);

        assert_eq!(hosted.committed_through, StreamOffset::new(6));
        let after_commit = hosted.snapshot();
        assert_eq!(after_commit.source_base, StreamOffset::new(6));

        hosted.content_mut().push_delta(SegmentKind::Text, "ld**");

        let after_append = hosted.snapshot();
        assert!(after_append.validate());
        assert_eq!(after_append.source_base, StreamOffset::new(6));
        assert_eq!(after_append.source_end, StreamOffset::new(14));
        assert_eq!(
            after_append.view.nodes[0].owned_range(),
            StreamRange::new(StreamOffset::new(6), StreamOffset::new(14)),
        );

        hosted.seal();
        let prepared2 = hosted.prepare_frame(80, 10);
        let plan2 = prepared2.history.semantic_plan.clone();
        assert_eq!(plan2.next_committed_through, StreamOffset::new(14));

        hosted.apply_commit_success(prepared2.history);
        assert!(hosted.can_handoff());
        assert_eq!(hosted.partial, None);
        assert_eq!(hosted.snapshot().source_base, StreamOffset::new(14));
    }

    #[test]
    fn hosted_stream_lifecycle_survives_sealing() {
        use crate::transcript::model::TranscriptId;

        let unit_id = TranscriptId(42);
        let mock = MockStream {
            source: "line 1\nline 2\nline 3".to_string(),
            stable_len: 13, // covers line 1 (6) + \n (1) + line 2 (6)
            sealed: false,
            revision: StreamRevision::ZERO,
        };

        let mut hosted = HostedStream::new(unit_id, mock, LeadingBoundaryState::None);
        assert_eq!(hosted.committed_through, StreamOffset::ZERO);
        assert!(!hosted.is_sealed());

        // Prepare plan at width 20 requesting 10 rows
        let prepared = hosted.prepare_frame(20, 10);
        let plan = prepared.history.semantic_plan.clone();
        // lines 1 and 2 are stable (2 rows)
        assert_eq!(plan.payload.rows_to_write(), 2);
        assert_eq!(plan.next_committed_through, StreamOffset::new(13));

        // SIMULATE FAILURE: terminal fails to write rows -> apply_commit_success is NOT called!
        assert_eq!(hosted.committed_through, StreamOffset::ZERO);
        assert_eq!(hosted.snapshot().source_base, StreamOffset::ZERO);

        // SIMULATE SUCCESS: apply commit plan
        hosted.apply_commit_success(prepared.history);
        assert_eq!(hosted.committed_through, StreamOffset::new(13));

        // Seal stream in-place: HostedStream retains unit_id, committed_through, and partial state!
        hosted.seal();
        assert!(hosted.is_sealed());
        assert_eq!(hosted.unit_id, unit_id);
        assert_eq!(hosted.committed_through, StreamOffset::new(13));

        // Drain remaining line 3 after sealing
        let prepared2 = hosted.prepare_frame(20, 10);
        let plan2 = prepared2.history.semantic_plan.clone();
        assert_eq!(plan2.payload.rows_to_write(), 1);
        assert_eq!(plan2.next_committed_through, StreamOffset::new(20));
        hosted.apply_commit_success(prepared2.history);

        assert!(hosted.can_handoff());
    }

    #[test]
    fn hosted_stream_seal_before_first_snapshot_can_handoff_before_history_drain() {
        use crate::transcript::model::TranscriptId;

        let unit_id = TranscriptId(42);
        let mock = MockStream {
            source: "abc".into(),
            stable_len: 3,
            sealed: false,
            revision: StreamRevision::ZERO,
        };

        let mut hosted = HostedStream::new(unit_id, mock, LeadingBoundaryState::None);
        // IMPORTANT: never call snapshot() or prepare_frame() before seal
        hosted.seal();
        assert!(hosted.can_handoff());

        let prepared = hosted.prepare_frame(80, 10);
        assert_eq!(prepared.history.semantic_plan.payload.rows_to_write(), 1);
        hosted.apply_commit_success(prepared.history);
        assert!(hosted.can_handoff());
    }

    // --- Fix 3: contiguity validation tests ---

    #[test]
    fn contiguity_rejects_gap_between_nodes() {
        // node 1 = 0..3, node 2 = 10..13 -- gap at 3..10
        let snapshot = StreamSnapshot {
            revision: StreamRevision::new(1),
            source_base: StreamOffset::ZERO,
            source_end: StreamOffset::new(13),
            stable_through: StreamOffset::new(13),
            view: StreamView::new(vec![
                StreamNode::exact_text(
                    StreamRange::new(StreamOffset::new(0), StreamOffset::new(3)),
                    vec![TextSpan::plain("abc")],
                ),
                StreamNode::exact_text(
                    StreamRange::new(StreamOffset::new(10), StreamOffset::new(13)),
                    vec![TextSpan::plain("def")],
                ),
            ]),
        };
        assert!(!snapshot.validate());
    }

    #[test]
    fn contiguity_rejects_trailing_uncovered_source() {
        // source_end = 10, only node = 0..5
        let snapshot = StreamSnapshot {
            revision: StreamRevision::new(1),
            source_base: StreamOffset::ZERO,
            source_end: StreamOffset::new(10),
            stable_through: StreamOffset::new(10),
            view: StreamView::new(vec![StreamNode::exact_text(
                StreamRange::new(StreamOffset::new(0), StreamOffset::new(5)),
                vec![TextSpan::plain("hello")],
            )]),
        };
        assert!(!snapshot.validate());
    }

    #[test]
    fn contiguity_rejects_leading_gap() {
        // source_base = 5, first node starts at 6
        let snapshot = StreamSnapshot {
            revision: StreamRevision::new(1),
            source_base: StreamOffset::new(5),
            source_end: StreamOffset::new(9),
            stable_through: StreamOffset::new(9),
            view: StreamView::new(vec![StreamNode::exact_text(
                StreamRange::new(StreamOffset::new(6), StreamOffset::new(9)),
                vec![TextSpan::plain("abc")],
            )]),
        };
        assert!(!snapshot.validate());
    }

    #[test]
    fn contiguity_accepts_typed_newline_chain() {
        // Exact: visible 0..3, HardNewline, owned 0..4
        // Next Exact: visible 4..7, None, owned 4..7
        // source_end = 7
        let snapshot = StreamSnapshot {
            revision: StreamRevision::new(1),
            source_base: StreamOffset::ZERO,
            source_end: StreamOffset::new(7),
            stable_through: StreamOffset::new(7),
            view: StreamView::new(vec![
                StreamNode::exact_line(
                    StreamRange::new(StreamOffset::new(0), StreamOffset::new(3)),
                    vec![TextSpan::plain("abc")],
                    true,
                ),
                StreamNode::exact_text(
                    StreamRange::new(StreamOffset::new(4), StreamOffset::new(7)),
                    vec![TextSpan::plain("def")],
                ),
            ]),
        };
        assert!(snapshot.validate());
    }

    #[test]
    fn contiguity_accepts_atomic_then_exact() {
        // Atomic 0..9, Exact 9..14, source_end = 14
        let snapshot = StreamSnapshot {
            revision: StreamRevision::new(1),
            source_base: StreamOffset::ZERO,
            source_end: StreamOffset::new(14),
            stable_through: StreamOffset::new(14),
            view: StreamView::new(vec![
                StreamNode::atomic(
                    StreamRange::new(StreamOffset::new(0), StreamOffset::new(9)),
                    View::text("bold").into_view(),
                ),
                StreamNode::exact_text(
                    StreamRange::new(StreamOffset::new(9), StreamOffset::new(14)),
                    vec![TextSpan::plain("plain")],
                ),
            ]),
        };
        assert!(snapshot.validate());
    }

    #[test]
    fn contiguity_empty_stream_valid_iff_base_equals_end() {
        let valid = StreamSnapshot {
            revision: StreamRevision::ZERO,
            source_base: StreamOffset::ZERO,
            source_end: StreamOffset::ZERO,
            stable_through: StreamOffset::ZERO,
            view: StreamView::empty(),
        };
        assert!(valid.validate());

        let invalid = StreamSnapshot {
            revision: StreamRevision::ZERO,
            source_base: StreamOffset::ZERO,
            source_end: StreamOffset::new(5),
            stable_through: StreamOffset::new(5),
            view: StreamView::empty(),
        };
        assert!(!invalid.validate());
    }
}
