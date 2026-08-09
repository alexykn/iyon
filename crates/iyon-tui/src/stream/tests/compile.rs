use crate::stream::{
    StreamNode, StreamOffset, StreamRange, StreamRowTransfer, StreamView, compile_stream,
};

#[test]
fn empty_hard_newline_transfers_as_one_row() {
    let view = StreamView::new(vec![StreamNode::exact_line(
        StreamRange::new(StreamOffset::new(2), StreamOffset::new(2)),
        Vec::new(),
        true,
    )]);
    let compiled = compile_stream(&view, 20, StreamOffset::new(3));
    assert_eq!(compiled.rows.len(), 1);
    assert_eq!(
        compiled.transfer[0],
        StreamRowTransfer::Checkpoint(StreamOffset::new(3))
    );
}
