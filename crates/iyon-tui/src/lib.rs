//! Semantic terminal UI construction.
//!
//! [`View`] is an owned backend-neutral semantic value. Compose views with
//! [`View::horizontal`] and [`View::vertical`], style nodes with semantic
//! properties, and use [`IntoView`] to integrate application components.
//! [`Component`] is retained mounted state referenced by a View. [`History`]
//! is an ordered root-level historical/live/stream flow, while [`Scene`] is
//! the terminal semantic root containing optional History and a required
//! ordinary body View. Layout internals and terminal backend types remain
//! private.
//!
//! ```compile_fail
//! use iyon_tui::presentation::ir::ViewKind;
//! ```
//!
//! ```compile_fail
//! use iyon_tui::{IntoView, View};
//!
//! let view = View::text("x").into_view();
//! let _ = view.kind;
//! ```
//!
//! ```compile_fail
//! use iyon_tui::{Decoration, RowChild, WidthRule};
//! ```
//!
//! ```compile_fail
//! use iyon_tui::View;
//!
//! let _ = View::text("x").container().no_wrap();
//! ```
//!
//! ```compile_fail
//! use iyon_tui::Horizontal;
//!
//! let _ = Horizontal::new();
//! ```

mod application;
mod backend;
mod component;
mod controls;
mod geometry;
mod history;
mod id;
mod interaction;
mod output;
mod physical;
mod presentation;
mod projection;
mod scene;
mod scroll;
mod scroll_command;
mod stream;
mod terminal;
#[cfg(feature = "test-util")]
pub mod testing;
mod theme;

pub use application::{
    App, AppClosed, AppCx, AppHandle, AppSendError, RunError, RuntimeError, TimerHandle,
};

pub use component::{Component, ComponentCx, ComponentHandle};
pub use controls::{TextChange, TextInput};
pub use history::{
    FlowBoundary, History, HistoryError, HistoryLayout, HistoryStreamHandle, HistoryUnitId,
};
pub use interaction::{InteractionResult, Key, KeyStroke, MediaKey, ModifierKey, Modifiers};
pub use output::{EventCx, Output, OutputRouter, RouteConflict};
pub use projection::{
    Projection, ProjectionBuilder, ProjectionRelationError, ProjectionSpan,
    ProjectionTransitionError, ProjectionValidationError, Projector, ProjectorExt, Then, ThenError,
    validate_projection_relation, validate_projection_transition,
};
pub use scene::Scene;
pub use scroll::ScrollPane;
pub use theme::Theme;

pub use presentation::api::{
    AnsiColor, BorderEdges, BorderGlyphError, BorderGlyphs, BorderSpec, BorderStyle, ColorSpec,
    Horizontal, HorizontalAlign, Insets, IntoView, OverflowIndicator, StyleRef, StyleSelector,
    StyleSpec, StyleStateKey, StyleStateValue, Text, TextAttribute, TextAttributeSpec, TextSpan,
    ThemeColor, ThemeKey, Vertical, VerticalAlign, View, WrapMode,
};
pub use stream::{
    ProjectedText, ProjectedTextBuilder, ProjectedValidationError, StreamError, StreamOffset,
    StreamPane, StreamRange, StreamRevision, StreamSnapshot, StreamSnapshotBuilder,
    StreamValidationError, StreamingSource,
};
