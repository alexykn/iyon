use std::sync::Arc;

use super::{Annotations, LiteralText, TextIrError, TextRun};

/// Inline line-break semantics.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BreakKind {
    Soft,
    Hard,
}

/// Format identifier for a literal embedded language.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FormatId(Arc<str>);

impl FormatId {
    pub fn new(value: impl Into<Arc<str>>) -> Result<Self, TextIrError> {
        let value = value.into();
        super::errors::validate_name(&value)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Language identifier used for nested code projectors.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LanguageId(Arc<str>);

impl LanguageId {
    pub fn new(value: impl Into<Arc<str>>) -> Result<Self, TextIrError> {
        let value = value.into();
        super::errors::validate_name(&value)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A resolved link target.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LinkTarget {
    destination: Arc<str>,
    title: Option<Arc<str>>,
}

impl LinkTarget {
    pub fn new(destination: impl Into<Arc<str>>, title: Option<impl Into<Arc<str>>>) -> Self {
        Self {
            destination: destination.into(),
            title: title.map(Into::into),
        }
    }

    pub fn destination(&self) -> &str {
        &self.destination
    }
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }
}

/// A generic inline formatting mark.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Mark {
    Emphasis,
    Strong,
    Strikethrough,
    Underline,
    Superscript,
    Subscript,
    SmallCaps,
    Code,
    Link(LinkTarget),
}

/// Canonical, order-independent inline marks.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct MarkSet(Arc<[Mark]>);

impl MarkSet {
    pub fn new(marks: impl IntoIterator<Item = Mark>) -> Result<Self, TextIrError> {
        let mut marks: Vec<_> = marks.into_iter().collect();
        marks.sort();
        marks.dedup();
        if marks
            .iter()
            .filter(|mark| matches!(mark, Mark::Link(_)))
            .count()
            > 1
        {
            return Err(TextIrError::DuplicateLinkMark);
        }
        Ok(Self(marks.into()))
    }

    pub fn empty() -> Self {
        Self::default()
    }
    pub fn marks(&self) -> &[Mark] {
        &self.0
    }

    pub fn with_mark(&self, mark: Mark) -> Result<Self, TextIrError> {
        let mut marks = self.0.to_vec();
        marks.push(mark);
        Self::new(marks)
    }

    pub fn contains(&self, mark: &Mark) -> bool {
        self.0.binary_search(mark).is_ok()
    }
}

/// Immutable ordered inline content.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct InlineContent {
    items: Arc<[Inline]>,
}

impl InlineContent {
    pub fn new(items: impl IntoIterator<Item = Inline>) -> Self {
        Self {
            items: items.into_iter().collect(),
        }
    }

    pub fn empty() -> Self {
        Self::default()
    }
    pub fn items(&self) -> &[Inline] {
        &self.items
    }
    pub fn iter(&self) -> impl Iterator<Item = &Inline> {
        self.items.iter()
    }
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
    pub fn len(&self) -> usize {
        self.items.len()
    }
}

/// Generic inline semantic kind.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InlineKind {
    Text(TextRun),
    Break(BreakKind),
    Image(Image),
    RawInline { format: FormatId, body: LiteralText },
}

/// Immutable inline semantic value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Inline(Arc<InlineData>);

#[derive(Clone, Debug, PartialEq, Eq)]
struct InlineData {
    kind: InlineKind,
    marks: MarkSet,
    annotations: Annotations,
}

impl Inline {
    pub fn new(kind: InlineKind) -> Self {
        Self(Arc::new(InlineData {
            kind,
            marks: MarkSet::default(),
            annotations: Annotations::default(),
        }))
    }

    pub fn text(run: TextRun) -> Self {
        Self::new(InlineKind::Text(run))
    }
    pub fn break_(kind: BreakKind) -> Self {
        Self::new(InlineKind::Break(kind))
    }
    pub fn image(image: Image) -> Self {
        Self::new(InlineKind::Image(image))
    }
    pub fn raw(format: FormatId, body: LiteralText) -> Self {
        Self::new(InlineKind::RawInline { format, body })
    }

    pub fn kind(&self) -> &InlineKind {
        &self.0.kind
    }
    pub fn marks(&self) -> &MarkSet {
        &self.0.marks
    }
    pub fn annotations(&self) -> &Annotations {
        &self.0.annotations
    }

    pub fn as_text(&self) -> Option<&TextRun> {
        match &self.0.kind {
            InlineKind::Text(text) => Some(text),
            _ => None,
        }
    }

    pub fn with_marks(&self, marks: MarkSet) -> Self {
        Self(Arc::new(InlineData {
            kind: self.0.kind.clone(),
            marks,
            annotations: self.0.annotations.clone(),
        }))
    }

    pub fn with_annotations(&self, annotations: Annotations) -> Self {
        Self(Arc::new(InlineData {
            kind: self.0.kind.clone(),
            marks: self.0.marks.clone(),
            annotations,
        }))
    }
    pub fn ptr_eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
    pub fn map_annotations(&self, map: impl FnOnce(Annotations) -> Annotations) -> Self {
        self.with_annotations(map(self.annotations().clone()))
    }

    pub(crate) fn from_parts(kind: InlineKind, marks: MarkSet, annotations: Annotations) -> Self {
        Self(Arc::new(InlineData {
            kind,
            marks,
            annotations,
        }))
    }
}

/// A terminal image value with semantic alt content.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    destination: Arc<str>,
    title: Option<Arc<str>>,
    alt: InlineContent,
}

impl Image {
    pub fn new(
        destination: impl Into<Arc<str>>,
        title: Option<impl Into<Arc<str>>>,
        alt: InlineContent,
    ) -> Self {
        Self {
            destination: destination.into(),
            title: title.map(Into::into),
            alt,
        }
    }

    pub fn destination(&self) -> &str {
        &self.destination
    }
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }
    pub fn alt(&self) -> &InlineContent {
        &self.alt
    }
}
