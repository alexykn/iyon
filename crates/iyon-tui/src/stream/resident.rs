//! Immutable semantic resident-prefix ownership.

use std::collections::VecDeque;

use super::{StreamNode, StreamOffset, StreamRange, StreamView};

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct ResidentPrefix {
    base: StreamOffset,
    end: StreamOffset,
    nodes: VecDeque<StreamNode>,
}

impl ResidentPrefix {
    pub(crate) const fn new(base: StreamOffset) -> Self {
        Self {
            base,
            end: base,
            nodes: VecDeque::new(),
        }
    }

    pub(crate) const fn base(&self) -> StreamOffset {
        self.base
    }

    pub(crate) const fn end(&self) -> StreamOffset {
        self.end
    }

    pub(crate) fn nodes(&self) -> impl Iterator<Item = &StreamNode> {
        self.nodes.iter()
    }

    pub(crate) fn view(&self) -> StreamView {
        StreamView::new(self.nodes.iter().cloned().collect())
    }

    pub(crate) fn push(&mut self, node: StreamNode) {
        let range = node.owned_range();
        assert_eq!(
            range.start, self.end,
            "resident semantic nodes must be contiguous"
        );
        if self.nodes.is_empty() {
            self.base = range.start;
        }
        self.end = range.end;
        self.nodes.push_back(node);
    }

    pub(crate) fn release_through(&mut self, offset: StreamOffset) -> StreamOffset {
        while self
            .nodes
            .front()
            .is_some_and(|node| node.owned_range().end <= offset)
        {
            self.nodes.pop_front();
        }
        self.base = self
            .nodes
            .front()
            .map_or(self.end, |node| node.owned_range().start);
        self.base
    }

    pub(crate) fn contains_range(&self, range: StreamRange) -> bool {
        range.start >= self.base && range.end <= self.end
    }
}
