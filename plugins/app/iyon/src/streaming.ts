import { Insets, TextFunnel, TextStreamSource, View, isTuiError } from "@iyon/tui";
import type { ContentConnector, ContentPort, History, TuiRuntime } from "@iyon/tui";
import type { TextSourceSnapshot } from "@iyon/tui";

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

/**
 * Application-owned Source plus an explicit content-plane Funnel. The
 * ContentPort/Connector are created once for the mounted History occurrence;
 * token/segment mutations never enter React or structural transport.
 */
export class NativeAssistantStream {
  readonly source: TextStreamSource;
  readonly funnel = TextFunnel.markdown().smooth({
    minUnitsPerSecond: 40,
    maxUnitsPerSecond: 800,
  });
  readonly buffer = new AssistantStreamBuffer();
  private attachment?: {
    readonly tui: TuiRuntime;
    readonly port: ContentPort;
    readonly connector: ContentConnector;
  };

  constructor() {
    this.source = TextStreamSource.create();
  }

  attach(tui: TuiRuntime, history: History): void {
    if (this.attachment !== undefined) throw new Error("assistant stream is already attached");
    const port = tui.contentPort();
    const connector = port.connect(this.source, this.funnel);
    connector.activate();
    history.push(View.content(port).fillWidth().padding(Insets.of(0, 2, 0, 2)));
    this.attachment = { tui, port, connector };
  }

  async append(kind: SegmentKind, text: string): Promise<void> {
    if (text.length === 0) return;
    const previous = this.buffer.snapshot().at(-1);
    const normalized = kind === "text"
      && previous?.kind === "thinking"
      && !previous.text.endsWith("\n")
      && !text.startsWith("\n")
      ? `\n\n${text}`
      : text;
    this.buffer.append(kind, normalized);
    this.source.append(
      normalized,
      kind === "thinking" ? [{ namespace: "app", name: "thinking" }] : [],
    );
  }

  snapshot(): TextSourceSnapshot { return this.source.snapshot(); }

  async seal(): Promise<void> {
    this.buffer.seal();
    this.source.seal();
  }

  async dispose(): Promise<void> {
    try {
      this.source.dispose();
      this.attachment = undefined;
      return;
    } catch (error) {
      if (!isTuiError(error) || error.nativeCode !== "ION_SOURCE_IN_USE") throw error;
    }
    const attachment = this.attachment;
    if (attachment === undefined) throw new Error("assistant Source is in use without a Connector attachment");
    attachment.connector.dispose();
    attachment.tui.flush();
    this.attachment = undefined;
    this.source.dispose();
  }
}
