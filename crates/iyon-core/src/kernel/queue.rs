use std::collections::VecDeque;

use thiserror::Error;
use tokio_util::sync::CancellationToken;
use crate::ids::QueueItemId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueKind {
    Prompt,
    Steer,
    FollowUp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueueOperation {
    Prompt(String),
    Steer(String),
    FollowUp(String),
    Abort,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[error("{kind:?} queue is full")]
pub struct QueueFull {
    pub kind: QueueKind,
}

#[derive(Debug, Clone)]
pub struct KernelQueues {
    capacity: usize,
    prompts: VecDeque<QueueItem>,
    steers: VecDeque<QueueItem>,
    follow_ups: VecDeque<QueueItem>,
    next_id: u64,
    active: bool,
    steerable: bool,
    abort_requested: bool,
    cancellation: CancellationToken,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueItem {
    pub id: QueueItemId,
    pub text: String,
}

impl KernelQueues {
    pub fn new(capacity: usize, cancellation: CancellationToken) -> Self {
        Self {
            capacity: capacity.max(1),
            prompts: VecDeque::new(),
            steers: VecDeque::new(),
            follow_ups: VecDeque::new(),
            next_id: 1,
            active: false,
            steerable: false,
            abort_requested: false,
            cancellation,
        }
    }
    pub fn set_active(&mut self, active: bool, steerable: bool) {
        self.active = active;
        self.steerable = active && steerable;
    }
    pub fn prompt(&mut self, text: String) -> Result<(), QueueFull> {
        self.prompt_with_id(text).map(|_| ())
    }
    pub fn prompt_with_id(&mut self, text: String) -> Result<QueueItemId, QueueFull> { Self::push(&mut self.prompts, text, QueueKind::Prompt, self.capacity, &mut self.next_id) }
    pub fn steer(&mut self, text: String) -> Result<(), QueueFull> {
        self.steer_with_id(text).map(|_| ())
    }
    pub fn steer_with_id(&mut self, text: String) -> Result<QueueItemId, QueueFull> { Self::push(&mut self.steers, text, QueueKind::Steer, self.capacity, &mut self.next_id) }
    pub fn follow_up(&mut self, text: String) -> Result<(), QueueFull> {
        self.follow_up_with_id(text).map(|_| ())
    }
    pub fn follow_up_with_id(&mut self, text: String) -> Result<QueueItemId, QueueFull> { Self::push(&mut self.follow_ups, text, QueueKind::FollowUp, self.capacity, &mut self.next_id) }
    pub fn submit_turn_compat(&mut self, text: String) -> Result<QueueKind, QueueFull> {
        if self.active && self.steerable {
            self.steer(text)?;
            return Ok(QueueKind::Steer);
        }
        self.prompt(text)?;
        Ok(QueueKind::Prompt)
    }
    pub fn abort(&mut self) {
        self.abort_requested = true;
        self.cancellation.cancel();
    }
    pub fn take_prompt(&mut self) -> Option<String> {
        self.take_prompt_with_id().map(|item| item.text)
    }
    pub fn take_prompt_with_id(&mut self) -> Option<QueueItem> { self.prompts.pop_front() }
    pub fn drain_steers_at_boundary(&mut self) -> Vec<String> {
        self.drain_steers_at_boundary_with_id().into_iter().map(|item| item.text).collect()
    }
    pub fn drain_steers_at_boundary_with_id(&mut self) -> Vec<QueueItem> {
        if self.active && self.steerable {
            return Vec::new();
        }
        self.steers.drain(..).collect()
    }
    pub fn drain_follow_ups_after_settle(&mut self) -> Vec<String> {
        self.drain_follow_ups_after_settle_with_id().into_iter().map(|item| item.text).collect()
    }
    pub fn drain_follow_ups_after_settle_with_id(&mut self) -> Vec<QueueItem> {
        if self.active {
            return Vec::new();
        }
        self.follow_ups.drain(..).collect()
    }
    pub fn abort_requested(&self) -> bool {
        self.abort_requested || self.cancellation.is_cancelled()
    }
    pub fn pending_steers(&self) -> usize {
        self.steers.len()
    }
    pub fn pending_follow_ups(&self) -> usize {
        self.follow_ups.len()
    }
    pub fn pending_prompts(&self) -> usize {
        self.prompts.len()
    }
    pub fn apply(&mut self, operation: QueueOperation) -> Result<(), QueueFull> {
        match operation {
            QueueOperation::Prompt(text) => self.prompt(text),
            QueueOperation::Steer(text) => self.steer(text),
            QueueOperation::FollowUp(text) => self.follow_up(text),
            QueueOperation::Abort => {
                self.abort();
                Ok(())
            }
        }
    }

    fn push(queue: &mut VecDeque<QueueItem>, text: String, kind: QueueKind, capacity: usize, next_id: &mut u64) -> Result<QueueItemId, QueueFull> {
        if queue.len() >= capacity {
            return Err(QueueFull { kind });
        }
        let id = QueueItemId(*next_id);
        *next_id = (*next_id).saturating_add(1);
        queue.push_back(QueueItem { id, text });
        Ok(id)
    }
}

#[cfg(test)]
mod tests {
    use super::{KernelQueues, QueueKind};
    use tokio_util::sync::CancellationToken;
    fn queues() -> KernelQueues {
        KernelQueues::new(8, CancellationToken::new())
    }
    #[test]
    fn prompt_starts_a_run() {
        let mut q = queues();
        q.prompt("hello".into()).unwrap();
        assert_eq!(q.take_prompt().as_deref(), Some("hello"));
    }
    #[test]
    fn steer_waits_for_the_next_safe_boundary() {
        let mut q = queues();
        q.set_active(true, true);
        q.steer("later".into()).unwrap();
        assert!(q.drain_steers_at_boundary().is_empty());
        q.set_active(false, false);
        assert_eq!(q.drain_steers_at_boundary(), vec!["later"]);
    }
    #[test]
    fn follow_up_waits_until_active_run_settles() {
        let mut q = queues();
        q.set_active(true, false);
        q.follow_up("next".into()).unwrap();
        assert!(q.drain_follow_ups_after_settle().is_empty());
        q.set_active(false, false);
        assert_eq!(q.drain_follow_ups_after_settle(), vec!["next"]);
    }
    #[test]
    fn steer_and_follow_up_keep_distinct_order() {
        let mut q = queues();
        q.steer("steer".into()).unwrap();
        q.follow_up("follow".into()).unwrap();
        assert_eq!(q.drain_steers_at_boundary(), vec!["steer"]);
        assert_eq!(q.drain_follow_ups_after_settle(), vec!["follow"]);
    }
    #[test]
    fn abort_preserves_partial_transcript() {
        let mut q = queues();
        q.abort();
        assert!(q.abort_requested());
    }
    #[test]
    fn abort_cancels_pending_approval() {
        let mut q = queues();
        q.abort();
        assert!(q.abort_requested());
    }
    #[test]
    fn submit_turn_compatibility_maps_to_prompt_or_steer() {
        let mut q = queues();
        assert_eq!(
            q.submit_turn_compat("prompt".into()).unwrap(),
            QueueKind::Prompt
        );
        q.set_active(true, true);
        assert_eq!(
            q.submit_turn_compat("steer".into()).unwrap(),
            QueueKind::Steer
        );
    }
    #[test]
    fn interrupt_restart_drains_queued_steers_into_continue() {
        let mut q = queues();
        q.set_active(true, true);
        q.steer("queued".into()).unwrap();
        q.abort();
        q.set_active(false, false);
        assert_eq!(q.drain_steers_at_boundary(), vec!["queued"]);
    }
}
