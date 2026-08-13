//! Assistant pacing/source lifecycle over the generic text pipeline.

use crate::transcript::pipeline::{AssistantPipeline, assistant_presentation, assistant_renderer};
use crate::transcript::semantic::{AssistantSegment, SegmentKind, think_to_text_newline};
use std::time::Instant;

use iyon_tui::projection::ProjectionBuilder;
use iyon_tui::stream::{
    StreamOffset, StreamRange, StreamRevision, StreamSnapshot, StreamSnapshotBuilder,
    StreamingSource,
};
use iyon_tui::{Projection, TextContent};

#[derive(Debug, Clone)]
pub(crate) struct AssistantPacingAtom {
    pub(crate) kind: SegmentKind,
    pub(crate) text: String,
}

#[derive(Debug)]
pub(crate) struct AssistantStream {
    source_base: StreamOffset,
    received_end: StreamOffset,
    received_segments: Vec<AssistantSegment>,
    pacing_atoms: Vec<(StreamRange, AssistantPacingAtom)>,
    pipeline: AssistantPipeline,
    pacing_input: Projection<AssistantPacingAtom>,
    semantic: Projection<TextContent>,
    snapshot: StreamSnapshot,
    renderer: iyon_tui::TextRenderer,
    revision: StreamRevision,
    sealed: bool,
}

impl AssistantStream {
    pub(crate) fn new() -> Self {
        let pacing_input = ProjectionBuilder::new(
            StreamOffset::ZERO,
            StreamOffset::ZERO,
            StreamOffset::ZERO,
            false,
        )
        .finish()
        .expect("empty pacing projection is valid");
        let semantic = pacing_input.map_ref(|_| TextContent::raw(""));
        let snapshot = StreamSnapshotBuilder::new(
            StreamRevision::ZERO,
            StreamOffset::ZERO,
            StreamOffset::ZERO,
            StreamOffset::ZERO,
        )
        .finish()
        .expect("empty assistant snapshot is valid");
        Self {
            source_base: StreamOffset::ZERO,
            received_end: StreamOffset::ZERO,
            received_segments: Vec::new(),
            pacing_atoms: Vec::new(),
            pipeline: AssistantPipeline::default(),
            pacing_input,
            semantic,
            snapshot,
            renderer: assistant_renderer(),
            revision: StreamRevision::ZERO,
            sealed: false,
        }
    }

    pub(crate) fn push_delta_paced(&mut self, kind: SegmentKind, chunk: &str) {
        self.receive_delta(kind, chunk);
        self.refresh(false);
    }

    pub(crate) fn semantic_projection_for_view(&self) -> Projection<TextContent> {
        self.semantic.clone()
    }

    fn receive_delta(&mut self, kind: SegmentKind, chunk: &str) {
        assert!(!self.sealed, "cannot append to sealed AssistantStream");
        if chunk.is_empty() {
            return;
        }
        let chunk = think_to_text_newline(&self.received_segments, kind, chunk).into_owned();
        append_segment(&mut self.received_segments, kind, &chunk);
        let mut cursor = self.received_end;
        for character in chunk.chars() {
            let end = cursor.saturating_add(character.len_utf8() as u64);
            self.pacing_atoms.push((
                StreamRange::new(cursor, end),
                AssistantPacingAtom {
                    kind,
                    text: character.to_string(),
                },
            ));
            cursor = end;
        }
        self.received_end = cursor;
    }

    fn rebuild_input(&mut self) {
        let mut builder = ProjectionBuilder::new(
            self.source_base,
            self.received_end,
            self.received_end,
            self.sealed,
        );
        for (range, atom) in &self.pacing_atoms {
            builder = builder.emit(*range, atom.clone());
        }
        self.pacing_input = builder
            .finish()
            .expect("assistant pacing projection is valid");
    }

    fn refresh(&mut self, force_revision: bool) {
        self.rebuild_input();
        self.semantic = self
            .pipeline
            .project(&self.pacing_input)
            .expect("assistant generic pipeline must project valid source");
        let next = self.build_snapshot();
        if force_revision || next != self.snapshot {
            self.revision = self.revision.next();
            self.snapshot = self.build_snapshot();
        }
    }

    fn build_snapshot(&self) -> StreamSnapshot {
        let chunks = assistant_presentation(&self.semantic, &self.renderer);
        let mut builder = StreamSnapshotBuilder::new(
            self.revision,
            self.semantic.source_base(),
            self.semantic.stable_through(),
            self.semantic.source_end(),
        );
        if chunks.is_empty() {
            if self.semantic.source_base() != self.semantic.source_end() {
                builder = builder
                    .atomic(
                        StreamRange::new(self.semantic.source_base(), self.semantic.source_end()),
                        iyon_tui::View::spacer(0).padding(iyon_tui::Insets::horizontal(2)),
                    )
                    .expect("assistant empty semantic ownership is valid");
            }
        } else {
            let chunk_count = chunks.len();
            for (index, chunk) in chunks.into_iter().enumerate() {
                let end = if index + 1 == chunk_count {
                    self.semantic.source_end()
                } else {
                    chunk.source.end()
                };
                let range = StreamRange::new(chunk.source.start(), end);
                builder = builder
                    .atomic(range, chunk.view)
                    .expect("assistant atomic chunk is valid");
            }
        }
        builder
            .finish()
            .expect("assistant snapshot coverage is valid")
    }

    fn compact_atoms_before(&mut self, restart: StreamOffset) {
        let removed = self
            .pacing_atoms
            .partition_point(|(range, _)| range.end() <= restart);
        self.pacing_atoms.drain(..removed);
    }
}

fn append_segment(segments: &mut Vec<AssistantSegment>, kind: SegmentKind, text: &str) {
    if text.is_empty() {
        return;
    }
    match kind {
        SegmentKind::Text => match segments.last_mut() {
            Some(AssistantSegment::Text(existing)) => existing.push_str(text),
            _ => segments.push(AssistantSegment::Text(text.to_owned())),
        },
        SegmentKind::Thinking => match segments.last_mut() {
            Some(AssistantSegment::Thinking(existing)) => existing.push_str(text),
            _ => segments.push(AssistantSegment::Thinking(text.to_owned())),
        },
    }
}

impl StreamingSource for AssistantStream {
    fn snapshot(&self) -> StreamSnapshot {
        self.snapshot.clone()
    }

    fn compact_before(&mut self, offset: StreamOffset) {
        if offset <= self.source_base {
            return;
        }
        let restart = self.pipeline.restart_from(offset).max(self.source_base);
        debug_assert!(restart <= self.semantic.source_end());
        self.source_base = restart;
        self.compact_atoms_before(restart);
        self.refresh(true);
    }

    fn seal(&mut self) {
        if self.sealed {
            return;
        }
        self.sealed = true;
        self.refresh(true);
    }

    fn is_sealed(&self) -> bool {
        self.sealed
    }

    fn next_wakeup(&self) -> Option<Instant> {
        (!self.sealed)
            .then(|| self.pipeline.next_wakeup())
            .flatten()
    }

    fn advance(&mut self, now: Instant) -> bool {
        if self.sealed || !self.pipeline.advance(now) {
            return false;
        }
        let previous = self.snapshot.clone();
        self.refresh(false);
        self.snapshot != previous
    }
}

#[cfg(test)]
mod tests;
