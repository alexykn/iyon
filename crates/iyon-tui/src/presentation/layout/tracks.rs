//! Axis-neutral parent-owned track allocation.

use crate::presentation::ir::TrackSize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TrackAllocation {
    pub(crate) tracks: Vec<u16>,
    pub(crate) gap: u16,
}

pub(crate) fn allocate_tracks(
    available: u16,
    requested_gap: u16,
    tracks: &[TrackSize],
    mut measure_content: impl FnMut(usize, u16) -> u16,
) -> TrackAllocation {
    let count = tracks.len();
    if count == 0 {
        return TrackAllocation {
            tracks: Vec::new(),
            gap: 0,
        };
    }

    let gap_count = count.saturating_sub(1);
    let gap = if gap_count == 0 {
        0
    } else {
        requested_gap.min(available / gap_count as u16)
    };
    let capacity = usize::from(available).saturating_sub(usize::from(gap) * gap_count);
    let mut allocation = vec![0u16; count];
    let mut used = 0usize;
    let mut flex = Vec::new();

    for (index, track) in tracks.iter().copied().enumerate() {
        match track {
            TrackSize::Fixed(requested) => {
                let amount = usize::from(requested).min(capacity.saturating_sub(used));
                allocation[index] = amount as u16;
                used += amount;
            }
            TrackSize::Content { max } => {
                let remaining = capacity.saturating_sub(used).min(usize::from(u16::MAX)) as u16;
                let preferred = usize::from(measure_content(index, remaining));
                let amount = preferred
                    .min(max.map_or(usize::MAX, usize::from))
                    .min(capacity.saturating_sub(used));
                allocation[index] = amount as u16;
                used += amount;
            }
            TrackSize::Flex { min } => flex.push((index, usize::from(min))),
        }
    }

    let mut remaining = capacity.saturating_sub(used);
    for (index, minimum) in flex.iter().copied() {
        let amount = minimum.min(remaining);
        allocation[index] = amount as u16;
        remaining -= amount;
    }

    if !flex.is_empty() && remaining > 0 {
        let each = remaining / flex.len();
        let remainder = remaining % flex.len();
        for (order, (index, _)) in flex.iter().enumerate() {
            let extra = each + usize::from(order < remainder);
            allocation[*index] = allocation[*index].saturating_add(extra as u16);
        }
    }

    TrackAllocation {
        tracks: allocation,
        gap,
    }
}
