import { describe, expect, test } from "bun:test";

import { native } from "../src/native.ts";
import { nativeViewAbiSession } from "../src/tui/native_view_abi.ts";
import { runtimeNoop, viewCommonPatchRoot, viewRefForNodeId, viewRenderRef, viewReleaseMany, viewSpacerCreate, viewTextLayoutPatchRoot } from "../src/tui/generated/view_calls.ts";
import { nodeForBridge, nodeIdPair, View } from "../src/tui/values/view.ts";

type AbiHost = {
  render(view: object): void;
  screenRows(): string[];
  dispose(): void;
};

const Host = native.NativeTuiHost as unknown as (new (width: number, height: number, headless: boolean) => AbiHost) | undefined;

describe("PERF-11 generated vertical slice", () => {
  test("links the generated ABI and keeps the runtime pointer stable", () => {
    const session = nativeViewAbiSession();
    if (session === undefined) return;
    expect(session.abi.function_count).toBe(8);
    expect(session.abi.runtime_ptr).toBeGreaterThan(0);
    expect(session.symbols).toBeDefined();
    expect(nativeViewAbiSession()?.abi.runtime_ptr ?? 0).toBe(session.abi.runtime_ptr);
    expect(runtimeNoop(session.symbols, session.runtime)).toBe(1);
  });

  test("creates, resolves, publishes, and releases a native spacer", () => {
    const session = nativeViewAbiSession();
    if (session === undefined) return;
    const view = View.spacer(2);
    const [nodeIdLow, nodeIdHigh] = nodeIdPair(view);
    const reference = viewSpacerCreate(session.symbols, session.runtime, nodeIdLow, nodeIdHigh, 2);
    expect(reference).toBeGreaterThan(0);
    expect(viewRenderRef(session.symbols, session.runtime, reference)).toBe(reference);
    expect(viewReleaseMany(session.symbols, session.runtime, new Uint32Array([reference]), 1)).toBe(1);
    expect(() => viewRenderRef(session.symbols, session.runtime, reference)).toThrow();
  });

  test("patches text metadata and publishes generated roots into the existing host decoder", () => {
    if (Host === undefined) return;
    const session = nativeViewAbiSession();
    if (session === undefined) return;
    const host = new Host(8, 4, true);
    const direct = new Host(8, 4, true);
    const textBase = View.text("hello");
    const textChanged = textBase.noWrap().textAlign("center");
    try {
      host.render(nodeForBridge(textBase));
      const [textLow, textHigh] = nodeIdPair(textBase);
      const textRef = viewRefForNodeId(session.symbols, session.runtime, textLow, textHigh);
      const [textChangedLow, textChangedHigh] = nodeIdPair(textChanged);
      const textChangedRef = viewTextLayoutPatchRoot(
        session.symbols,
        session.runtime,
        textRef,
        textChangedLow,
        textChangedHigh,
        3,
        2,
      );
      expect(textChangedRef).toBeGreaterThan(0);
      host.render(nodeForBridge(textChanged));
      direct.render(nodeForBridge(textChanged));
      expect(host.screenRows()).toEqual(direct.screenRows());
      viewReleaseMany(session.symbols, session.runtime, new Uint32Array([textRef, textChangedRef]), 2);

      const base = View.spacer(1);
      const [baseLow, baseHigh] = nodeIdPair(base);
      const baseRef = viewSpacerCreate(session.symbols, session.runtime, baseLow, baseHigh, 1);
      const changed = base.padding(1);
      const [changedLow, changedHigh] = nodeIdPair(changed);
      const packedPadding = (1 << 16) | 1;
      const changedRef = viewCommonPatchRoot(
        session.symbols,
        session.runtime,
        baseRef,
        changedLow,
        changedHigh,
        4,
        packedPadding,
        packedPadding,
        1,
        1,
        0,
        0,
        0,
        0,
        baseRef,
      );
      expect(changedRef).toBeGreaterThan(0);
      host.render(nodeForBridge(base));
      host.render(nodeForBridge(changed));
      expect(host.screenRows()).toHaveLength(4);
      viewReleaseMany(session.symbols, session.runtime, new Uint32Array([baseRef, changedRef]), 2);
    } finally {
      direct.dispose();
      host.dispose();
    }
  });
});
