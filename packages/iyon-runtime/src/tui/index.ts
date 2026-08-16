export {
  TuiError,
  asTuiError,
  isTuiCancelledError,
  isTuiError,
  tuiError,
} from "./errors.ts";
export type {
  AppHarness,
  ComponentAdapter,
  ComponentCapabilities,
  ComponentContext,
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
  StreamSnapshot,
  StreamingSource,
  TerminateEvent,
  TextContent,
  TextRewriter,
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
export { History } from "./history.ts";
export { TextInput } from "./text-input.ts";
export { TextStream, StreamPane } from "./stream.ts";
export { Component } from "./component.ts";

export const tuiSmoke = "iyon:tui/t5" as const;
