//! Generic append-only stability helpers.

use unicode_segmentation::UnicodeSegmentation;

use super::coord::StreamOffset;

/// Conservative stability helper for plain append-only text.
///
/// When sealed, the entire text is stable.
/// When open, holds back the trailing extended grapheme cluster so that partial
/// UTF-8 or combining sequences are never committed before completion.
pub fn append_only_text_stable_frontier(
    source: &str,
    base: StreamOffset,
    sealed: bool,
) -> StreamOffset {
    if sealed || source.is_empty() {
        return base.saturating_add(source.len() as u64);
    }

    let mut last_offset = 0;
    for (offset, _grapheme) in source.grapheme_indices(true) {
        last_offset = offset;
    }

    base.saturating_add(last_offset as u64)
}
