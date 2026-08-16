import { HandleBase, nativeTui } from "./handles.ts";
import type { StreamSnapshot, TextStream as TextStreamContract } from "./types.ts";

export class TextStream extends HandleBase<ReturnType<typeof nativeTui.textStream>, "text-stream"> implements TextStreamContract {
  constructor(options: { readonly projector?: "markdown" } = {}) { super("text-stream", nativeTui.textStream(options.projector)); }
  update(text: string): Promise<void> { return this.call(() => this.nativeHandle.update(text)); }
  appendSegment(kind: "text" | "thinking", text: string): Promise<void> { return this.call(() => this.nativeHandle.appendSegment(kind, text)); }
  seal(): Promise<void> { return this.call(() => this.nativeHandle.seal()); }
  snapshot(): Promise<StreamSnapshot> { return this.call(() => this.nativeHandle.snapshot() as StreamSnapshot); }
  nativeObject(): object { this.ensureOpen(); return this.nativeHandle; }
}

export { TextStream as StreamPane };
