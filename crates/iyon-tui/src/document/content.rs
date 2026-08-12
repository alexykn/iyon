use std::{fmt, sync::Arc};

use super::{Block, TextIrError};

/// Exact, unclaimed text at the root of a text projection.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct RawText(Arc<str>);

impl RawText {
    pub fn new(text: impl Into<Arc<str>>) -> Self {
        Self(text.into())
    }
    pub fn text(&self) -> &str {
        &self.0
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
}

/// The closed set of generic text projection values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextContent {
    Raw(RawText),
    Block(Block),
}

impl TextContent {
    pub fn raw(text: impl Into<Arc<str>>) -> Self {
        Self::Raw(RawText::new(text))
    }
    pub fn block(block: Block) -> Self {
        Self::Block(block)
    }
}

impl From<RawText> for TextContent {
    fn from(value: RawText) -> Self {
        Self::Raw(value)
    }
}

impl From<Block> for TextContent {
    fn from(value: Block) -> Self {
        Self::Block(value)
    }
}

impl fmt::Display for RawText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<TextIrError> for std::io::Error {
    fn from(error: TextIrError) -> Self {
        std::io::Error::other(error)
    }
}
