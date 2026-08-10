//! Private backend-neutral geometry and layout constraints.

mod constraints;
mod rect;
mod size;

pub(crate) use constraints::{AxisConstraint, LayoutConstraints};
pub(crate) use rect::Rect;
pub(crate) use size::Size;
