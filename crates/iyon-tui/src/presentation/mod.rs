//! Private presentation boundaries for semantic construction and lowering.
//!
//! `api` is the curated semantic construction facade. `ir` is the private
//! retained semantic tree. `layout` owns manual semantic compilation into
//! physical rows, `paint` resolves styles and decoration, `wrap` owns Unicode
//! wrapping, and `stream` owns the current source-provenance stream model.

pub(crate) mod api;
pub(crate) mod host;
pub(crate) mod ir;
pub(crate) mod layout;
pub(crate) mod paint;
pub(crate) mod stream;
pub(crate) mod wrap;

#[allow(unused_imports)]
pub(crate) use api::{
    BorderSpec, BorderStyle, ColorSpec, Horizontal, HorizontalAlign, Insets, IntoView,
    OverflowIndicator, StyleSpec, Text, TextAttribute, TextAttributeSpec, TextSpan, ThemeKey,
    Vertical, VerticalAlign, View, WrapMode,
};

pub(crate) use host::{
    ActiveContent, DockPanel, DockSizePolicy, FlowBoundary, InteractionResult, UiKey,
};

// Migration-only crate-internal structural vocabulary. Feature code will stop
// depending on these once the semantic construction API migration is complete.
pub(crate) use ir::{Decoration, RowChild, WidthRule};
pub(crate) use stream::{
    ExactTerminator, HostedStream, LeadingBoundaryState, PreparedStreamFrame, ProjectedText,
    ProjectedTextLayout, ProjectedTextRun, ResidentStreamHandoff, StreamNode, StreamOffset,
    StreamRange, StreamRevision, StreamSnapshot, StreamView, StreamingContent,
};
