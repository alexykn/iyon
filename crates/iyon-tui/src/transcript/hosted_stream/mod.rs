//! Transitional conversation/native-history stream host.

#[cfg(test)]
mod tests;

use crate::{
    physical::PhysicalRow,
    presentation::View,
    stream::{
        StreamOffset, StreamRevision, StreamSnapshot, StreamTransferPayload, StreamingSource,
        compile_stream, plan_stream_transfer,
    },
};

use crate::stream::{FrozenPhysicalRows, StreamPartialCommit, StreamTransferPlan};

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
    C: StreamingSource,
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
    C: StreamingSource,
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
            snapshot.validate().is_ok(),
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
                        .chain(suffix.rows.iter().map(|row| row.physical.clone()))
                        .collect(),
                    suffix,
                )
            }
            _ => {
                let live_view = snapshot.view.suffix_from(self.committed_through);
                let compiled = compile_stream(&live_view, width, snapshot.stable_through);
                (
                    compiled
                        .rows
                        .iter()
                        .map(|row| row.physical.clone())
                        .collect(),
                    compiled,
                )
            }
        };
        let mut live_rows: Vec<PhysicalRow> = live_rows;
        if self.leading_boundary == LeadingBoundaryState::Pending {
            live_rows.insert(0, PhysicalRow::empty());
        }

        // `desired_commit_rows` is measured against the complete conversation projection.
        // The leading boundary is one of those rows, but it may never be committed alone.
        let pending_boundary_rows = if self.leading_boundary == LeadingBoundaryState::Pending {
            1
        } else {
            0
        };
        let semantic_desired_rows = desired_commit_rows.saturating_sub(pending_boundary_rows);
        let semantic_plan = plan_stream_transfer(
            &compiled,
            semantic_desired_rows,
            self.partial.as_ref(),
            self.committed_through,
        );
        let semantic_rows = match &semantic_plan.payload {
            StreamTransferPayload::Compiled { start, len } => compiled.rows
                [*start..start.saturating_add(*len)]
                .iter()
                .map(|row| row.physical.clone())
                .collect(),
            StreamTransferPayload::Frozen { rows } => rows.as_slice().to_vec(),
        };
        let commit_leading_boundary =
            self.leading_boundary == LeadingBoundaryState::Pending && !semantic_rows.is_empty();
        let mut history_rows = Vec::with_capacity(
            semantic_rows
                .len()
                .saturating_add(usize::from(commit_leading_boundary)),
        );
        if commit_leading_boundary {
            history_rows.push(PhysicalRow::empty());
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

    pub(crate) fn is_fully_native(&self) -> bool {
        self.committed_through == self.last_source_end
            && self.leading_boundary != LeadingBoundaryState::Pending
    }

    /// Consumes the mutable host and transfers its immutable remaining source
    /// presentation to the transcript host.
    pub(crate) fn into_resident_handoff(self) -> ResidentStreamHandoff {
        assert!(
            self.can_handoff(),
            "stream is not ready for resident handoff"
        );

        let snapshot = self.content.snapshot();
        assert!(snapshot.validate().is_ok(), "invalid final stream snapshot");
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
            view: snapshot
                .view
                .suffix_from(self.committed_through)
                .into_static_view(),
            leading_boundary: self.leading_boundary,
        }
    }
}

/// A prepared native-history transaction and its semantic acknowledgement.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PreparedHistoryWrite {
    pub(crate) rows: FrozenPhysicalRows,
    pub(crate) semantic_plan: StreamTransferPlan,
    pub(crate) commit_leading_boundary: bool,
}

/// A host-prepared live projection and native-history transaction.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PreparedStreamFrame {
    pub(crate) live_rows: Vec<PhysicalRow>,
    pub(crate) history: PreparedHistoryWrite,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ResidentStreamHandoff {
    pub(crate) unit_id: crate::transcript::model::TranscriptUnitId,
    pub(crate) view: View,
    pub(crate) leading_boundary: LeadingBoundaryState,
}
