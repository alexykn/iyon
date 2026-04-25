use std::time::{Duration, Instant};

const TICK_INTERVAL: Duration = Duration::from_millis(16);
const SPRING_MULTIPLIER: f32 = 2.0;
const MIN_CHARS_PER_SECOND: f32 = 20.0;
const MAX_CHARS_PER_SECOND: f32 = 800.0;

#[derive(Debug, Default)]
pub(crate) struct StreamSmoother {
    pending: String,
    pending_chars: usize,
    carry_chars: f32,
    last_drain: Option<Instant>,
}

impl StreamSmoother {
    pub(crate) fn push(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }

        self.pending_chars = self.pending_chars.saturating_add(text.chars().count());
        self.pending.push_str(text);
    }

    pub(crate) fn drain_ready(&mut self, now: Instant) -> Option<String> {
        if !self.has_pending() {
            self.last_drain = None;
            self.carry_chars = 0.0;
            return None;
        }

        let first_drain = self.last_drain.is_none();
        let last_drain = self.last_drain.unwrap_or(now - TICK_INTERVAL);
        self.last_drain = Some(now);

        let elapsed = now.saturating_duration_since(last_drain);
        let chars_per_second = self.adaptive_chars_per_second();
        let budget = elapsed.as_secs_f32() * chars_per_second + self.carry_chars;
        let mut chars_to_emit = budget.floor() as usize;
        self.carry_chars = budget.fract();

        // Make the first pending chunk visible immediately. Without this, a small chunk at
        // the minimum speed can wait for a second event-loop wakeup before showing text.
        if first_drain && chars_to_emit == 0 {
            chars_to_emit = 1;
            self.carry_chars = 0.0;
        }

        if chars_to_emit == 0 {
            return None;
        }

        Some(self.drain_prefix_chars(chars_to_emit.min(self.pending_chars)))
    }

    pub(crate) fn flush(&mut self) -> Option<String> {
        if !self.has_pending() {
            return None;
        }

        self.pending_chars = 0;
        self.carry_chars = 0.0;
        self.last_drain = None;
        Some(std::mem::take(&mut self.pending))
    }

    pub(crate) fn has_pending(&self) -> bool {
        self.pending_chars > 0
    }

    pub(crate) fn next_tick_interval(&self) -> Option<Duration> {
        self.has_pending().then_some(TICK_INTERVAL)
    }

    fn adaptive_chars_per_second(&self) -> f32 {
        // Jitter smoothing wants to avoid emptying the buffer too aggressively. Tie speed
        // to backlog size like a spring: large pending chunks render quickly, then the
        // stream naturally decelerates as pending text runs low. This stretches text over
        // backend gaps instead of producing a burst followed by a hard stop.
        (self.pending_chars as f32 * SPRING_MULTIPLIER)
            .clamp(MIN_CHARS_PER_SECOND, MAX_CHARS_PER_SECOND)
    }

    fn drain_prefix_chars(&mut self, chars: usize) -> String {
        let split_at = byte_index_after_chars(&self.pending, chars);
        self.pending_chars = self.pending_chars.saturating_sub(chars);
        self.pending.drain(..split_at).collect()
    }
}

fn byte_index_after_chars(text: &str, chars: usize) -> usize {
    if chars == 0 {
        return 0;
    }

    text.char_indices()
        .nth(chars)
        .map_or(text.len(), |(idx, _)| idx)
}
