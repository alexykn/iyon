//! Manual horizontal track allocation.

use crate::{
    physical::PhysicalStyle,
    presentation::ir::{RowView, TrackSize},
};

use super::ViewCompiler;

#[derive(Debug, Clone)]
pub(crate) struct RowAllocation {
    pub(crate) tracks: Vec<u16>,
    pub(crate) gap: u16,
}

pub(crate) fn allocate_tracks(
    row: &RowView,
    width: u16,
    compiler: &ViewCompiler,
    inherited: PhysicalStyle,
) -> RowAllocation {
    let count = row.children.len();
    if count == 0 {
        return RowAllocation {
            tracks: Vec::new(),
            gap: 0,
        };
    }
    let gap_count = count.saturating_sub(1);
    let gap = if gap_count == 0 {
        0
    } else {
        row.gap.min(width / gap_count as u16)
    };
    let available = usize::from(width).saturating_sub(usize::from(gap) * gap_count);
    let mut tracks = vec![0u16; count];
    let mut used = 0usize;
    let mut flex = None;

    for (index, child) in row.children.iter().enumerate() {
        match child.track {
            TrackSize::Fixed(requested) => {
                let allocation = usize::from(requested).min(available.saturating_sub(used));
                tracks[index] = allocation as u16;
                used += allocation;
            }
            TrackSize::Content { max } => {
                let remaining = available.saturating_sub(used).min(usize::from(u16::MAX)) as u16;
                let preferred = compiler.layout(&child.view, remaining, inherited).width as usize;
                let allocation = preferred
                    .min(max.map_or(usize::MAX, usize::from))
                    .min(available.saturating_sub(used));
                tracks[index] = allocation as u16;
                used += allocation;
            }
            TrackSize::Flex { min } => {
                debug_assert!(flex.is_none(), "a row may contain at most one flex track");
                flex = Some((index, usize::from(min)));
            }
        }
    }

    if let Some((index, minimum)) = flex {
        let remaining = available.saturating_sub(used);
        tracks[index] = remaining.min(usize::from(u16::MAX)) as u16;
        let _minimum_is_satisfied = remaining >= minimum;
    }

    RowAllocation { tracks, gap }
}
