//! Generic semantic stream model and resident/source coordination.

use super::{
    StreamOffset, StreamRange, StreamSnapshot, StreamView, StreamingSource,
    compile::{CompiledStream, compile_stream},
    node::{StreamSliceError, semantic_slice_nodes},
    resident::ResidentPrefix,
    validate::StreamValidationError,
};

#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamModelError {
    Validation(StreamValidationError),
    RevisionRegressed,
    SourceBaseRegressed,
    SourceEndRegressed,
    StabilityRegressed,
    ChangedWithoutRevision,
    SourceBeforeResident,
    CompactionChangedCoordinates,
    CompactionChangedSemanticSuffix,
    SourceNotSealed,
    UnstableAfterSeal,
}

impl std::fmt::Display for StreamModelError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "stream model error: {self:?}")
    }
}

impl std::error::Error for StreamModelError {}

pub(crate) struct StreamModel<S: StreamingSource> {
    source: S,
    resident: ResidentPrefix,
    current: StreamSnapshot,
}

impl<S: StreamingSource> StreamModel<S> {
    pub(crate) fn new(source: S) -> Result<Self, StreamModelError> {
        let current = source.snapshot();
        current.validate().map_err(StreamModelError::Validation)?;
        Ok(Self {
            resident: ResidentPrefix::new(current.source_base),
            source,
            current,
        })
    }

    pub(crate) fn source(&self) -> &S {
        &self.source
    }

    pub(crate) fn source_mut(&mut self) -> &mut S {
        &mut self.source
    }

    pub(crate) fn snapshot(&self) -> &StreamSnapshot {
        &self.current
    }

    pub(crate) fn resident(&self) -> &ResidentPrefix {
        &self.resident
    }

    pub(crate) fn restore_semantic_state(
        &mut self,
        resident: ResidentPrefix,
        current: StreamSnapshot,
    ) {
        self.resident = resident;
        self.current = current;
    }

    pub(crate) fn refresh(&mut self) -> Result<(), StreamModelError> {
        let observed = self.source.snapshot();
        Self::validate_transition(&self.current, &observed)?;
        if observed.source_base > self.resident.end() {
            return Err(StreamModelError::SourceBeforeResident);
        }

        let resident_end = self.resident.end();
        let mut staged_nodes = Vec::new();
        let mut captured_end = resident_end;
        for node in observed.view.suffix_from(resident_end).nodes {
            if node.owned_range().end > observed.stable_through {
                break;
            }
            captured_end = node.owned_range().end;
            staged_nodes.push(node);
        }

        let next = if captured_end == resident_end {
            observed
        } else {
            let before_suffix = observed.view.suffix_from(captured_end);
            let before_end = observed.source_end;
            let before_stable = observed.stable_through;
            self.source.compact_before(captured_end);
            let compacted = self.source.snapshot();
            Self::validate_transition(&observed, &compacted)?;
            if compacted.source_base > captured_end {
                return Err(StreamModelError::SourceBeforeResident);
            }
            if compacted.source_end != before_end || compacted.stable_through != before_stable {
                return Err(StreamModelError::CompactionChangedCoordinates);
            }
            if compacted.view.suffix_from(captured_end) != before_suffix {
                return Err(StreamModelError::CompactionChangedSemanticSuffix);
            }
            compacted
        };

        if next.source_base > captured_end {
            return Err(StreamModelError::SourceBeforeResident);
        }
        for node in staged_nodes {
            self.resident.push(node);
        }
        self.current = next;
        Ok(())
    }

    pub(crate) fn seal(&mut self) -> Result<(), StreamModelError> {
        self.source.seal();
        self.refresh()?;
        if !self.source.is_sealed() {
            return Err(StreamModelError::SourceNotSealed);
        }
        if self.current.stable_through != self.current.source_end {
            return Err(StreamModelError::UnstableAfterSeal);
        }
        self.refresh()?;
        if self.resident.end() != self.current.source_end {
            return Err(StreamModelError::UnstableAfterSeal);
        }
        Ok(())
    }

    pub(crate) fn semantic_view(&self) -> StreamView {
        self.combined_view()
    }

    pub(crate) fn semantic_view_from(&self, offset: StreamOffset) -> StreamView {
        self.combined_view().suffix_from(offset)
    }

    pub(crate) fn semantic_slice(
        &self,
        range: StreamRange,
    ) -> Result<StreamView, StreamSliceError> {
        let frontier = self.resident.end();
        let mut nodes = Vec::new();

        if range.start() < frontier {
            let end = range.end().min(frontier);
            if range.start() < end {
                nodes.extend(
                    semantic_slice_nodes(
                        self.resident.nodes(),
                        StreamRange::new(range.start(), end),
                    )?
                    .nodes,
                );
            }
        }

        if range.end() > frontier {
            let start = range.start().max(frontier);
            if start < range.end() {
                nodes.extend(
                    semantic_slice_nodes(
                        self.current.view.nodes.iter(),
                        StreamRange::new(start, range.end()),
                    )?
                    .nodes,
                );
            }
        }

        Ok(StreamView::new(nodes))
    }

    pub(crate) fn compile(&self, width: u16) -> CompiledStream {
        compile_stream(&self.combined_view(), width, self.current.stable_through)
    }

    pub(crate) fn compile_from(&self, offset: StreamOffset, width: u16) -> CompiledStream {
        compile_stream(
            &self.semantic_view_from(offset),
            width,
            self.current.stable_through,
        )
    }

    pub(crate) fn release_resident_through(&mut self, offset: StreamOffset) -> StreamOffset {
        self.resident.release_through(offset)
    }

    fn combined_view(&self) -> StreamView {
        let mut nodes = self.resident.view().nodes;
        nodes.extend(self.current.view.suffix_from(self.resident.end()).nodes);
        StreamView::new(nodes)
    }

    fn validate_transition(
        previous: &StreamSnapshot,
        next: &StreamSnapshot,
    ) -> Result<(), StreamModelError> {
        next.validate().map_err(StreamModelError::Validation)?;
        if next.revision < previous.revision {
            return Err(StreamModelError::RevisionRegressed);
        }
        if next.source_base < previous.source_base {
            return Err(StreamModelError::SourceBaseRegressed);
        }
        if next.source_end < previous.source_end {
            return Err(StreamModelError::SourceEndRegressed);
        }
        if next.stable_through < previous.stable_through {
            return Err(StreamModelError::StabilityRegressed);
        }
        if next.revision == previous.revision
            && (next.source_base != previous.source_base
                || next.source_end != previous.source_end
                || next.stable_through != previous.stable_through
                || next.view != previous.view)
        {
            return Err(StreamModelError::ChangedWithoutRevision);
        }
        Ok(())
    }
}

impl From<StreamValidationError> for StreamModelError {
    fn from(error: StreamValidationError) -> Self {
        Self::Validation(error)
    }
}
