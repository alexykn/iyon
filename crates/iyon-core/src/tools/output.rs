use serde::Serialize;
use serde_json::{Value, json};

#[allow(dead_code)]
pub const DEFAULT_MODEL_MAX_LINES: usize = 2_000;
#[allow(dead_code)]
pub const DEFAULT_MODEL_MAX_BYTES: usize = 50 * 1024;
#[allow(dead_code)]
pub const GREP_MAX_LINE_CHARS: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TruncationStrategy {
    Head,
    Tail,
    Line,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TruncatedBy {
    Lines,
    Bytes,
    Characters,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TruncationReport {
    pub scope: &'static str,
    pub strategy: TruncationStrategy,
    pub truncated: bool,
    pub truncated_by: Option<TruncatedBy>,
    pub total_lines: usize,
    pub output_lines: usize,
    pub total_bytes: usize,
    pub output_bytes: usize,
    pub max_lines: Option<usize>,
    pub max_bytes: Option<usize>,
    pub first_line_exceeds_limit: bool,
    pub last_line_partial: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TruncatedText {
    pub text: String,
    pub report: TruncationReport,
}

#[derive(Debug, Clone, Copy)]
pub struct ModelOutputLimits {
    pub max_lines: usize,
    pub max_bytes: usize,
}

impl Default for ModelOutputLimits {
    fn default() -> Self {
        Self {
            max_lines: DEFAULT_MODEL_MAX_LINES,
            max_bytes: DEFAULT_MODEL_MAX_BYTES,
        }
    }
}

pub fn truncate_head(content: &str, limits: ModelOutputLimits) -> TruncatedText {
    truncate_from_lines(content, limits, TruncationStrategy::Head)
}

pub fn truncate_tail(content: &str, limits: ModelOutputLimits) -> TruncatedText {
    let total_bytes = content.len();
    let lines: Vec<&str> = content.split('\n').collect();
    let total_lines = lines.len();
    if total_lines <= limits.max_lines && total_bytes <= limits.max_bytes {
        return unchanged(
            content,
            limits,
            TruncationStrategy::Tail,
            total_lines,
            total_bytes,
        );
    }

    let mut selected = Vec::new();
    let mut output_bytes = 0;
    let mut truncated_by = TruncatedBy::Lines;
    let mut last_line_partial = false;

    for line in lines.iter().rev().take(limits.max_lines) {
        let separator_bytes = usize::from(!selected.is_empty());
        let line_bytes = line.len() + separator_bytes;
        if output_bytes + line_bytes > limits.max_bytes {
            truncated_by = TruncatedBy::Bytes;
            if selected.is_empty() {
                selected.push(truncate_str_from_end_to_bytes(line, limits.max_bytes));
                last_line_partial = true;
            }
            break;
        }
        output_bytes += line_bytes;
        selected.push((*line).to_string());
    }

    selected.reverse();
    let text = selected.join("\n");
    let output_bytes = text.len();
    TruncatedText {
        text,
        report: TruncationReport {
            scope: "model",
            strategy: TruncationStrategy::Tail,
            truncated: true,
            truncated_by: Some(truncated_by),
            total_lines,
            output_lines: selected.len(),
            total_bytes,
            output_bytes,
            max_lines: Some(limits.max_lines),
            max_bytes: Some(limits.max_bytes),
            first_line_exceeds_limit: false,
            last_line_partial,
        },
    }
}

pub fn truncate_line(line: &str, max_chars: usize) -> (String, bool) {
    let char_count = line.chars().count();
    if char_count <= max_chars {
        return (line.to_string(), false);
    }
    let mut output: String = line.chars().take(max_chars).collect();
    output.push_str("... [truncated]");
    (output, true)
}

pub fn truncation_details(report: &TruncationReport) -> Value {
    json!({ "truncation": report })
}

fn truncate_from_lines(
    content: &str,
    limits: ModelOutputLimits,
    strategy: TruncationStrategy,
) -> TruncatedText {
    let total_bytes = content.len();
    let lines: Vec<&str> = content.split('\n').collect();
    let total_lines = lines.len();
    if total_lines <= limits.max_lines && total_bytes <= limits.max_bytes {
        return unchanged(content, limits, strategy, total_lines, total_bytes);
    }

    let first_line_exceeds_limit = lines
        .first()
        .is_some_and(|line| line.len() > limits.max_bytes);
    if first_line_exceeds_limit {
        return TruncatedText {
            text: String::new(),
            report: TruncationReport {
                scope: "model",
                strategy,
                truncated: true,
                truncated_by: Some(TruncatedBy::Bytes),
                total_lines,
                output_lines: 0,
                total_bytes,
                output_bytes: 0,
                max_lines: Some(limits.max_lines),
                max_bytes: Some(limits.max_bytes),
                first_line_exceeds_limit: true,
                last_line_partial: false,
            },
        };
    }

    let mut selected = Vec::new();
    let mut output_bytes = 0;
    let mut truncated_by = TruncatedBy::Lines;
    for line in lines.into_iter().take(limits.max_lines) {
        let separator_bytes = usize::from(!selected.is_empty());
        let line_bytes = line.len() + separator_bytes;
        if output_bytes + line_bytes > limits.max_bytes {
            truncated_by = TruncatedBy::Bytes;
            break;
        }
        output_bytes += line_bytes;
        selected.push(line);
    }

    let text = selected.join("\n");
    let output_bytes = text.len();
    TruncatedText {
        text,
        report: TruncationReport {
            scope: "model",
            strategy,
            truncated: true,
            truncated_by: Some(truncated_by),
            total_lines,
            output_lines: selected.len(),
            total_bytes,
            output_bytes,
            max_lines: Some(limits.max_lines),
            max_bytes: Some(limits.max_bytes),
            first_line_exceeds_limit: false,
            last_line_partial: false,
        },
    }
}

fn unchanged(
    content: &str,
    limits: ModelOutputLimits,
    strategy: TruncationStrategy,
    total_lines: usize,
    total_bytes: usize,
) -> TruncatedText {
    TruncatedText {
        text: content.to_string(),
        report: TruncationReport {
            scope: "model",
            strategy,
            truncated: false,
            truncated_by: None,
            total_lines,
            output_lines: total_lines,
            total_bytes,
            output_bytes: total_bytes,
            max_lines: Some(limits.max_lines),
            max_bytes: Some(limits.max_bytes),
            first_line_exceeds_limit: false,
            last_line_partial: false,
        },
    }
}

fn truncate_str_from_end_to_bytes(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_string();
    }
    let mut start = value.len().saturating_sub(max_bytes);
    while start < value.len() && !value.is_char_boundary(start) {
        start += 1;
    }
    value[start..].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn head_truncates_by_lines() {
        let out = truncate_head(
            "a\nb\nc",
            ModelOutputLimits {
                max_lines: 2,
                max_bytes: 100,
            },
        );
        assert_eq!(out.text, "a\nb");
        assert!(out.report.truncated);
        assert_eq!(out.report.truncated_by, Some(TruncatedBy::Lines));
    }

    #[test]
    fn tail_keeps_recent_lines() {
        let out = truncate_tail(
            "a\nb\nc",
            ModelOutputLimits {
                max_lines: 2,
                max_bytes: 100,
            },
        );
        assert_eq!(out.text, "b\nc");
        assert!(out.report.truncated);
    }

    #[test]
    fn line_truncates_by_chars() {
        let (line, truncated) = truncate_line("abcdef", 3);
        assert!(truncated);
        assert_eq!(line, "abc... [truncated]");
    }
}
