use crate::{
    physical::PhysicalRow,
    stream::{FrozenPhysicalRows, StreamOffset, StreamPartialTransfer},
};

use super::super::HistoryUnitId;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum SpacingTransferState {
    Semantic,
    Frozen(FrozenPhysicalRows),
    Native,
}

impl Default for SpacingTransferState {
    fn default() -> Self {
        Self::Semantic
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FrozenStaticRemainder {
    pub(crate) unit: HistoryUnitId,
    pub(crate) rows: FrozenPhysicalRows,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct StreamFrontierState {
    pub(crate) unit: HistoryUnitId,
    pub(crate) committed_through: StreamOffset,
    pub(crate) partial: Option<StreamPartialTransfer>,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub(crate) struct NativeFrontier {
    pub(crate) last_native_unit: Option<HistoryUnitId>,
    pub(crate) top_padding: SpacingTransferState,
    pub(crate) leading_gap: Option<SpacingTransferState>,
    pub(crate) frozen_static: Option<FrozenStaticRemainder>,
    pub(crate) stream: Option<StreamFrontierState>,
}

impl NativeFrontier {
    pub(super) fn reset_unit_state(&mut self) {
        self.leading_gap = None;
        self.frozen_static = None;
        self.stream = None;
    }

    pub(super) fn blank_rows(width: u16, count: usize) -> Vec<PhysicalRow> {
        (0..count)
            .map(|_| {
                PhysicalRow::from_cells(vec![
                    crate::physical::PhysicalCell::transparent();
                    usize::from(width)
                ])
            })
            .collect()
    }
}
