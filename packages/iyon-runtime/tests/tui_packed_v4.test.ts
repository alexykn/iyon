import { describe, expect, test } from "bun:test";

import { native } from "../src/native.ts";
import { View, nodeForBridge } from "../src/tui/values/view.ts";
import { TextSpan } from "../src/tui/values/text.ts";
import { createPackedV4Encoder, packedV4Snapshot, renderPackedV4View, resetPackedV4Counters } from "../src/tui/packed_v4.ts";

type V4Host = {
  tuiPerfV4ResetViewBridgeCache?: () => void;
  tuiPerfV4PackedRender?: (words: Uint32Array, bytes: Uint8Array) => void;
  tuiPerfV4PackedRenderRef?: (generation: number, packedRef: number) => void;
  render(view: object): void;
  screenRows(): string[];
  dispose(): void;
};

const Host = native.NativeTuiHost as unknown as (new (width: number, height: number, headless: boolean) => V4Host) | undefined;

function available(): boolean {
  if (Host === undefined) return false;
  const host = new Host(1, 1, true);
  try {
    host.tuiPerfV4ResetViewBridgeCache?.();
    return host.tuiPerfV4PackedRender !== undefined && host.tuiPerfV4PackedRenderRef !== undefined;
  } finally {
    host.dispose();
  }
}

function render(encoder: ReturnType<typeof createPackedV4Encoder>, host: V4Host, view: View): void {
  if (host.tuiPerfV4PackedRender === undefined || host.tuiPerfV4PackedRenderRef === undefined) {
    throw new Error("V4 native transport is unavailable");
  }
  renderPackedV4View(
    encoder,
    view,
    (words, bytes) => host.tuiPerfV4PackedRender!(words, bytes),
    (generation, reference) => host.tuiPerfV4PackedRenderRef!(generation, reference),
  );
}

describe("PERF-9 Packed V4 dual-lane transport", () => {
  test("uses one UTF-8 offset table and preserves semantic rendering", () => {
    if (!available()) return;
    const direct = new Host!(40, 8, true);
    const packed = new Host!(40, 8, true);
    const encoder = createPackedV4Encoder();
    const view = View.vertical([
      View.text("héllo 🌍\0x").foreground("cyan"),
      View.text("same"),
      View.text("same"),
      View.text(""),
    ]);
    try {
      direct.render(nodeForBridge(view));
      const transaction = encoder.encodeRoots([nodeForBridge(view)]);
      expect(transaction.words[1]).toBe(4);
      expect(transaction.words[6]).toBe(new TextEncoder().encode("héllo 🌍\0xcyan" ).length + 4);
      expect(transaction.words[10]).toBe(3);
      expect(Array.from(transaction.words.slice(transaction.words[11]))).toEqual([0, 13, 17, 21]);
      render(encoder, packed, view);
      expect(packed.screenRows()).toEqual(direct.screenRows());
      const first = packed.screenRows();
      render(encoder, packed, view);
      expect(packed.screenRows()).toEqual(first);
      expect(packedV4Snapshot().packed_v4_exact_ref_fast_hits).toBeGreaterThan(0);
    } finally {
      direct.dispose();
      packed.dispose();
    }
  });

  test("empty strings use StringRef zero and no byte lane", () => {
    if (!available()) return;
    const host = new Host!(20, 3, true);
    const encoder = createPackedV4Encoder();
    try {
      const transaction = encoder.encodeRoots([nodeForBridge(View.text(""))]);
      expect(transaction.bytes.length).toBe(0);
      expect(transaction.words[4] & 4).toBe(0);
      expect(transaction.words[10]).toBe(0);
      expect(Array.from(transaction.words.slice(transaction.words[11]))).toEqual([0]);
      render(encoder, host, View.text(""));
    } finally {
      host.dispose();
    }
  });

  test("matches direct bridge Unicode semantics, including lone surrogates", () => {
    if (!available()) return;
    const validValues = ["", "ASCII", "é", "漢", "🌍", "a\0b", "e\u0301", "\uD83C\uDF0D", "\uDBFF\uDFFF"];
    const invalidValues = ["\uD800", "\uDC00", "\uD800A", "A\uDC00"];
    const direct = new Host!(30, 2, true);
    const packed = new Host!(30, 2, true);
    const encoder = createPackedV4Encoder();
    const view = View.styledText(validValues.map((value) => TextSpan.plain(value)));
    try {
      direct.render(nodeForBridge(view));
      render(encoder, packed, view);
      expect(packed.screenRows()).toEqual(direct.screenRows());
      for (const value of invalidValues) {
        expect(() => render(createPackedV4Encoder(), packed, View.text(value))).toThrow();
      }
    } finally {
      direct.dispose();
      packed.dispose();
    }
  });

  test("rejects invalid UTF-8 before host mutation", () => {
    if (!available()) return;
    const host = new Host!(30, 4, true);
    const encoder = createPackedV4Encoder();
    const view = View.text("valid 🌍");
    try {
      render(encoder, host, view);
      const before = host.screenRows();
      resetPackedV4Counters();
      const transaction = encoder.encodeRoots([nodeForBridge(View.text("different 🌍"))]);
      const bytes = new Uint8Array(transaction.bytes);
      bytes[0] = 0xff;
      expect(() => host.tuiPerfV4PackedRender!(transaction.words, bytes)).toThrow();
      expect(host.screenRows()).toEqual(before);
    } finally {
      host.dispose();
    }
  });

  test("rejects an offset that splits a UTF-8 scalar", () => {
    if (!available()) return;
    const host = new Host!(30, 4, true);
    const encoder = createPackedV4Encoder();
    const view = View.vertical([View.text("🌍"), View.text("a")]);
    try {
      render(encoder, host, View.text("stable"));
      const before = host.screenRows();
      const transaction = encoder.encodeRoots([nodeForBridge(view)]);
      const words = new Uint32Array(transaction.words);
      words[transaction.words[11] + 1] = 1;
      expect(() => host.tuiPerfV4PackedRender!(words, transaction.bytes)).toThrow();
      expect(host.screenRows()).toEqual(before);
    } finally {
      host.dispose();
    }
  });
});
