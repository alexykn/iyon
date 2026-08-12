//! Backend-neutral text measurement used by the layout pass.

use crate::{
    geometry::Size,
    presentation::{ir::TextView, wrap::text_flow_metrics},
};

pub(crate) fn text_intrinsic_size(text: &TextView, width: u16) -> Size {
    let flow = text_flow_metrics(text, width);
    Size::new(flow.width, flow.row_count)
}

pub(crate) fn text_fits(text: &TextView, width: u16) -> bool {
    text_flow_metrics(text, width).fits
}
