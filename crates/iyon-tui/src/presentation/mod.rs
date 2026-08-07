//! Presentation boundaries for semantic feature views and renderer internals.
//!
//! The `api` module is deliberately independent of Ratatui and Iyon domain
//! concepts. `internal` is the adapter that lowers those descriptions into the
//! existing transcript row and terminal machinery.

pub(crate) mod api;
pub(super) mod internal;

pub(crate) use api::{
    ActiveContent, BoxView, ColorSpec, ColumnView, Decoration, DockPanel, DockSizePolicy,
    FlowBoundary, HorizontalAlign, HostAction, Insets, InteractionResult, Modal, OverflowIndicator,
    RowChild, RowView, StyleSpec, TextAttributes, TextSpan, TextView, ThemeKey, TrackSize, UiKey,
    View, ViewKind, WidthRule, WrapMode,
};
