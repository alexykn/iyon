use std::time::{Duration, Instant};

/// Opaque identity for one application-owned one-shot timer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TimerHandle {
    id: u64,
}

#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "S9 timer entries are consumed by the application kernel"
    )
)]
struct TimerEntry<Action> {
    handle: TimerHandle,
    deadline: Instant,
    sequence: u64,
    action: Action,
}

pub(crate) struct TimerQueue<Action> {
    entries: Vec<TimerEntry<Action>>,
    next_id: u64,
    next_sequence: u64,
}

impl<Action> Default for TimerQueue<Action> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            next_id: 0,
            next_sequence: 0,
        }
    }
}

#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "S9 timer queue operations are consumed by the application kernel"
    )
)]
impl<Action> TimerQueue<Action> {
    pub(crate) fn schedule(
        &mut self,
        now: Instant,
        delay: Duration,
        action: Action,
    ) -> TimerHandle {
        let handle = TimerHandle { id: self.next_id };
        self.next_id = self
            .next_id
            .checked_add(1)
            .expect("application timer identity exhausted");
        let sequence = self.next_sequence;
        self.next_sequence = self
            .next_sequence
            .checked_add(1)
            .expect("application timer sequence exhausted");
        let deadline = now
            .checked_add(delay)
            .expect("application timer deadline exhausted");
        self.entries.push(TimerEntry {
            handle,
            deadline,
            sequence,
            action,
        });
        handle
    }

    pub(crate) fn cancel(&mut self, handle: TimerHandle) -> bool {
        let Some(index) = self.entries.iter().position(|entry| entry.handle == handle) else {
            return false;
        };
        self.entries.swap_remove(index);
        true
    }

    pub(crate) fn pop_due(&mut self, now: Instant) -> Option<Action> {
        let index = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.deadline <= now)
            .min_by_key(|(_, entry)| (entry.deadline, entry.sequence))
            .map(|(index, _)| index)?;
        Some(self.entries.swap_remove(index).action)
    }

    pub(crate) fn next_deadline(&self) -> Option<Instant> {
        self.entries.iter().map(|entry| entry.deadline).min()
    }

    pub(crate) fn clear(&mut self) {
        self.entries.clear();
    }
}
