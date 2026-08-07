//! Linear presenter: append-only native history, one bounded semantic-suffix
//! tail, and a Ratatui-only bottom viewport, all presented through a single
//! synchronized transaction.
//!
//! Architecture:
//!
//! ```text
//! canonical state (immutable finalized TranscriptState + RuntimeState::active_turn)
//!         │
//!         ▼
//! semantic-block projection (width-independent source cursor + stability)
//!         │
//! ┌───────┴───────────────────┬──────────────────┐
//! │ immutable history         │ live tail        │
//! │ (append-only, insert_before│ (open blocks,    │
//! │  into native scrollback)  │  top of frame)   │
//! └───────────────────────────┴──────────────────┘
//!         │
//!         ▼
//! InlinePresenter::present(batch) — ONE BeginSynchronizedUpdate..End sync
//!   transaction; cursors advance only after it succeeds.
//! ```
//!
//! The commit cursor is a **semantic source boundary**, not a physical-row
//! count, so resize never duplicates or skips source text.

pub(crate) mod buffer;
pub(crate) mod presenter;
pub(crate) mod projection;

pub(crate) use presenter::{InlinePresenter, PresentBatch};
pub(crate) use projection::{project_active_turn, split_projection, ProjectionMode};
