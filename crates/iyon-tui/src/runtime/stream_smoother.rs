use std::{collections::VecDeque, time::{Duration, Instant}};

use crate::transcript::SegmentKind;

const TICK_INTERVAL: Duration = Duration::from_millis(16);
const SPRING_MULTIPLIER: f32 = 2.0;
const MIN_CHARS_PER_SECOND: f32 = 20.0;
const MAX_CHARS_PER_SECOND: f32 = 800.0;

/// A char-budget pacing buffer for streamed assistant content. It preserves the
/// kind (text vs thinking) of each incoming chunk while releasing a smooth,
/// adaptive number of characters per tick, so interleaved reasoning and answer
/// text are both paced identically to a plain single-kind stream.
#[derive(Debug, Default)]
pub(crate) struct StreamSmoother {
    /// FIFO of (kind, text) chunks awaiting release.
    pending: VecDeque<(SegmentKind, String)>,
    pending_chars: usize,
    carry_chars: f32,
    last_drain: Option<Instant>,
}

impl StreamSmoother {
    pub(crate) fn push(&mut self, kind: SegmentKind, text: &str) {
        if text.is_empty() {
            return;
        }

        self.pending_chars = self.pending_chars.saturating_add(text.chars().count());
        self.pending.push_back((kind, text.to_string()));
    }

    pub(crate) fn drain_ready(&mut self, now: Instant) -> Option<Vec<(SegmentKind, String)>> {
        if !self.has_pending() {
            self.last_drain = None;
            self.carry_chars = 0.0;
            return None;
        }

        let first_drain = self.last_drain.is_none();
        let last_drain = self
            .last_drain
            .unwrap_or(now.checked_sub(TICK_INTERVAL).unwrap());
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

    pub(crate) fn flush(&mut self) -> Option<Vec<(SegmentKind, String)>> {
        if !self.has_pending() {
            return None;
        }

        let pending = std::mem::take(&mut self.pending);
        self.clear();
        Some(pending.into_iter().collect())
    }

    pub(crate) fn clear(&mut self) {
        self.pending.clear();
        self.pending_chars = 0;
        self.carry_chars = 0.0;
        self.last_drain = None;
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

    /// Emits exactly `chars` characters from the front of the queue, preserving each
    /// chunk's kind. Whole leading chunks are consumed; the chunk that crosses the
    /// budget boundary is split (its remainder stays queued).
    fn drain_prefix_chars(&mut self, chars: usize) -> Vec<(SegmentKind, String)> {
        if chars == 0 {
            return Vec::new();
        }

        let mut emitted: Vec<(SegmentKind, String)> = Vec::new();
        let mut remaining = chars;

        while remaining > 0 {
            let Some((kind, text)) = self.pending.pop_front() else {
                break;
            };
            let chunk_chars = text.chars().count();

            if chunk_chars <= remaining {
                emitted.push((kind, text));
                remaining -= chunk_chars;
                self.pending_chars = self.pending_chars.saturating_sub(chunk_chars);
            } else {
                let split_at = byte_index_after_chars(&text, remaining);
                emitted.push((kind, text[..split_at].to_string()));
                let rest = text[split_at..].to_string();
                self.pending.push_front((kind, rest));
                self.pending_chars = self.pending_chars.saturating_sub(remaining);
                remaining = 0;
            }
        }

        emitted
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

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds_of(chunks: &[(SegmentKind, String)]) -> Vec<SegmentKind> {
        chunks.iter().map(|(kind, _)| *kind).collect()
    }

    fn concat(chunks: &[(SegmentKind, String)]) -> String {
        chunks.iter().map(|(_, text)| text.as_str()).collect()
    }

    #[test]
    fn single_kind_flush_matches_input() {
        let mut smoother = StreamSmoother::default();
        smoother.push(SegmentKind::Text, "hello world");

        let out = smoother.flush().expect("pending");
        assert_eq!(concat(&out), "hello world");
        assert_eq!(kinds_of(&out), vec![SegmentKind::Text]);
    }

    fn runs_of(chunks: &[(SegmentKind, String)]) -> Vec<SegmentKind> {
        let mut runs = Vec::new();
        for (kind, _) in chunks {
            if runs.last() != Some(kind) {
                runs.push(*kind);
            }
        }
        runs
    }

    #[test]
    fn kind_sequence_is_preserved_on_flush() {
        let mut smoother = StreamSmoother::default();
        for (kind, text) in [
            (SegmentKind::Thinking, "t1".to_string()),
            (SegmentKind::Text, "a1".to_string()),
            (SegmentKind::Text, "a2".to_string()),
            (SegmentKind::Thinking, "t2".to_string()),
        ] {
            smoother.push(kind, &text);
        }

        let out = smoother.flush().expect("pending");
        assert_eq!(concat(&out), "t1a1a2t2");
        assert_eq!(
            kinds_of(&out),
            vec![SegmentKind::Thinking, SegmentKind::Text, SegmentKind::Text, SegmentKind::Thinking]
        );
    }

    #[test]
    fn drain_then_flush_round_trips_without_loss() {
        let mut smoother = StreamSmoother::default();
        for (kind, text) in [
            (SegmentKind::Thinking, "aaa".to_string()),
            (SegmentKind::Text, "bbb".to_string()),
            (SegmentKind::Thinking, "ccc".to_string()),
        ] {
            smoother.push(kind, &text);
        }

        let drained = smoother
            .drain_ready(std::time::Instant::now())
            .unwrap_or_default();
        let rest = smoother.flush().unwrap_or_default();
        let all = drained.into_iter().chain(rest).collect::<Vec<_>>();

        // End-to-end round trip: no text lost or duplicated, kinds preserved, and
        // every emitted chunk is non-empty (a budget may split a chunk at a kind
        // boundary but never produce empty fragments).
        assert_eq!(concat(&all), "aaabbbccc");
        assert_eq!(runs_of(&all), vec![
            SegmentKind::Thinking,
            SegmentKind::Text,
            SegmentKind::Thinking,
        ]);
        assert!(all.iter().all(|(_, text)| !text.is_empty()));
    }

    #[test]
    fn clear_empties_pending() {
        let mut smoother = StreamSmoother::default();
        smoother.push(SegmentKind::Text, "abc");
        smoother.clear();
        assert!(!smoother.has_pending());
        assert!(smoother.flush().is_none());
    }

    #[test]
    fn byte_index_after_chars_handles_unicode() {
        let text = "héllo";
        // 'é' is two bytes: h=0, é bytes 1..3, l=3, l=4, o=5.
        assert_eq!(byte_index_after_chars(text, 3), 4); // after "hé" + "l"
        assert_eq!(byte_index_after_chars("abc", 2), 2);
    }
}
