export {
  TuiError,
  asTuiError,
  isTuiCancelledError,
  isTuiError,
  tuiError,
} from "./errors.ts";
export type {
  AppHarness,
  Component,
  ComponentAdapter,
  ComponentCapabilities,
  ComponentContext,
  History,
  HistoryLayout,
  KeyEvent,
  NativeHandle,
  NativeHandleId,
  Output,
  PasteEvent,
  Projector,
  RenderContext,
  Renderer,
  ResizeEvent,
  Scene,
  StreamPane,
  StreamSnapshot,
  StreamingSource,
  TerminateEvent,
  TextContent,
  TextInput,
  TextRewriter,
  TextStream,
  TextVisitor,
  TuiEvent,
  TuiOpenOptions,
  TuiOperation,
  TuiRuntime,
  TuiFailure,
} from "./types.ts";
export { View, ChildrenBuilder } from "./values/view.ts";
export { Insets } from "./values/geometry.ts";
export { Style, StyleSpec } from "./values/style.ts";
export { TextSpan } from "./values/text.ts";
export { materializeView } from "./materialize.ts";

export const tuiSmoke = "iyon:tui/t5" as const;
