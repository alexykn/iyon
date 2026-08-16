import type { JsonValue } from "@iyon/sdk";

export const DEFAULT_MODEL_MAX_LINES = 2_000;
export const DEFAULT_MODEL_MAX_BYTES = 50 * 1024;
export const GREP_MAX_LINE_CHARS = 500;

export type TruncationStrategy = "head" | "tail" | "line";
export type TruncatedBy = "lines" | "bytes" | "characters";

export interface ModelOutputLimits {
  readonly maxLines: number;
  readonly maxBytes: number;
}

export interface TruncationReport {
  readonly scope: "model" | "grep";
  readonly strategy: TruncationStrategy;
  readonly truncated: boolean;
  readonly truncatedBy?: TruncatedBy;
  readonly totalLines: number;
  readonly outputLines: number;
  readonly totalBytes: number;
  readonly outputBytes: number;
  readonly maxLines?: number;
  readonly maxBytes?: number;
  readonly firstLineExceedsLimit: boolean;
  readonly lastLinePartial: boolean;
}

export interface TruncatedText {
  readonly text: string;
  readonly report: TruncationReport;
}

export const DEFAULT_MODEL_LIMITS: ModelOutputLimits = {
  maxLines: DEFAULT_MODEL_MAX_LINES,
  maxBytes: DEFAULT_MODEL_MAX_BYTES,
};

export function truncateHead(content: string, limits: ModelOutputLimits, scope: TruncationReport["scope"] = "model"): TruncatedText {
  return truncateFromLines(content, limits, "head", scope);
}

export function truncateTail(content: string, limits: ModelOutputLimits, scope: TruncationReport["scope"] = "model"): TruncatedText {
  const lines = content.split("\n");
  const totalBytes = byteLength(content);
  if (lines.length <= limits.maxLines && totalBytes <= limits.maxBytes) return unchanged(content, limits, "tail", scope);

  const selected: string[] = [];
  let outputBytes = 0;
  let truncatedBy: TruncatedBy = "lines";
  let lastLinePartial = false;
  for (const line of lines.slice().reverse().slice(0, limits.maxLines)) {
    const separatorBytes = selected.length > 0 ? 1 : 0;
    const lineBytes = byteLength(line) + separatorBytes;
    if (outputBytes + lineBytes > limits.maxBytes) {
      truncatedBy = "bytes";
      if (selected.length === 0) {
        selected.push(takeBytesFromEnd(line, limits.maxBytes));
        lastLinePartial = true;
      }
      break;
    }
    outputBytes += lineBytes;
    selected.push(line);
  }
  selected.reverse();
  const text = selected.join("\n");
  return { text, report: report(scope, "tail", true, truncatedBy, lines.length, selected.length, totalBytes, byteLength(text), limits, false, lastLinePartial) };
}

export function truncateLine(line: string, maxChars: number): { readonly text: string; readonly truncated: boolean } {
  if ([...line].length <= maxChars) return { text: line, truncated: false };
  return { text: `${[...line].slice(0, maxChars).join("")}... [truncated]`, truncated: true };
}

export function truncateGrepLines(lines: readonly string[], maxLines: number, maxBytes = DEFAULT_MODEL_MAX_BYTES): TruncatedText {
  const content = lines.join("\n");
  return truncateHead(content, { maxLines, maxBytes }, "grep");
}

export function truncationDetails(value: TruncationReport): JsonValue {
  return { truncation: value as unknown as JsonValue };
}

function truncateFromLines(content: string, limits: ModelOutputLimits, strategy: "head", scope: TruncationReport["scope"]): TruncatedText {
  const lines = content.split("\n");
  const totalBytes = byteLength(content);
  if (lines.length <= limits.maxLines && totalBytes <= limits.maxBytes) return unchanged(content, limits, strategy, scope);
  const firstLineExceedsLimit = byteLength(lines[0] ?? "") > limits.maxBytes;
  if (firstLineExceedsLimit) return { text: "", report: report(scope, strategy, true, "bytes", lines.length, 0, totalBytes, 0, limits, true, false) };

  const selected: string[] = [];
  let outputBytes = 0;
  let truncatedBy: TruncatedBy = "lines";
  for (const line of lines.slice(0, limits.maxLines)) {
    const separatorBytes = selected.length > 0 ? 1 : 0;
    const lineBytes = byteLength(line) + separatorBytes;
    if (outputBytes + lineBytes > limits.maxBytes) {
      truncatedBy = "bytes";
      break;
    }
    outputBytes += lineBytes;
    selected.push(line);
  }
  const text = selected.join("\n");
  return { text, report: report(scope, strategy, true, truncatedBy, lines.length, selected.length, totalBytes, byteLength(text), limits, false, false) };
}

function unchanged(content: string, limits: ModelOutputLimits, strategy: TruncationStrategy, scope: TruncationReport["scope"]): TruncatedText {
  const totalLines = content.split("\n").length;
  const totalBytes = byteLength(content);
  return { text: content, report: report(scope, strategy, false, undefined, totalLines, totalLines, totalBytes, totalBytes, limits, false, false) };
}

function report(scope: TruncationReport["scope"], strategy: TruncationStrategy, truncated: boolean, truncatedBy: TruncatedBy | undefined, totalLines: number, outputLines: number, totalBytes: number, outputBytes: number, limits: ModelOutputLimits, firstLineExceedsLimit: boolean, lastLinePartial: boolean): TruncationReport {
  return { scope, strategy, truncated, ...(truncatedBy ? { truncatedBy } : {}), totalLines, outputLines, totalBytes, outputBytes, maxLines: limits.maxLines, maxBytes: limits.maxBytes, firstLineExceedsLimit, lastLinePartial };
}

function byteLength(value: string): number { return new TextEncoder().encode(value).byteLength; }

function takeBytesFromEnd(value: string, maxBytes: number): string {
  if (byteLength(value) <= maxBytes) return value;
  let result = "";
  for (const char of [...value].reverse()) {
    if (byteLength(char + result) > maxBytes) break;
    result = char + result;
  }
  return result;
}
