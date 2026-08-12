mod annotations;
mod block;
mod content;
mod errors;
mod inline;
mod provenance;
mod validate;
mod visit;

pub use annotations::{Annotations, SemanticKey, SemanticTag, SemanticValue};
pub use block::{
    Alignment, Block, BlockKind, CodeBlock, HeadingLevel, List, ListItem, ListMarker,
    NumberDelimiter, NumberStyle, Table, TableCell, TableColumn, TableRow,
};
pub use content::{RawText, TextContent};
pub use errors::{TextIrError, TextProjectionError};
pub use inline::{
    BreakKind, FormatId, Image, Inline, InlineContent, InlineKind, LanguageId, LinkTarget, Mark,
    MarkSet,
};
pub use provenance::{LiteralText, TextProvenance, TextRun};
pub use validate::{validate_text_content, validate_text_projection};
pub use visit::{
    TextRewriter, TextVisitor, walk_block, walk_content, walk_inline, walk_inline_content,
    walk_literal, walk_rewrite_block, walk_rewrite_blocks, walk_rewrite_content,
    walk_rewrite_inline, walk_rewrite_inline_content, walk_rewrite_literal,
};
