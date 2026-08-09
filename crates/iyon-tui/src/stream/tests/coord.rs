use crate::stream::{StreamOffset, StreamRevision};

#[test]
#[should_panic(expected = "stream revision exhausted")]
fn revision_does_not_wrap() {
    StreamRevision::new(u64::MAX).next();
}

#[test]
fn offset_saturating_add_is_monotonic() {
    assert_eq!(StreamOffset::new(4).saturating_add(3), StreamOffset::new(7));
}
