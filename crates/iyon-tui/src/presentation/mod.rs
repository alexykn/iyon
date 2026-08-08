//! Presentation boundaries for semantic feature views and renderer internals.
//!
//! The `api` module is deliberately independent of Ratatui and Iyon domain
//! concepts. `internal` is the adapter that lowers those descriptions into the
//! existing transcript row and terminal machinery.

pub(crate) mod api;
pub(super) mod internal;
pub(crate) mod stream;
pub(crate) mod wrap;

pub(crate) use api::{
    ActiveContent, ColorSpec, Decoration, DockPanel, DockSizePolicy, FlowBoundary, Insets,
    InteractionResult, RowChild, StyleSpec, TextAttributes, TextSpan, ThemeKey, UiKey, View,
    WidthRule, WrapMode,
};
pub(crate) use stream::{
    ExactTerminator, HostedStream, LeadingBoundaryState, PreparedStreamFrame, ProjectedText,
    ProjectedTextLayout, ProjectedTextRun, ResidentStreamHandoff, StreamNode, StreamOffset,
    StreamRange, StreamRevision, StreamSnapshot, StreamView, StreamingContent,
};
