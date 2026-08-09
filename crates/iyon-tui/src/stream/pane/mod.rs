//! Generic local semantic stream viewport.

mod anchor;
mod command;
mod index;
mod presentation;

#[cfg(test)]
mod tests;

use crate::{Component, ComponentCx, View, geometry::Size};

use super::{StreamError, StreamModel, StreamRowAnchor, StreamingSource, compile_stream};
use anchor::{StreamPaneMode, StreamViewportAnchor, anchor_matches, anchor_to_viewport};
use command::StreamPaneCommand;
use index::{StreamPaneRowIndex, nearest_anchor, top_index};

/// A generic mounted local semantic stream viewport.
///
/// The pane retains semantic resident content through its [`StreamModel`]. It
/// never transfers or releases that content to native terminal history.
pub struct StreamPane<S: StreamingSource> {
    model: StreamModel<S>,
    mode: StreamPaneMode,
    layout_size: Option<Size>,
    row_index: Option<StreamPaneRowIndex>,
}

impl<S: StreamingSource> StreamPane<S> {
    pub fn new(source: S) -> Result<Self, StreamError> {
        Ok(Self {
            model: StreamModel::new(source)?,
            mode: StreamPaneMode::FollowEnd,
            layout_size: None,
            row_index: None,
        })
    }

    pub fn update_source<R>(&mut self, update: impl FnOnce(&mut S) -> R) -> Result<R, StreamError> {
        let result = update(self.model.source_mut());
        self.model.refresh()?;
        self.invalidate_and_repair();
        Ok(result)
    }

    pub fn refresh(&mut self) -> Result<(), StreamError> {
        self.model.refresh()?;
        self.invalidate_and_repair();
        Ok(())
    }

    pub fn seal(&mut self) -> Result<(), StreamError> {
        self.model.seal()?;
        self.invalidate_and_repair();
        Ok(())
    }

    pub fn is_sealed(&self) -> bool {
        self.model.source().is_sealed()
    }

    pub fn scroll_up(&mut self, rows: usize) -> bool {
        self.move_by(rows, false)
    }

    pub fn scroll_down(&mut self, rows: usize) -> bool {
        self.move_by(rows, true)
    }

    pub fn page_up(&mut self) -> bool {
        self.scroll_up(self.layout_size.map_or(0, |size| usize::from(size.height)))
    }

    pub fn page_down(&mut self) -> bool {
        self.scroll_down(self.layout_size.map_or(0, |size| usize::from(size.height)))
    }

    pub fn scroll_to_start(&mut self) {
        let Some(anchor) = self
            .current_index()
            .and_then(|index| index.anchors.first().cloned())
        else {
            return;
        };
        self.mode = StreamPaneMode::Detached(anchor_to_viewport(&anchor));
    }

    pub fn follow_end(&mut self) {
        self.mode = StreamPaneMode::FollowEnd;
    }

    pub fn is_following_end(&self) -> bool {
        matches!(self.mode, StreamPaneMode::FollowEnd)
    }

    fn move_by(&mut self, rows: usize, down: bool) -> bool {
        let Some(size) = self.layout_size else {
            return false;
        };
        if size.width == 0 || size.height == 0 || rows == 0 {
            return false;
        }
        let mode = self.mode.clone();
        let (top, target, max_top, target_anchor) = {
            let Some(index) = self.current_index() else {
                return false;
            };
            if index.anchors.is_empty() {
                return false;
            }
            let viewport = usize::from(size.height);
            let max_top = index.anchors.len().saturating_sub(viewport);
            let top = top_index(&mode, index, viewport);
            let target = if down {
                top.saturating_add(rows).min(max_top)
            } else {
                top.saturating_sub(rows)
            };
            (top, target, max_top, index.anchors.get(target).cloned())
        };
        if target == top {
            if down && target == max_top && !self.is_following_end() {
                self.mode = StreamPaneMode::FollowEnd;
                return true;
            }
            return false;
        }
        if down && target == max_top {
            self.mode = StreamPaneMode::FollowEnd;
        } else if let Some(anchor) = target_anchor {
            self.mode = StreamPaneMode::Detached(anchor_to_viewport(&anchor));
        } else {
            return false;
        }
        true
    }

    fn current_index(&mut self) -> Option<&StreamPaneRowIndex> {
        let width = self.layout_size?.width;
        if width == 0 {
            return None;
        }
        let revision = self.model.snapshot().revision;
        let valid = self
            .row_index
            .as_ref()
            .is_some_and(|index| index.revision == revision && index.width == width);
        if !valid {
            let snapshot = self.model.snapshot();
            let compiled =
                compile_stream(&self.model.semantic_view(), width, snapshot.stable_through);
            self.row_index = Some(StreamPaneRowIndex {
                revision,
                width,
                anchors: compiled.rows.into_iter().map(|row| row.anchor).collect(),
            });
        }
        self.row_index.as_ref()
    }

    fn invalidate_and_repair(&mut self) {
        self.row_index = None;
        let anchor = match &self.mode {
            StreamPaneMode::Detached(anchor) => Some(anchor.clone()),
            StreamPaneMode::FollowEnd => None,
        };
        let Some(index) = self.current_index() else {
            return;
        };
        let Some(anchor) = anchor else {
            return;
        };
        let Some(candidate) = (|| {
            let repaired = index
                .anchors
                .iter()
                .find(|candidate| anchor_matches(&anchor, candidate))
                .map_or_else(
                    || nearest_anchor(index, &anchor),
                    |candidate| {
                        index
                            .anchors
                            .iter()
                            .position(|item| item == candidate)
                            .unwrap_or(0)
                    },
                );
            index.anchors.get(repaired).cloned()
        })() else {
            return;
        };
        self.mode = StreamPaneMode::Detached(anchor_to_viewport(&candidate));
    }

    fn on_layout_changed(&mut self, size: Size) {
        if self.layout_size == Some(size) {
            return;
        }
        let width_changed = self
            .layout_size
            .is_none_or(|previous| previous.width != size.width);
        self.layout_size = Some(size);
        if width_changed {
            self.invalidate_and_repair();
        } else if size.width != 0 {
            let _ = self.current_index();
        }
    }
}

impl<S: StreamingSource> Component for StreamPane<S> {
    fn view(&self) -> View {
        self.render_view()
    }

    fn capabilities(&self, cx: &mut ComponentCx<'_, Self>) {
        cx.focusable();
        cx.on_layout_changed(Self::on_layout_changed);
        cx.key_commands(Self::map_command, Self::handle_command);
    }
}
