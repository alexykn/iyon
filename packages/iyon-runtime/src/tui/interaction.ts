import type { KeyEvent, PasteEvent, TuiEvent } from "./types.ts";

export class FocusController {
  private focused = 0;
  focus(id: number): void { this.focused = id; }
  current(): number { return this.focused; }
}

export class InteractionRouter {
  constructor(private readonly focus = new FocusController()) {}
  route(event: TuiEvent, key: (event: KeyEvent, focused: number) => boolean, paste: (event: PasteEvent, focused: number) => boolean): boolean {
    if (event.type === "key") return key(event, this.focus.current());
    if (event.type === "paste") return paste(event, this.focus.current());
    return false;
  }
  focusController(): FocusController { return this.focus; }
}
