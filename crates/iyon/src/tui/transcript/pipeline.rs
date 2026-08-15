//! Iyon's assistant-specific composition of the generic text pipeline.

use iyon_tui::projection::{Projection, Projector};
use iyon_tui::stream::{StreamOffset, StreamRange};
use iyon_tui::text::{
    CodeBlockLabelPolicy, Inline, InlineContent, InlineKind, ListMarker, LiteralText,
    MarkdownOptions, MarkdownProjectionError, MarkdownProjector, Renderer, RewriteProjectionError,
    SemanticTag, SoftBreakPolicy, TableColumnSizing, TaskListMarkerPolicy, TextContent,
    TextIrError, TextProvenance, TextRenderPolicy, TextRenderer, TextRewriter, TextRun,
    walk_rewrite_inline,
};
use iyon_tui::{View, WrapMode};

use super::assistant_stream::AssistantPacingAtom;
use super::semantic::{SegmentKind, thinking_tag};

#[derive(Debug)]
pub(crate) enum AssistantPipelineError {
    Markdown(MarkdownProjectionError),
    Annotation(RewriteProjectionError<TextIrError>),
}

impl std::fmt::Display for AssistantPipelineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Markdown(error) => error.fmt(f),
            Self::Annotation(error) => error.fmt(f),
        }
    }
}
impl std::error::Error for AssistantPipelineError {}

#[derive(Debug)]
pub(crate) struct AssistantPipeline {
    smoother: iyon_tui::Smooth,
    markdown: MarkdownProjector,
}

impl Default for AssistantPipeline {
    fn default() -> Self {
        Self {
            smoother: iyon_tui::Smooth::default(),
            markdown: MarkdownProjector::new(
                MarkdownOptions::gfm().with_live_table_stabilization(true),
            ),
        }
    }
}

impl AssistantPipeline {
    pub(crate) fn project(
        &mut self,
        input: &Projection<AssistantPacingAtom>,
    ) -> Result<Projection<TextContent>, AssistantPipelineError> {
        let paced = self.smoother.project(input).expect("Smooth is infallible");
        let raw = paced.map_ref(|atom| TextContent::raw(atom.text.clone()));
        let markdown = self
            .markdown
            .project(&raw)
            .map_err(AssistantPipelineError::Markdown)?;
        let tables = super::pipe_table::PipeTableRewriter
            .project(&markdown)
            .map_err(AssistantPipelineError::Annotation)?;
        AssistantThinkingRewriter::new(&paced)
            .into_projector()
            .project(&tables)
            .map_err(AssistantPipelineError::Annotation)
    }

    pub(crate) fn advance(&mut self, now: std::time::Instant) -> bool {
        self.smoother.advance(now)
    }

    pub(crate) fn next_wakeup(&self) -> Option<std::time::Instant> {
        self.smoother.next_wakeup()
    }

    pub(crate) fn restart_from(&self, offset: StreamOffset) -> StreamOffset {
        self.markdown.restart_from(offset)
    }
}

impl Projector<AssistantPacingAtom> for AssistantPipeline {
    type Output = TextContent;
    type Error = AssistantPipelineError;

    fn project(
        &mut self,
        input: &Projection<AssistantPacingAtom>,
    ) -> Result<Projection<Self::Output>, Self::Error> {
        Self::project(self, input)
    }

    fn next_wakeup(&self) -> Option<std::time::Instant> {
        Self::next_wakeup(self)
    }

    fn advance(&mut self, now: std::time::Instant) -> bool {
        Self::advance(self, now)
    }

    fn restart_from(&self, output_from: StreamOffset) -> StreamOffset {
        Self::restart_from(self, output_from)
    }
}

#[derive(Debug)]
struct ThinkingMap(Vec<(StreamRange, SegmentKind)>);

impl ThinkingMap {
    fn from_projection(projection: &Projection<AssistantPacingAtom>) -> Self {
        let mut ranges: Vec<(StreamRange, SegmentKind)> = Vec::new();
        for span in projection.spans() {
            let Some(atom) = span.values().first() else {
                continue;
            };
            if let Some((last, kind)) = ranges.last_mut()
                && *kind == atom.kind
                && last.end() == span.source().start()
            {
                *last = StreamRange::new(last.start(), span.source().end());
            } else {
                ranges.push((span.source(), atom.kind));
            }
        }
        Self(ranges)
    }

    fn kind_for(&self, range: StreamRange) -> Option<SegmentKind> {
        self.0.iter().find_map(|(candidate, kind)| {
            (candidate.start() <= range.start() && range.end() <= candidate.end()).then_some(*kind)
        })
    }
}

struct AssistantThinkingRewriter {
    map: ThinkingMap,
    tag: SemanticTag,
}

impl AssistantThinkingRewriter {
    fn new(paced: &Projection<AssistantPacingAtom>) -> Self {
        Self {
            map: ThinkingMap::from_projection(paced),
            tag: thinking_tag(),
        }
    }

    fn annotate_run(&self, run: TextRun) -> Result<Vec<TextRun>, TextIrError> {
        let TextProvenance::Exact(range) = run.provenance().clone() else {
            let range = match run.provenance() {
                TextProvenance::Derived(range) => *range,
                TextProvenance::Synthetic => return Ok(vec![run]),
                TextProvenance::Exact(_) => unreachable!(),
            };
            return Ok(if self.map.kind_for(range) == Some(SegmentKind::Thinking) {
                vec![run.map_annotations(|annotations| annotations.with_tag(self.tag.clone()))]
            } else {
                vec![run]
            });
        };
        if range.is_empty() {
            return Ok(vec![run]);
        }
        let mut result = Vec::new();
        let mut remaining = run;
        let mut cursor = range.start();
        while !remaining.text().is_empty() {
            let Some((map_range, kind)) = self
                .map
                .0
                .iter()
                .find(|(candidate, _)| candidate.contains_offset(cursor))
            else {
                return Ok(vec![remaining]);
            };
            let end = map_range.end().min(range.end());
            let length = usize::try_from(end.as_u64().saturating_sub(cursor.as_u64()))
                .expect("stream range fits usize");
            let (piece, rest) = if length == remaining.text().len() {
                (remaining, None)
            } else {
                let (left, right) = remaining.split_at(length)?;
                (left, Some(right))
            };
            let piece = if *kind == SegmentKind::Thinking {
                piece.map_annotations(|annotations| annotations.with_tag(self.tag.clone()))
            } else {
                piece
            };
            result.push(piece);
            cursor = end;
            let Some(next) = rest else { break };
            remaining = next;
        }
        Ok(result)
    }
}

impl TextRewriter for AssistantThinkingRewriter {
    type Error = TextIrError;

    fn rewrite_inline(&mut self, inline: Inline) -> Result<Inline, Self::Error> {
        let InlineKind::Text(run) = inline.kind().clone() else {
            return walk_rewrite_inline(self, inline);
        };
        let Some(run) = self.annotate_run(run)?.into_iter().next() else {
            return Ok(inline);
        };
        Ok(Inline::new(InlineKind::Text(run))
            .with_marks(inline.marks().clone())
            .with_annotations(inline.annotations().clone()))
    }

    fn rewrite_inline_content(
        &mut self,
        content: InlineContent,
    ) -> Result<InlineContent, Self::Error> {
        let mut items = Vec::new();
        for inline in content.items() {
            let InlineKind::Text(run) = inline.kind() else {
                items.push(self.rewrite_inline(inline.clone())?);
                continue;
            };
            let runs = self.annotate_run(run.clone())?;
            for run in runs {
                items.push(
                    Inline::new(InlineKind::Text(run))
                        .with_marks(inline.marks().clone())
                        .with_annotations(inline.annotations().clone()),
                );
            }
        }
        Ok(InlineContent::new(items))
    }

    fn rewrite_literal(&mut self, literal: LiteralText) -> Result<LiteralText, Self::Error> {
        let mut runs = Vec::new();
        for run in literal.runs() {
            runs.extend(self.annotate_run(run.clone())?);
        }
        Ok(LiteralText::new(runs))
    }
}

fn assistant_render_policy() -> TextRenderPolicy {
    TextRenderPolicy::new()
        .with_block_gap(1)
        .with_soft_break(SoftBreakPolicy::LineBreak)
        .with_table_column_sizing(TableColumnSizing::Content)
        .with_table_column_gap(1)
        .with_table_row_gap(0)
        .with_task_list_marker(TaskListMarkerPolicy::TaskOnly)
        .with_code_block_label(CodeBlockLabelPolicy::Language)
        .with_code_block_gap(0)
        .with_code_wrap(WrapMode::NoWrap)
}

pub(crate) fn assistant_renderer() -> TextRenderer {
    TextRenderer::with_policy(assistant_render_policy())
}

pub(crate) struct AssistantPresentationChunk {
    pub(crate) source: StreamRange,
    pub(crate) view: View,
}

pub(crate) fn assistant_presentation(
    semantic: &Projection<TextContent>,
    renderer: &TextRenderer,
) -> Vec<AssistantPresentationChunk> {
    let visible: Vec<_> = semantic
        .spans()
        .iter()
        .filter(|span| !span.values().is_empty())
        .collect();
    if visible.is_empty() {
        return if semantic.source_base() == semantic.source_end() {
            Vec::new()
        } else {
            vec![AssistantPresentationChunk {
                source: StreamRange::new(semantic.source_base(), semantic.source_end()),
                view: View::spacer(0).padding(crate::tui::viewport_gutter()),
            }]
        };
    }

    visible
        .iter()
        .enumerate()
        .map(|(index, span)| {
            let start = if index == 0 {
                semantic.source_base()
            } else {
                visible[index - 1].source().end()
            };
            let end = span.source().end();
            let next = visible.get(index + 1).map(|span| span.values());
            let view = Renderer::render(renderer, span.values()).padding(crate::tui::viewport_pad(
                0,
                chunk_bottom_gap(span.values(), next, renderer.policy().block_gap()),
            ));
            AssistantPresentationChunk {
                source: StreamRange::new(start, end),
                view,
            }
        })
        .collect()
}

// Gap lives on the preceding chunk so compacting a prefix does not
// re-pad the remaining suffix. Consecutive tight same-kind lists stay
// visually one list after the root projector splits them per item.
fn chunk_bottom_gap(current: &[TextContent], next: Option<&[TextContent]>, block_gap: u16) -> u16 {
    let Some(next) = next else {
        return 0;
    };
    let curr_list = current.last().and_then(as_list);
    let next_list = next.first().and_then(as_list);
    if let (Some(current), Some(next)) = (curr_list, next_list)
        && same_list_kind(current.marker(), next.marker())
    {
        return if current.tight() && next.tight() {
            0
        } else {
            block_gap
        };
    }
    block_gap
}

fn as_list(content: &TextContent) -> Option<&iyon_tui::text::List> {
    match content {
        TextContent::Block(block) => block.as_list(),
        TextContent::Raw(_) => None,
    }
}

fn same_list_kind(left: ListMarker, right: ListMarker) -> bool {
    match (left, right) {
        (ListMarker::Bullet, ListMarker::Bullet) => true,
        (
            ListMarker::Ordered {
                style: left_style,
                delimiter: left_delimiter,
                ..
            },
            ListMarker::Ordered {
                style: right_style,
                delimiter: right_delimiter,
                ..
            },
        ) => left_style == right_style && left_delimiter == right_delimiter,
        _ => false,
    }
}

pub(crate) fn assistant_view(semantic: &Projection<TextContent>, renderer: &TextRenderer) -> View {
    let chunks = assistant_presentation(semantic, renderer);
    View::vertical(|column| {
        column.children(chunks.into_iter().map(|chunk| chunk.view));
    })
}
