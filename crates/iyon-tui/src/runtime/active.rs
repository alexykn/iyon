use std::time::{Duration, Instant};

use crate::presentation::{ActiveContent, IntoView, View};

const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

#[derive(Debug, Clone)]
/// Transient content attached to the active conversation turn. Assistant
/// semantic content is owned by `HostedStream<AssistantStream>`, not by this
/// state.
pub(crate) enum ActiveTurnContent {
    WorkingSpinner {
        spinner_frame: usize,
    },
    Tool {
        tool_name: String,
        status: ToolActiveStatus,
        detail: Option<String>,
    },
}

impl ActiveTurnContent {
    pub(crate) fn working_spinner() -> Self {
        Self::WorkingSpinner { spinner_frame: 0 }
    }

    pub(crate) fn is_working_spinner(&self) -> bool {
        matches!(self, Self::WorkingSpinner { .. })
    }

    pub(crate) fn tick(&mut self) {
        if let Self::WorkingSpinner { spinner_frame } = self {
            *spinner_frame = spinner_frame.wrapping_add(1);
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum ToolActiveStatus {
    WaitingForApproval { approval_id: u64 },
    Running,
}

impl ActiveContent for ActiveTurnContent {
    fn view(&self) -> View {
        match self {
            Self::WorkingSpinner { spinner_frame } => View::text(format!(
                "{} Working",
                SPINNER_FRAMES[*spinner_frame % SPINNER_FRAMES.len()]
            ))
            .into_view(),
            Self::Tool {
                tool_name,
                status,
                detail,
            } => {
                let status = match status {
                    ToolActiveStatus::WaitingForApproval { .. } => "approval required",
                    ToolActiveStatus::Running => "running",
                };
                let mut children =
                    vec![View::text(format!("tool {tool_name}: {status}")).into_view()];
                if let Some(detail) = detail {
                    children.push(View::text(detail).into_view());
                }
                View::column(children, 0)
            }
        }
    }
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
        active: Option<&ActiveTurnContent>,
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
        active: Option<&mut ActiveTurnContent>,
    ) -> bool {
        let Some(active) = active else {
            self.animating = false;
            self.next_tick = now + self.interval;
            return false;
        };

        if !active.is_working_spinner() {
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

fn is_spinner_animating(active: Option<&ActiveTurnContent>) -> bool {
    active.is_some_and(ActiveTurnContent::is_working_spinner)
}
