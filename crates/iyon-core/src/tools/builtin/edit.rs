use std::{ops::Range, path::PathBuf};

use anyhow::{Context, bail};
use iyon_api::ContentBlock;
use serde::Deserialize;
use serde_json::{Value, json};
use similar::TextDiff;
use tokio::fs;

use crate::tools::{
    FileMutationQueue, ToolApprovalPolicy, ToolContext, ToolDefinition, ToolExecutionMode,
    ToolExecutor, ToolFuture, ToolResult, ToolSource, ToolUpdateSink,
};

#[derive(Debug)]
pub struct EditTool {
    mutation_queue: FileMutationQueue,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EditInput {
    path: String,
    edits: Vec<TextEdit>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TextEdit {
    old_text: String,
    new_text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LineEnding {
    Lf,
    Crlf,
    Cr,
}

impl EditTool {
    pub fn new(mutation_queue: FileMutationQueue) -> Self {
        Self { mutation_queue }
    }
}

impl ToolExecutor for EditTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "edit".to_string(),
            label: "edit".to_string(),
            description: "Edit a single file using exact text replacement. Every edits[].oldText must match a unique, non-overlapping region of the original file. If two changes affect the same block or nearby lines, merge them into one edit instead of emitting overlapping edits. Do not include large unchanged regions just to connect distant changes.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to the file to edit (relative or absolute)"
                    },
                    "edits": {
                        "type": "array",
                        "description": "One or more targeted replacements. Each edit is matched against the original file, not incrementally. Do not include overlapping or nested edits.",
                        "items": {
                            "type": "object",
                            "properties": {
                                "oldText": {
                                    "type": "string",
                                    "description": "Exact text for one targeted replacement. It must be unique in the original file."
                                },
                                "newText": {
                                    "type": "string",
                                    "description": "Replacement text for this targeted edit."
                                }
                            },
                            "required": ["oldText", "newText"],
                            "additionalProperties": false
                        }
                    }
                },
                "required": ["path", "edits"],
                "additionalProperties": false
            }),
            execution_mode: ToolExecutionMode::Sequential,
            approval: ToolApprovalPolicy::NeverAsk,
            source: ToolSource::Builtin,
            prompt_snippet: Some("Make precise file edits with exact text replacement, including multiple disjoint edits in one call".to_string()),
            prompt_guidelines: vec![
                "Use edit for precise changes (edits[].oldText must match exactly)".to_string(),
                "When changing multiple separate locations in one file, use one edit call with multiple entries instead of multiple edit calls".to_string(),
                "Each edits[].oldText is matched against the original file, not after earlier edits are applied. Do not emit overlapping or nested edits.".to_string(),
                "Keep edits[].oldText as small as possible while still being unique in the file.".to_string(),
            ],
        }
    }

    fn execute(&self, ctx: ToolContext, input: Value, _updates: ToolUpdateSink) -> ToolFuture<'_> {
        let queue = self.mutation_queue.clone();
        Box::pin(async move {
            let input = parse_edit_input(input)?;
            validate_input(&input)?;
            ensure_not_cancelled(&ctx)?;
            let path = ctx.workspace.resolve_write_path(&input.path)?;
            queue
                .run(path.clone(), || async move {
                    edit_file(&ctx, path, input).await
                })
                .await
        })
    }
}

fn parse_edit_input(mut input: Value) -> anyhow::Result<EditInput> {
    normalize_legacy_edit_input(&mut input);
    serde_json::from_value(input).context("invalid edit input")
}

fn normalize_legacy_edit_input(input: &mut Value) {
    let Some(object) = input.as_object_mut() else {
        return;
    };
    if let Some(edits) = object.get_mut("edits")
        && let Some(text) = edits.as_str()
        && let Ok(parsed) = serde_json::from_str::<Value>(text)
        && parsed.is_array()
    {
        *edits = parsed;
    }

    let Some(old_text) = object.remove("oldText") else {
        return;
    };
    let Some(new_text) = object.remove("newText") else {
        object.insert("oldText".to_string(), old_text);
        return;
    };
    let edit = json!({ "oldText": old_text, "newText": new_text });
    if let Some(edits) = object
        .entry("edits".to_string())
        .or_insert_with(|| json!([]))
        .as_array_mut()
    {
        edits.push(edit)
    }
}

fn validate_input(input: &EditInput) -> anyhow::Result<()> {
    if input.path.trim().is_empty() {
        bail!("edit path must not be empty");
    }
    if input.edits.is_empty() {
        bail!("Edit tool input is invalid. edits must contain at least one replacement.");
    }
    if input.edits.iter().any(|edit| edit.old_text.is_empty()) {
        bail!("edit oldText must not be empty");
    }
    Ok(())
}

async fn edit_file(
    ctx: &ToolContext,
    path: PathBuf,
    input: EditInput,
) -> anyhow::Result<ToolResult> {
    ensure_not_cancelled(ctx)?;
    let raw = fs::read_to_string(&path)
        .await
        .with_context(|| format!("could not edit file: {}", input.path))?;
    ensure_not_cancelled(ctx)?;

    let (bom, content) = strip_bom(&raw);
    let line_ending = detect_line_ending(content);
    let base_content = normalize_to_lf(content);
    let edits = normalize_edits(&input.edits);
    let ranges = find_replacement_ranges(&base_content, &edits, &input.path)?;
    validate_non_overlapping(&ranges, &input.path)?;
    let new_content = apply_replacements(&base_content, &edits, &ranges);
    let final_content = format!("{bom}{}", restore_line_endings(&new_content, line_ending));

    fs::write(&path, final_content)
        .await
        .with_context(|| format!("failed to write edited file: {}", path.display()))?;
    ensure_not_cancelled(ctx)?;

    let diff = generate_diff(&input.path, &base_content, &new_content);
    let first_changed_line = first_changed_line(&base_content, &ranges);
    Ok(ToolResult {
        content: vec![ContentBlock::Text {
            text: format!(
                "Successfully replaced {} block(s) in {}.",
                input.edits.len(),
                input.path
            ),
        }],
        details: json!({
            "diff": diff,
            "firstChangedLine": first_changed_line,
        }),
        is_error: false,
        terminate: false,
    })
}

fn normalize_edits(edits: &[TextEdit]) -> Vec<TextEdit> {
    edits
        .iter()
        .map(|edit| TextEdit {
            old_text: normalize_to_lf(&edit.old_text),
            new_text: normalize_to_lf(&edit.new_text),
        })
        .collect()
}

fn find_replacement_ranges(
    content: &str,
    edits: &[TextEdit],
    path: &str,
) -> anyhow::Result<Vec<Range<usize>>> {
    edits
        .iter()
        .map(|edit| find_unique_range(content, &edit.old_text, path))
        .collect()
}

fn find_unique_range(content: &str, old_text: &str, path: &str) -> anyhow::Result<Range<usize>> {
    let matches: Vec<_> = content.match_indices(old_text).collect();
    match matches.as_slice() {
        [] => bail!("oldText not found in {path}: {old_text:?}"),
        [(start, _)] => Ok(*start..start + old_text.len()),
        _ => bail!("oldText must be unique in {path}: {old_text:?}"),
    }
}

fn validate_non_overlapping(ranges: &[Range<usize>], path: &str) -> anyhow::Result<()> {
    let mut sorted = ranges.to_vec();
    sorted.sort_by_key(|range| range.start);
    for pair in sorted.windows(2) {
        if pair[0].end > pair[1].start {
            bail!("edit replacements overlap in {path}");
        }
    }
    Ok(())
}

fn apply_replacements(content: &str, edits: &[TextEdit], ranges: &[Range<usize>]) -> String {
    let mut replacements: Vec<_> = ranges.iter().cloned().zip(edits.iter()).collect();
    replacements.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
    let mut output = content.to_string();
    for (range, edit) in replacements {
        output.replace_range(range, &edit.new_text);
    }
    output
}

fn strip_bom(text: &str) -> (&str, &str) {
    text.strip_prefix('\u{feff}')
        .map_or(("", text), |stripped| ("\u{feff}", stripped))
}

fn detect_line_ending(text: &str) -> LineEnding {
    if text.contains("\r\n") {
        LineEnding::Crlf
    } else if text.contains('\r') {
        LineEnding::Cr
    } else {
        LineEnding::Lf
    }
}

fn normalize_to_lf(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

fn restore_line_endings(text: &str, line_ending: LineEnding) -> String {
    match line_ending {
        LineEnding::Lf => text.to_string(),
        LineEnding::Crlf => text.replace('\n', "\r\n"),
        LineEnding::Cr => text.replace('\n', "\r"),
    }
}

fn generate_diff(path: &str, before: &str, after: &str) -> String {
    TextDiff::from_lines(before, after)
        .unified_diff()
        .header(&format!("a/{path}"), &format!("b/{path}"))
        .to_string()
}

fn first_changed_line(content: &str, ranges: &[Range<usize>]) -> Option<usize> {
    let start = ranges.iter().map(|range| range.start).min()?;
    Some(
        content[..start]
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count()
            + 1,
    )
}

fn ensure_not_cancelled(ctx: &ToolContext) -> anyhow::Result<()> {
    if ctx.cancellation.is_cancelled() {
        bail!("edit tool cancelled");
    }
    Ok(())
}
