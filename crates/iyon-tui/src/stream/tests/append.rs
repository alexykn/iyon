use crate::stream::StreamOffset;
use crate::stream::append_only_text_stable_frontier;

#[test]
fn open_append_frontier_withholds_trailing_egc() {
    assert_eq!(
        append_only_text_stable_frontier("e\u{301}", StreamOffset::ZERO, false),
        StreamOffset::ZERO
    );
    assert_eq!(
        append_only_text_stable_frontier("e\u{301}", StreamOffset::ZERO, true),
        StreamOffset::new(3)
    );
}
