use std::time::{Duration, Instant};

use super::*;
use crate::History;

#[derive(Clone)]
struct TemporalSource {
    value: u8,
    deadline: Option<Instant>,
    sealed: bool,
}

impl StreamingSource for TemporalSource {
    fn snapshot(&self) -> StreamSnapshot {
        let end = StreamOffset::new(u64::from(self.value));
        StreamSnapshotBuilder::new(
            StreamRevision::new(u64::from(self.value)),
            StreamOffset::ZERO,
            end,
            end,
        )
        .exact_text(
            StreamRange::new(StreamOffset::ZERO, end),
            [TextSpan::plain("x".repeat(usize::from(self.value)))],
        )
        .finish()
        .unwrap()
    }

    fn seal(&mut self) {
        self.sealed = true;
        self.deadline = None;
    }

    fn is_sealed(&self) -> bool {
        self.sealed
    }

    fn next_wakeup(&self) -> Option<Instant> {
        self.deadline
    }

    fn advance(&mut self, now: Instant) -> bool {
        if self.deadline.is_some_and(|deadline| now >= deadline) {
            self.value += 1;
            self.deadline = None;
            return true;
        }
        false
    }
}

#[test]
fn temporal_source_defaults_and_history_refresh_path() {
    let now = Instant::now();
    let source = TemporalSource {
        value: 1,
        deadline: Some(now + Duration::from_millis(10)),
        sealed: false,
    };
    assert_eq!(source.next_wakeup(), Some(now + Duration::from_millis(10)));
    let mut history = History::new();
    let stream = history.push_stream(source).unwrap();
    assert_eq!(
        history.next_stream_wakeup(),
        Some(now + Duration::from_millis(10))
    );
    assert!(!history.advance_streams(now).unwrap());
    assert!(
        history
            .advance_streams(now + Duration::from_millis(10))
            .unwrap()
    );
    assert_eq!(history.next_stream_wakeup(), None);
    history.seal_stream(stream).unwrap();
    assert_eq!(history.next_stream_wakeup(), None);
}
