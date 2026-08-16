import { TextStream } from "@iyon/runtime/tui";
import type { StreamSnapshot, TextStream as TextStreamHandle } from "@iyon/runtime/tui";

export type SegmentKind = "text" | "thinking";
export interface StreamSegment { readonly kind: SegmentKind; readonly text: string; }

export class AssistantStreamBuffer {
  private segments: StreamSegment[] = [];
  private sealed = false;

  append(kind: SegmentKind, text: string): void {
    if (this.sealed) throw new Error("assistant stream is sealed");
    if (text.length === 0) return;
    const previous = this.segments.at(-1);
    if (previous?.kind === kind) this.segments[this.segments.length - 1] = { kind, text: previous.text + text };
    else this.segments.push({ kind, text });
  }
  snapshot(): readonly StreamSegment[] { return this.segments.map((segment) => ({ ...segment })); }
  text(): string { return this.segments.map((segment) => segment.text).join(""); }
  seal(): void { this.sealed = true; }
  isSealed(): boolean { return this.sealed; }
}

export class NativeAssistantStream {
  readonly native: TextStreamHandle;
  readonly buffer = new AssistantStreamBuffer();

  constructor() { this.native = new TextStream(); }
  async append(kind: SegmentKind, text: string): Promise<void> { this.buffer.append(kind, text); await this.native.update(this.buffer.text()); }
  async snapshot(): Promise<StreamSnapshot> { return this.native.snapshot(); }
  async seal(): Promise<void> { this.buffer.seal(); await this.native.seal(); }
  async dispose(): Promise<void> { await this.native.dispose(); }
}
