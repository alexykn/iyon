//! Framework-owned low-priority paint defaults.
//!
//! This layer is resolved before the application Theme. It may provide
//! portable framework semantic defaults, but must not contain product-specific
//! application vocabulary or layout/geometry policy.
//!
//! The layer is intentionally empty until typed generic text semantics are
//! introduced.

use super::Theme;

pub(crate) fn framework_theme() -> Theme {
    Theme::new()
}
