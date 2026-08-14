//! Wide-cell glyph spans over a physical cell slice.
//!
//! A two-cell emoji is stored as `leader + continuation`. Iterating, clipping,
//! and compositing cell-by-cell can keep a leader without its continuation (or
//! overwrite only the continuation). The TTY then paints a truncated cluster
//! while the next row overwrites the overflow — the “text under other text”
//! failure.
//!
//! Every crop/composite path must copy, skip, or clear a whole
//! [`PhysicalGlyph`]. Do not `clone_from_slice` a cell range unless both ends
//! are already known glyph boundaries.

use super::PhysicalCell;
use super::text_metrics::text_cell_width;

/// One indivisible terminal glyph occupying `width` physical cells.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PhysicalGlyph<'a> {
    /// Column of the leader in the source slice.
    pub start: usize,
    /// Leader plus following continuation cells. Always >= 1.
    pub width: usize,
    pub leader: &'a PhysicalCell,
}

/// Walk `cells` as glyph spans.
///
/// An orphan continuation (no preceding leader) is skipped after a debug
/// assertion: it is already an invalid row, and inventing a 1-cell glyph for it
/// would hide the corruption from [`validate_cells`].
pub(crate) fn glyphs(cells: &[PhysicalCell]) -> PhysicalGlyphIter<'_> {
    PhysicalGlyphIter { cells, index: 0 }
}

pub(crate) struct PhysicalGlyphIter<'a> {
    cells: &'a [PhysicalCell],
    index: usize,
}

impl<'a> Iterator for PhysicalGlyphIter<'a> {
    type Item = PhysicalGlyph<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        while self.index < self.cells.len() {
            if self.cells[self.index].continuation {
                debug_assert!(
                    false,
                    "orphan continuation at column {} (PhysicalRow must never store this)",
                    self.index
                );
                self.index += 1;
                continue;
            }
            let start = self.index;
            let mut width = 1;
            while start + width < self.cells.len() && self.cells[start + width].continuation {
                width += 1;
            }
            let leader = &self.cells[start];
            self.index = start + width;
            return Some(PhysicalGlyph {
                start,
                width,
                leader,
            });
        }
        None
    }
}

/// Why a cell slice is not a valid physical row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CellGeometryError {
    OrphanContinuation {
        index: usize,
    },
    TruncatedLeader {
        index: usize,
        expected: usize,
        actual: usize,
    },
    LeaderWidthMismatch {
        index: usize,
        grapheme: String,
        expected: usize,
        actual: usize,
    },
    ContinuationHasGrapheme {
        index: usize,
    },
    ContinuationStyleMismatch {
        index: usize,
    },
    ContinuationUnpainted {
        index: usize,
    },
    ZeroWidthLeader {
        index: usize,
        grapheme: String,
    },
}

/// Every leader's complete span exists; every continuation belongs to exactly
/// one preceding leader; a stored grapheme's cell count matches the canonical
/// metric.
pub(crate) fn validate_cells(cells: &[PhysicalCell]) -> Result<(), CellGeometryError> {
    let mut index = 0;
    while index < cells.len() {
        if cells[index].continuation {
            return Err(CellGeometryError::OrphanContinuation { index });
        }
        let Some(grapheme) = cells[index].grapheme.as_deref() else {
            index += 1;
            continue;
        };
        let expected = text_cell_width(grapheme);
        if expected == 0 {
            return Err(CellGeometryError::ZeroWidthLeader {
                index,
                grapheme: grapheme.to_owned(),
            });
        }
        let remaining = cells.len() - index;
        if remaining < expected {
            return Err(CellGeometryError::TruncatedLeader {
                index,
                expected,
                actual: remaining,
            });
        }
        let mut actual = 1;
        while index + actual < cells.len() && cells[index + actual].continuation {
            actual += 1;
        }
        if actual != expected {
            return Err(CellGeometryError::LeaderWidthMismatch {
                index,
                grapheme: grapheme.to_owned(),
                expected,
                actual,
            });
        }
        let leader_style = cells[index].style;
        for offset in 1..expected {
            let cell = &cells[index + offset];
            if cell.grapheme.is_some() {
                return Err(CellGeometryError::ContinuationHasGrapheme {
                    index: index + offset,
                });
            }
            if cell.style != leader_style {
                return Err(CellGeometryError::ContinuationStyleMismatch {
                    index: index + offset,
                });
            }
            if !cell.painted {
                return Err(CellGeometryError::ContinuationUnpainted {
                    index: index + offset,
                });
            }
        }
        index += expected;
    }
    Ok(())
}

pub(crate) fn copy_glyph(
    src: &[PhysicalCell],
    src_start: usize,
    dest: &mut [PhysicalCell],
    dest_start: usize,
    width: usize,
) {
    dest[dest_start..dest_start + width].clone_from_slice(&src[src_start..src_start + width]);
}

/// Copy source glyphs into `dest` starting at column `left`.
///
/// A glyph that would not fully fit is omitted entirely and later glyphs are
/// not considered — that would leave a hole. Returns whether every **painted**
/// source glyph was copied.
pub(crate) fn place_glyphs(src: &[PhysicalCell], dest: &mut [PhysicalCell], left: usize) -> bool {
    let mut complete = true;
    for glyph in glyphs(src) {
        let dest_start = left.saturating_add(glyph.start);
        let dest_end = dest_start.saturating_add(glyph.width);
        if dest_start >= dest.len() || dest_end > dest.len() {
            if glyph.leader.painted {
                complete = false;
            }
            break;
        }
        copy_glyph(src, glyph.start, dest, dest_start, glyph.width);
    }
    debug_assert!(validate_cells(dest).is_ok());
    complete
}

/// Neutralize the glyph covering column `x`, including scanning left from a
/// continuation to its leader.
///
/// Termwiz's `Line::set_cell` does the same: modifying a cell hidden by a wide
/// character blanks the whole obscuring sequence, and inserting a wide cell
/// blanks occluded successors.
pub(crate) fn clear_glyph_covering(cells: &mut [PhysicalCell], x: usize) {
    if x >= cells.len() {
        return;
    }
    if !cells[x].painted {
        return;
    }
    let mut leader = x;
    while leader > 0 && cells[leader].continuation {
        leader -= 1;
    }
    if cells[leader].continuation {
        cells[x] = PhysicalCell::transparent();
        return;
    }
    let mut end = leader + 1;
    while end < cells.len() && cells[end].continuation {
        end += 1;
    }
    for cell in &mut cells[leader..end] {
        *cell = PhysicalCell::transparent();
    }
}

/// Write a source glyph into `dest` after clearing every destination glyph that
/// intersects the destination span.
pub(crate) fn write_glyph_span(
    dest: &mut [PhysicalCell],
    dest_start: usize,
    src: &[PhysicalCell],
    src_start: usize,
    width: usize,
) {
    for column in dest_start..dest_start + width {
        clear_glyph_covering(dest, column);
    }
    copy_glyph(src, src_start, dest, dest_start, width);
}

/// Byte offset in `plain_text` → physical cell column of that leader.
///
/// `str::find` returns UTF-8 bytes. Indexing `cells[byte]` inspects the wrong
/// cell after any wide glyph (`💛 Age` would report `Age` at byte 5, not
/// column 3).
#[cfg(any(test, feature = "test-util"))]
pub(crate) fn cell_x_of(cells: &[PhysicalCell], needle: &str) -> Option<usize> {
    if needle.is_empty() {
        return None;
    }
    let mut text = String::new();
    let mut spans: Vec<(std::ops::Range<usize>, usize)> = Vec::new();
    for (x, cell) in cells.iter().enumerate() {
        if cell.continuation {
            continue;
        }
        let visible = if cell.painted {
            cell.grapheme.as_deref().unwrap_or(" ")
        } else {
            " "
        };
        let start = text.len();
        text.push_str(visible);
        let end = text.len();
        spans.push((start..end, x));
    }
    let byte = text.find(needle)?;
    spans
        .into_iter()
        .find(|(range, _)| range.start <= byte && byte < range.end)
        .map(|(_, x)| x)
}
