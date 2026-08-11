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
            TrackSize::Flex { min } => flex.push((index, usize::from(min), None)),
            TrackSize::FlexMax { min, max } => {
                flex.push((index, usize::from(min), Some(usize::from(max))))
            }
        }
    }

    let mut remaining = capacity.saturating_sub(used);
    for (index, minimum, maximum) in flex.iter().copied() {
        let amount = minimum.min(maximum.unwrap_or(usize::MAX)).min(remaining);
        allocation[index] = amount as u16;
        remaining -= amount;
    }

    // Distribute the remaining capacity in rounds. A capped track leaves the
    // active set as soon as it saturates, so its unused share is redistributed
    // to the tracks that can still grow.
    let mut active = flex
        .iter()
        .filter(|(index, _, maximum)| {
            maximum.is_none_or(|maximum| usize::from(allocation[*index]) < maximum)
        })
        .map(|(index, _, _)| *index)
        .collect::<Vec<_>>();
    while remaining > 0 && !active.is_empty() {
        let each = remaining / active.len();
        let remainder = remaining % active.len();
        let mut granted_total = 0;
        let mut next_active = Vec::with_capacity(active.len());
        for (order, index) in active.into_iter().enumerate() {
            let requested = each + usize::from(order < remainder);
            let maximum = flex
                .iter()
                .find(|(candidate, _, _)| *candidate == index)
                .and_then(|(_, _, maximum)| *maximum);
            let available = maximum.map_or(requested, |maximum| {
                maximum.saturating_sub(usize::from(allocation[index]))
            });
            let granted = requested.min(available).min(remaining);
            allocation[index] = allocation[index].saturating_add(granted as u16);
            granted_total += granted;
            if maximum.is_none_or(|maximum| usize::from(allocation[index]) < maximum) {
                next_active.push(index);
            }
        }
        if granted_total == 0 {
            break;
        }
        remaining -= granted_total;
        active = next_active;
    }

    TrackAllocation {
        tracks: allocation,
        gap,
    }
}
