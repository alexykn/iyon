use std::{fmt, sync::Arc};

use crate::StreamRange;

use super::TextIrError;

/// How an inline text run relates to root source bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextProvenance {
    /// Display bytes are exactly the bytes in the source range.
    Exact(StreamRange),
    /// Display text derives from the source range but was transformed.
    Derived(StreamRange),
    /// Display text was introduced without source bytes.
    Synthetic,
}

/// Immutable text with provenance and optional semantic annotations.
#[derive(Clone, PartialEq, Eq)]
pub struct TextRun {
    text: Arc<str>,
    provenance: TextProvenance,
    annotations: super::Annotations,
}

impl fmt::Debug for TextRun {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TextRun")
            .field("text", &self.text)
            .field("provenance", &self.provenance)
            .field("annotations", &self.annotations)
            .finish()
    }
}

impl TextRun {
    pub fn exact(text: impl Into<Arc<str>>, range: StreamRange) -> Result<Self, TextIrError> {
        let text = text.into();
        if !text.is_char_boundary(0) || !text.is_char_boundary(text.len()) {
            return Err(TextIrError::NotCharBoundary);
        }
        if text.len() as u64 != range.len() {
            return Err(TextIrError::InvalidExactLength {
                text_len: text.len() as u64,
                range_len: range.len(),
            });
        }
        Ok(Self {
            text,
            provenance: TextProvenance::Exact(range),
            annotations: super::Annotations::default(),
        })
    }

    pub fn derived(text: impl Into<Arc<str>>, range: StreamRange) -> Self {
        Self {
            text: text.into(),
            provenance: TextProvenance::Derived(range),
            annotations: super::Annotations::default(),
        }
    }

    pub fn synthetic(text: impl Into<Arc<str>>) -> Self {
        Self {
            text: text.into(),
            provenance: TextProvenance::Synthetic,
            annotations: super::Annotations::default(),
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn provenance(&self) -> &TextProvenance {
        &self.provenance
    }

    pub fn annotations(&self) -> &super::Annotations {
        &self.annotations
    }
    pub fn map_annotations(
        self,
        map: impl FnOnce(super::Annotations) -> super::Annotations,
    ) -> Self {
        let annotations = map(self.annotations.clone());
        self.with_annotations(annotations)
    }

    pub fn with_annotations(mut self, annotations: super::Annotations) -> Self {
        self.annotations = annotations;
        self
    }

    /// Splits a run at a UTF-8 byte boundary.
    pub fn split_at(&self, byte_offset: usize) -> Result<(Self, Self), TextIrError> {
        if !self.text.is_char_boundary(byte_offset) {
            return Err(TextIrError::NotCharBoundary);
        }
        let (left, right) = self.text.split_at(byte_offset);
        let provenance = |start: usize, end: usize| match self.provenance {
            TextProvenance::Exact(range) => TextProvenance::Exact(StreamRange::new(
                range.start().saturating_add(start as u64),
                range.start().saturating_add(end as u64),
            )),
            TextProvenance::Derived(range) => TextProvenance::Derived(range),
            TextProvenance::Synthetic => TextProvenance::Synthetic,
        };
        Ok((
            Self {
                text: Arc::from(left),
                provenance: provenance(0, byte_offset),
                annotations: self.annotations.clone(),
            },
            Self {
                text: Arc::from(right),
                provenance: provenance(byte_offset, self.text.len()),
                annotations: self.annotations.clone(),
            },
        ))
    }
}

/// Text intentionally exposed to a nested-language projector.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct LiteralText {
    runs: Arc<[TextRun]>,
}

impl LiteralText {
    pub fn new(runs: impl IntoIterator<Item = TextRun>) -> Self {
        Self {
            runs: runs.into_iter().collect(),
        }
    }

    pub fn from_exact(text: impl Into<Arc<str>>, range: StreamRange) -> Result<Self, TextIrError> {
        Ok(Self::new([TextRun::exact(text, range)?]))
    }

    pub fn runs(&self) -> &[TextRun] {
        &self.runs
    }

    pub fn is_empty(&self) -> bool {
        self.runs.is_empty() || self.runs.iter().all(|run| run.text().is_empty())
    }

    pub fn text(&self) -> String {
        self.runs.iter().map(TextRun::text).collect()
    }
}
