import { HandleBase, nativeTui } from "./handles.ts";
import type { TextInput as TextInputContract } from "./types.ts";
import { View } from "./values/view.ts";

export class TextInput extends HandleBase<ReturnType<typeof nativeTui.textInput>, "text-input"> implements TextInputContract {
  constructor(options?: { multiline?: boolean }) { super("text-input", nativeTui.textInput(options?.multiline)); }

  text(): Promise<string> { return this.call(() => this.nativeHandle.text()); }
  cursorBytes(): Promise<number> { return this.call(() => this.nativeHandle.cursorBytes()); }
  setText(value: string): Promise<void> { return this.call(() => this.nativeHandle.setText(value)); }
  clear(): Promise<void> { return this.call(() => this.nativeHandle.clear()); }
  submitted(): Promise<string | null> { return this.call(() => this.nativeHandle.submitted()); }
  setMultiline(enabled: boolean): Promise<void> { return this.call(() => this.nativeHandle.setMultiline(enabled)); }
  isMultiline(): Promise<boolean> { return this.call(() => this.nativeHandle.isMultiline()); }
  async view(): Promise<View> { return View.text(await this.text()); }
}
