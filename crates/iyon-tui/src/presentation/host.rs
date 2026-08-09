//! Current host-facing presentation lifecycle and interaction interfaces.
//!
//! These interfaces sit outside the retained View IR and semantic View
//! construction facade.

use std::{fmt::Debug, time::Instant};

use super::ir::View;
use crate::component::Component;

/// FEATURE EXTENSION API. Generic attachment relationship decided by a feature.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum FlowBoundary {
    #[default]
    Default,
    AttachToPrevious,
}

/// FEATURE EXTENSION API. Backend-neutral key semantics for interaction surfaces.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UiKey {
    Char(char),
    Enter,
    Escape,
    Backspace,
    Tab,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    Unknown,
}

/// FEATURE EXTENSION API. Result returned by a dock or modal interaction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum InteractionResult {
    Ignored,
    Consumed,
    Action(HostAction),
}

/// FEATURE EXTENSION API. Host-owned action emitted by an interaction surface.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HostAction(pub(crate) String);

/// FEATURE EXTENSION API. A stateful surface outside durable conversation history.
/// Its view is semantic; physical size and clipping belong to the host.
pub(crate) trait DockPanel: Component {
    fn size_policy(&self) -> DockSizePolicy;
    fn handle_key(&mut self, key: UiKey) -> InteractionResult;
    fn focus(&mut self) {}
    fn blur(&mut self) {}
}

/// FEATURE EXTENSION API. Generic dock sizing policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DockSizePolicy {
    HiddenWhenEmpty,
    Content { max_rows: Option<u16> },
    Fixed(u16),
}

/// FEATURE EXTENSION API. A focused interaction surface with priority input.
pub(crate) trait Modal: Component {
    fn handle_key(&mut self, key: UiKey) -> InteractionResult;
}

/// FEATURE EXTENSION API. Mutable live-region content. It does not expose
/// wrapping, terminal coordinates, spill, or commit operations.
pub(crate) trait ActiveContent: Component {
    fn boundary(&self) -> FlowBoundary {
        FlowBoundary::Default
    }

    fn tick(&mut self, _now: Instant) -> bool {
        false
    }

    fn finish(self: Box<Self>) -> Vec<Box<dyn TranscriptBlock>> {
        Vec::new()
    }
}

/// FEATURE EXTENSION API. Durable Iyon content implemented above the generic
/// presentation boundary.
pub(crate) trait TranscriptBlock: Debug {
    fn view(&self) -> View;
}
