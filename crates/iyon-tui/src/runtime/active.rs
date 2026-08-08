use std::time::{Duration, Instant};

use crate::presentation::{ActiveContent, View};

const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

#[derive(Debug, Clone)]
/// Transient active-pane chrome. Assistant semantic content is owned by
/// `HostedStream<AssistantStream>`, not by this state.
pub(crate) enum ActivePaneState {
    WorkingSpinner {
        spinner_frame: usize,
    },
    Tool {
        tool_name: String,
        status: ToolActiveStatus,
        detail: Option<String>,
    },
    SlashMenu,
    FilePicker,
}

impl ActivePaneState {
    pub(crate) fn working_spinner() -> Self {
        Self::WorkingSpinner { spinner_frame: 0 }
    }

    pub(crate) fn kind(&self) -> ActivePaneKind {
        match self {
            Self::WorkingSpinner { .. } => ActivePaneKind::WorkingSpinner,
            Self::Tool { .. } => ActivePaneKind::Tool,
            Self::SlashMenu => ActivePaneKind::SlashMenu,
            Self::FilePicker => ActivePaneKind::FilePicker,
        }
    }

    pub(crate) fn behavior(&self) -> ActiveBehavior {
        match self {
            Self::WorkingSpinner { .. } => ActiveBehavior::SpillToTranscript,
            Self::Tool { .. } | Self::SlashMenu | Self::FilePicker => ActiveBehavior::OccludeOnly,
        }
    }

    pub(crate) fn desired_height(&self) -> u16 {
        match self {
            Self::WorkingSpinner { .. }
            | Self::Tool { .. }
            | Self::SlashMenu
            | Self::FilePicker => 3,
        }
    }

    pub(crate) fn tick(&mut self) {
        if let Self::WorkingSpinner { spinner_frame } = self {
            *spinner_frame = spinner_frame.wrapping_add(1);
        }
    }

    pub(crate) fn spinner_frame(&self) -> &'static str {
        let frame = match self {
            Self::WorkingSpinner { spinner_frame } => *spinner_frame,
            _ => 0,
        };
        SPINNER_FRAMES[frame % SPINNER_FRAMES.len()]
    }
}

#[derive(Debug, Clone)]
pub(crate) enum ToolActiveStatus {
    WaitingForApproval { approval_id: u64 },
    Running,
}

impl ActiveContent for ActivePaneState {
    fn view(&self) -> View {
        match self {
            Self::WorkingSpinner { spinner_frame } => View::column(
                vec![
                    View::spacer(1),
                    View::text(format!(
                        "{} Working",
                        SPINNER_FRAMES[*spinner_frame % SPINNER_FRAMES.len()]
                    )),
                    View::spacer(1),
                ],
                0,
            ),
            Self::Tool {
                tool_name,
                status,
                detail,
            } => {
                let status = match status {
                    ToolActiveStatus::WaitingForApproval { .. } => "approval required",
                    ToolActiveStatus::Running => "running",
                };
                View::column(
                    vec![
                        View::spacer(1),
                        View::text(format!("tool {tool_name}: {status}")),
                        View::text(detail.as_deref().unwrap_or("")),
                    ],
                    0,
                )
            }
            Self::SlashMenu | Self::FilePicker => View::spacer(1),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ActivePaneKind {
    WorkingSpinner,
    Tool,
    SlashMenu,
    FilePicker,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ActiveBehavior {
    SpillToTranscript,
    OccludeOnly,
}

#[derive(Debug)]
pub(crate) struct ActiveTicker {
    interval: Duration,
    next_tick: Instant,
    animating: bool,
}

impl ActiveTicker {
    pub(crate) fn new(interval: Duration) -> Self {
        Self {
            interval,
            next_tick: Instant::now() + interval,
            animating: false,
        }
    }

    pub(crate) fn wait_timeout(
        &mut self,
        now: Instant,
        active: Option<&ActivePaneState>,
        idle_timeout: Duration,
    ) -> Duration {
        if !is_spinner_animating(active) {
            self.animating = false;
            self.next_tick = now + self.interval;
            return idle_timeout;
        }

        if !self.animating {
            self.animating = true;
            self.next_tick = now + self.interval;
        }

        self.next_tick.saturating_duration_since(now)
    }

    pub(crate) fn tick_if_due(
        &mut self,
        now: Instant,
        active: Option<&mut ActivePaneState>,
    ) -> bool {
        let Some(active) = active else {
            self.animating = false;
            self.next_tick = now + self.interval;
            return false;
        };

        if active.kind() != ActivePaneKind::WorkingSpinner {
            self.animating = false;
            self.next_tick = now + self.interval;
            return false;
        }

        if now < self.next_tick {
            return false;
        }

        active.tick();
        self.next_tick = now + self.interval;
        true
    }
}

fn is_spinner_animating(active: Option<&ActivePaneState>) -> bool {
    matches!(
        active.map(ActivePaneState::kind),
        Some(ActivePaneKind::WorkingSpinner)
    )
}
