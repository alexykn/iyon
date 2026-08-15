use std::{error::Error, fmt};

use iyon_tui::{
    DiffHunk, DiffLine, DiffLineNumber, DiffLineOffset, DiffLineTermination, DiffRange,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct UnifiedDiffParseError(String);

impl UnifiedDiffParseError {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for UnifiedDiffParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for UnifiedDiffParseError {}

pub(crate) fn parse_unified_diff(input: &str) -> Result<Vec<DiffHunk>, UnifiedDiffParseError> {
    let mut hunks = Vec::new();
    let mut current: Option<HunkBuilder> = None;

    for (index, line) in input.lines().enumerate() {
        if line.starts_with("@@") {
            let (old, new_side) =
                parse_hunk_header(line).map_err(|error| at_line(index + 1, error))?;
            if let Some(builder) = current.take() {
                hunks.push(
                    builder
                        .finish()
                        .map_err(|error| at_line(index + 1, error))?,
                );
            }
            current = Some(HunkBuilder::new(old, new_side)?);
            continue;
        }

        if let Some(builder) = current.as_mut() {
            parse_body_line(builder, line).map_err(|error| at_line(index + 1, error))?;
            continue;
        }

        if is_file_header(line) {
            continue;
        }

        return Err(at_line(
            index + 1,
            UnifiedDiffParseError::new("unexpected line outside a unified diff hunk"),
        ));
    }

    if let Some(builder) = current {
        hunks.push(builder.finish()?);
    }
    Ok(hunks)
}

fn at_line(line: usize, error: UnifiedDiffParseError) -> UnifiedDiffParseError {
    UnifiedDiffParseError::new(format!("line {line}: {error}"))
}

fn is_file_header(line: &str) -> bool {
    line.starts_with("--- ") || line.starts_with("+++ ")
}

fn parse_hunk_header(line: &str) -> Result<(DiffRange, DiffRange), UnifiedDiffParseError> {
    let body = line
        .strip_prefix("@@")
        .and_then(|rest| rest.split_once("@@"))
        .ok_or_else(|| UnifiedDiffParseError::new("malformed hunk header"))?
        .0
        .trim();
    let mut fields = body.split_whitespace();
    let old = fields
        .next()
        .ok_or_else(|| UnifiedDiffParseError::new("hunk header is missing its old range"))?;
    let new_side = fields
        .next()
        .ok_or_else(|| UnifiedDiffParseError::new("hunk header is missing its new range"))?;
    if fields.next().is_some() {
        return Err(UnifiedDiffParseError::new(
            "hunk header contains too many ranges",
        ));
    }

    Ok((parse_range(old, '-')?, parse_range(new_side, '+')?))
}

fn parse_range(token: &str, prefix: char) -> Result<DiffRange, UnifiedDiffParseError> {
    let value = token
        .strip_prefix(prefix)
        .ok_or_else(|| UnifiedDiffParseError::new("hunk range has an invalid side marker"))?;
    let (start_text, count_text) = value.split_once(',').map_or((value, "1"), |parts| parts);
    let start = start_text
        .parse::<u64>()
        .map_err(|_| UnifiedDiffParseError::new("hunk range has an invalid start"))?;
    let count = count_text
        .parse::<u64>()
        .map_err(|_| UnifiedDiffParseError::new("hunk range has an invalid count"))?;
    if count > 0 && start == 0 {
        return Err(UnifiedDiffParseError::new(
            "non-empty hunk ranges cannot start at zero",
        ));
    }
    let offset = if count == 0 {
        start
    } else {
        start
            .checked_sub(1)
            .ok_or_else(|| UnifiedDiffParseError::new("hunk range underflows its offset"))?
    };
    DiffRange::new(DiffLineOffset::new(offset), count)
        .map_err(|error| UnifiedDiffParseError::new(error.to_string()))
}

struct HunkBuilder {
    old: DiffRange,
    new_side: DiffRange,
    old_consumed: u64,
    new_consumed: u64,
    next_old: Option<u64>,
    next_new: Option<u64>,
    lines: Vec<DiffLine>,
}

impl HunkBuilder {
    fn new(old: DiffRange, new_side: DiffRange) -> Result<Self, UnifiedDiffParseError> {
        Ok(Self {
            old,
            new_side,
            old_consumed: 0,
            new_consumed: 0,
            next_old: first_line(old)?,
            next_new: first_line(new_side)?,
            lines: Vec::new(),
        })
    }

    fn finish(self) -> Result<DiffHunk, UnifiedDiffParseError> {
        DiffHunk::new(self.old, self.new_side, self.lines)
            .map_err(|error| UnifiedDiffParseError::new(error.to_string()))
    }

    fn take_old(&mut self) -> Result<DiffLineNumber, UnifiedDiffParseError> {
        take_coordinate(&mut self.next_old, "old")
    }

    fn take_new(&mut self) -> Result<DiffLineNumber, UnifiedDiffParseError> {
        take_coordinate(&mut self.next_new, "new")
    }

    fn consume_old(&mut self) -> Result<(), UnifiedDiffParseError> {
        self.old_consumed = self
            .old_consumed
            .checked_add(1)
            .ok_or_else(|| UnifiedDiffParseError::new("old hunk line count overflows"))?;
        if self.old_consumed > self.old.line_count() {
            return Err(UnifiedDiffParseError::new(
                "hunk body consumes more old lines than its header",
            ));
        }
        Ok(())
    }

    fn consume_new(&mut self) -> Result<(), UnifiedDiffParseError> {
        self.new_consumed = self
            .new_consumed
            .checked_add(1)
            .ok_or_else(|| UnifiedDiffParseError::new("new hunk line count overflows"))?;
        if self.new_consumed > self.new_side.line_count() {
            return Err(UnifiedDiffParseError::new(
                "hunk body consumes more new lines than its header",
            ));
        }
        Ok(())
    }
}

fn first_line(range: DiffRange) -> Result<Option<u64>, UnifiedDiffParseError> {
    if range.is_empty() {
        return Ok(None);
    }
    range
        .start()
        .as_u64()
        .checked_add(1)
        .ok_or_else(|| UnifiedDiffParseError::new("hunk range overflows its first line"))
        .map(Some)
}

fn take_coordinate(
    coordinate: &mut Option<u64>,
    side: &str,
) -> Result<DiffLineNumber, UnifiedDiffParseError> {
    let value = coordinate
        .take()
        .ok_or_else(|| UnifiedDiffParseError::new(format!("hunk body has no {side} line left")))?;
    let number = DiffLineNumber::new(value)
        .ok_or_else(|| UnifiedDiffParseError::new(format!("{side} line number is zero")))?;
    *coordinate = Some(
        value
            .checked_add(1)
            .ok_or_else(|| UnifiedDiffParseError::new(format!("{side} line number overflows")))?,
    );
    Ok(number)
}

fn parse_body_line(builder: &mut HunkBuilder, line: &str) -> Result<(), UnifiedDiffParseError> {
    if matches!(
        line,
        "\\ No newline at end of file" | "No newline at end of file"
    ) {
        let previous = builder.lines.last_mut().ok_or_else(|| {
            UnifiedDiffParseError::new("no-newline marker has no preceding logical line")
        })?;
        *previous = previous
            .clone()
            .with_termination(DiffLineTermination::Unterminated);
        return Ok(());
    }

    if line.is_empty() {
        return Err(UnifiedDiffParseError::new(
            "empty unified diff body line has no prefix",
        ));
    }

    let marker = line
        .chars()
        .next()
        .ok_or_else(|| UnifiedDiffParseError::new("empty unified diff body line has no prefix"))?;
    let payload = &line[marker.len_utf8()..];
    match marker {
        ' ' => {
            builder.consume_old()?;
            builder.consume_new()?;
            let old = builder.take_old()?;
            let new = builder.take_new()?;
            builder.lines.push(DiffLine::context(old, new, payload));
        }
        '+' => {
            builder.consume_new()?;
            let new = builder.take_new()?;
            builder.lines.push(DiffLine::addition(new, payload));
        }
        '-' => {
            builder.consume_old()?;
            let old = builder.take_old()?;
            builder.lines.push(DiffLine::deletion(old, payload));
        }
        _ => {
            return Err(UnifiedDiffParseError::new(
                "unknown unified diff body line prefix",
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use iyon_tui::DiffLineKind;

    fn number(value: u64) -> DiffLineNumber {
        DiffLineNumber::new(value).unwrap()
    }

    fn range(start: u64, count: u64) -> DiffRange {
        DiffRange::new(DiffLineOffset::new(start), count).unwrap()
    }

    fn parse(input: &str) -> Vec<DiffHunk> {
        parse_unified_diff(input).unwrap()
    }

    fn producer_diff(path: &str, before: &str, after: &str) -> String {
        similar::TextDiff::from_lines(before, after)
            .unified_diff()
            .header(&format!("a/{path}"), &format!("b/{path}"))
            .to_string()
    }

    #[test]
    fn parses_one_hunk_and_file_headers_without_classifying_headers() {
        let hunks = parse("--- a/file\n+++ b/file\n@@ -1,2 +1,2 @@ suffix\n context\n-old\n+new\n");
        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].old(), range(0, 2));
        assert_eq!(hunks[0].new_side(), range(0, 2));
        assert_eq!(hunks[0].lines()[0].kind(), DiffLineKind::Context);
        assert_eq!(hunks[0].lines()[0].old_line(), Some(number(1)));
        assert_eq!(hunks[0].lines()[0].new_line(), Some(number(1)));
        assert_eq!(hunks[0].lines()[0].text(), "context");
        assert_eq!(hunks[0].lines()[1].kind(), DiffLineKind::Deletion);
        assert_eq!(hunks[0].lines()[1].old_line(), Some(number(2)));
        assert_eq!(hunks[0].lines()[1].new_line(), None);
        assert_eq!(hunks[0].lines()[1].text(), "old");
        assert_eq!(hunks[0].lines()[2].kind(), DiffLineKind::Addition);
        assert_eq!(hunks[0].lines()[2].old_line(), None);
        assert_eq!(hunks[0].lines()[2].new_line(), Some(number(2)));
        assert_eq!(hunks[0].lines()[2].text(), "new");
    }

    #[test]
    fn parses_multiple_hunks_and_omitted_counts() {
        let hunks = parse("@@ -1 +1 @@\n-a\n+b\n@@ -4,2 +4,3 @@\n c\n-d\n+e\n+f\n");
        assert_eq!(hunks.len(), 2);
        assert_eq!(hunks[0].old(), range(0, 1));
        assert_eq!(hunks[0].new_side(), range(0, 1));
        assert_eq!(hunks[1].old(), range(3, 2));
        assert_eq!(hunks[1].new_side(), range(3, 3));
        assert_eq!(hunks[1].lines()[0].text(), "c");
    }

    #[test]
    fn translates_zero_length_sides_and_insertions() {
        let beginning = parse("@@ -0,0 +1,2 @@\n+x\n+y\n");
        assert_eq!(beginning[0].old(), range(0, 0));
        assert_eq!(beginning[0].new_side(), range(0, 2));
        assert_eq!(beginning[0].lines()[0].new_line(), Some(number(1)));
        assert_eq!(beginning[0].lines()[1].new_line(), Some(number(2)));

        let after_existing = parse("@@ -2,0 +3,1 @@\n+insert\n");
        assert_eq!(after_existing[0].old(), range(2, 0));
        assert_eq!(after_existing[0].new_side(), range(2, 1));
        assert_eq!(after_existing[0].lines()[0].new_line(), Some(number(3)));

        let deletion = parse("@@ -2,2 +2,0 @@\n-left\n-right\n");
        assert_eq!(deletion[0].old(), range(1, 2));
        assert_eq!(deletion[0].new_side(), range(2, 0));
        assert_eq!(deletion[0].lines()[0].old_line(), Some(number(2)));
        assert_eq!(deletion[0].lines()[1].old_line(), Some(number(3)));
    }

    #[test]
    fn preserves_blank_and_marker_like_payloads() {
        let hunks = parse("@@ -1,2 +1,2 @@\n \n-+deleted\n+@@added\n");
        assert_eq!(hunks[0].lines()[0].text(), "");
        assert_eq!(hunks[0].lines()[1].text(), "+deleted");
        assert_eq!(hunks[0].lines()[2].text(), "@@added");
        assert_eq!(hunks[0].lines()[0].kind(), DiffLineKind::Context);
    }

    #[test]
    fn applies_unterminated_markers_to_the_preceding_line() {
        let old = parse("@@ -1 +0,0 @@\n-old\n\\ No newline at end of file\n");
        assert_eq!(
            old[0].lines()[0].termination(),
            DiffLineTermination::Unterminated
        );

        let new = parse("@@ -0,0 +1 @@\n+new\nNo newline at end of file\n");
        assert_eq!(
            new[0].lines()[0].termination(),
            DiffLineTermination::Unterminated
        );

        let both = parse(
            "@@ -1 +1 @@\n-old\n\\ No newline at end of file\n+new\n\\ No newline at end of file\n",
        );
        assert!(
            both[0]
                .lines()
                .iter()
                .all(|line| line.termination() == DiffLineTermination::Unterminated)
        );
    }

    #[test]
    fn rejects_malformed_headers_body_mismatches_and_orphan_markers() {
        for input in [
            "@@ -bad +1 @@\n+x\n",
            "@@ -1 +bad @@\n+x\n",
            "@@ -1 +1\n-x\n+y\n",
            "@@ -1 +1 @@\n+x\n",
            "@@ -1 +1 @@\n?x\n",
            "@@ -1 +1 @@\néx\n",
            "@@ -1 +1 @@\n\\ No newline at end of file\n",
        ] {
            assert!(parse_unified_diff(input).is_err(), "accepted {input:?}");
        }
    }

    #[test]
    fn producer_output_is_compatible_for_common_edit_shapes() {
        let cases = [
            ("old\nvalue\n", "old\nchanged\n", false),
            ("one\n", "one\ntwo\n", false),
            ("one\ntwo\n", "one\n", false),
            (
                "line 1\nline 2\nline 3\nline 4\nline 5\nline 6\nline 7\nline 8\nline 9\nline 10\nline 11\nline 12\nline 13\nline 14\nline 15\nline 16\nline 17\nline 18\nline 19\nline 20\n",
                "line 1\nchanged 2\nline 3\nline 4\nline 5\nline 6\nline 7\nline 8\nline 9\nline 10\nline 11\nline 12\nline 13\nline 14\nline 15\nline 16\nline 17\nchanged 18\nline 19\nline 20\n",
                true,
            ),
            ("before", "after\n", false),
            ("before\n", "after", false),
            ("before", "after", false),
        ];

        for (before, after, multiple_hunks) in cases {
            let diff = producer_diff("src/file.rs", before, after);
            let hunks = parse_unified_diff(&diff)
                .unwrap_or_else(|error| panic!("could not parse producer diff {diff:?}: {error}"));
            assert!(!hunks.is_empty());
            if multiple_hunks {
                assert!(
                    hunks.len() > 1,
                    "producer did not emit multiple hunks: {diff}"
                );
            }
        }
    }
}
