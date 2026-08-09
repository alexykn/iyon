//! Width-dependent stream compilation.

mod rows;
mod text;

use crate::physical::PhysicalRow;

use super::{
    coord::StreamOffset,
    node::{StreamNode, StreamView},
};

pub(crate) use rows::{CompiledStream, StreamAtomicId, StreamRowTransfer};

/// Compiles a [`StreamView`] into physical lines with transfer metadata at the specified width.
///
/// Uses the unified presentation text compiler to guarantee that styles, Unicode
/// graphemes, and wrapping agreement remain identical between stream and view paths.
pub(crate) fn compile_stream(
    view: &StreamView,
    max_width: u16,
    stable_through: StreamOffset,
) -> CompiledStream {
    use crate::presentation::layout::ViewCompiler;

    let width = max_width.max(1);
    let compiler = ViewCompiler::default();
    let mut rows = Vec::new();
    let mut transfer = Vec::new();
    let mut transferable_prefix_rows = 0;
    let mut blocked = false;
    let mut next_atomic_id = 0usize;

    for node in &view.nodes {
        match node {
            StreamNode::Text(text) => {
                let (_w, compiled_rows) =
                    compiler.compile_projected_text_with_metadata(text, width);
                let final_offset = text.owned_range().end;
                let row_count = compiled_rows.len();
                if row_count == 0 {
                    rows.push(PhysicalRow::empty());
                    if !blocked && final_offset <= stable_through {
                        transfer.push(StreamRowTransfer::Checkpoint(final_offset));
                        transferable_prefix_rows += 1;
                    } else {
                        blocked = true;
                        transfer.push(StreamRowTransfer::Blocked);
                    }
                } else {
                    for (r_idx, row) in compiled_rows.into_iter().enumerate() {
                        let is_last = r_idx + 1 == row_count;
                        let offset = if is_last {
                            final_offset
                        } else {
                            row.source_end.map_or(text.content_range.start, |relative| {
                                text.content_range.start.saturating_add(relative as u64)
                            })
                        };
                        rows.push(row.row);
                        if !blocked && row.fits && offset <= stable_through {
                            transfer.push(StreamRowTransfer::Checkpoint(offset));
                            transferable_prefix_rows += 1;
                        } else {
                            blocked = true;
                            transfer.push(StreamRowTransfer::Blocked);
                        }
                    }
                }
            }
            StreamNode::Atomic { range, view } => {
                next_atomic_id += 1;
                let group = StreamAtomicId(next_atomic_id);
                let layout = compiler.compile(view, width);
                let all_safe =
                    !blocked && range.end <= stable_through && layout.physically_complete;

                for row in layout.rows {
                    rows.push(row);
                    if all_safe {
                        transfer.push(StreamRowTransfer::Atomic {
                            group,
                            source_end: range.end,
                        });
                        transferable_prefix_rows += 1;
                    } else {
                        blocked = true;
                        transfer.push(StreamRowTransfer::Blocked);
                    }
                }
            }
        }
    }

    CompiledStream {
        rows,
        transfer,
        transferable_prefix_rows,
    }
}
