import { describe, expect, test } from "bun:test";

import { native } from "../src/native.ts";
import { View } from "../src/tui/values/view.ts";
import { Style } from "../src/tui/values/style.ts";
import { createPackedV3Encoder, packedV3Snapshot, renderPackedV3View, replacePackedAxisChild, replacePackedGridCell, resetPackedV3Counters, splicePackedAxisChildren } from "../src/tui/packed_v3.ts";
import { PersistentSeq } from "../src/tui/persistent_seq.ts";
import { nodeForBridge, nodeForDirectBridge } from "../src/tui/values/view.ts";
import { DiffHunk, DiffLine, DiffRange, DiffRenderer } from "../src/tui/values/diff.ts";

interface V3Host {
  tuiPerfV3PackedRender?: (words: Uint32Array, bytes: Uint8Array) => void;
  tuiPerfV3PackedRenderStrings?: (words: Uint32Array, strings: readonly string[]) => void;
  tuiPerfV3PackedRenderRef?: (generation: number, packedRef: number) => void;
  render(view: object): void;
  screenRows(): string[];
  dispose(): void;
}

const Host = native.NativeTuiHost as unknown as (new (width: number, height: number, headless: boolean) => V3Host) | undefined;

function available(): boolean {
  if (Host === undefined) return false;
  const host = new Host(1, 1, true);
  try { return host.tuiPerfV3PackedRender !== undefined && host.tuiPerfV3PackedRenderRef !== undefined; } finally { host.dispose(); }
}

function render(encoder: ReturnType<typeof createPackedV3Encoder>, host: V3Host, view: View): void {
  if (host.tuiPerfV3PackedRenderRef === undefined) throw new Error("V3 native transport is unavailable");
  renderPackedV3View(
    encoder,
    view,
    (words, bytes, strings) => {
      if (encoder.stringLane === "strings") {
        if (host.tuiPerfV3PackedRenderStrings === undefined) throw new Error("V3 string-lane transport is unavailable");
        host.tuiPerfV3PackedRenderStrings(words, strings);
      } else {
        if (host.tuiPerfV3PackedRender === undefined) throw new Error("V3 byte-lane transport is unavailable");
        host.tuiPerfV3PackedRender(words, bytes);
      }
    },
    (generation, reference) => host.tuiPerfV3PackedRenderRef!(generation, reference),
  );
}

describe("PERF-8 retained packed V3 smokes", () => {
  test("persistent sequence preserves logical results and shares unchanged roots", () => {
    const original = PersistentSeq.from(Array.from({ length: 10_000 }, (_, index) => index));
    const changed = original.set(8_721, -1);
    expect(original.get(8_721)).toBe(8_721);
    expect(changed.get(8_721)).toBe(-1);
    expect(changed.get(8_720)).toBe(8_720);
    expect(changed.length).toBe(original.length);
    expect(changed.height).toBe(original.height);
    expect(changed.toArray().slice(8_719, 8_723)).toEqual([8_719, 8_720, -1, 8_722]);
  });

  test("persistent sequence supports boundary edits without changing old values", () => {
    for (const size of [0, 1, 31, 32, 33, 1_024, 1_025, 10_000, 100_000]) {
      const original = PersistentSeq.from(Array.from({ length: size }, (_, index) => index));
      const appended = original.append(size);
      expect(appended.length).toBe(size + 1);
      expect(appended.get(size)).toBe(size);
      if (size > 0) {
        const inserted = original.insert(Math.floor(size / 2), -1);
        expect(inserted.length).toBe(size + 1);
        expect(inserted.get(Math.floor(size / 2))).toBe(-1);
        const removed = inserted.remove(Math.floor(size / 2));
        expect(removed.toArray()).toEqual(original.toArray());
      }
      const [left, right] = appended.split(Math.floor(appended.length / 2));
      expect(left.concat(right).toArray()).toEqual(appended.toArray());
    }
  });

  test("V3 full definitions and exact-ref submissions match direct rendering", () => {
    if (!available()) return;
    const direct = new Host!(80, 12, true);
    const packed = new Host!(80, 12, true);
    const encoder = createPackedV3Encoder();
    const view = View.vertical([
      View.text("hello").foreground("cyan"),
      View.text("world").padding(1).maxWidth(20),
      View.horizontal([View.text("left"), View.text("right")]),
      View.hanging(View.text("> "), View.text("  "), View.text("body")),
      new DiffRenderer().render([new DiffHunk(new DiffRange(0, 1), new DiffRange(0, 1), [DiffLine.context(1, 1, "same")])]),
      View.text("clamped").clampRows(1, { kind: "footer", prefix: "more", style: Style.new() }),
      View.contentMax(1, View.text("max")),
    ]);
    try {
      direct.render(nodeForBridge(view));
      render(encoder, packed, view);
      expect(packed.screenRows()).toEqual(direct.screenRows());
      const first = packed.screenRows();
      render(encoder, packed, view);
      expect(packed.screenRows()).toEqual(first);
      expect(packedV3Snapshot().packed_v3_exact_ref_fast_hits).toBeGreaterThan(0);
    } finally {
      direct.dispose();
      packed.dispose();
    }
  });

  test("V3 move-once string lane matches byte-lane parity", () => {
    if (!available()) return;
    const direct = new Host!(40, 6, true);
    const packed = new Host!(40, 6, true);
    const encoder = createPackedV3Encoder("strings");
    const view = View.vertical([
      View.text("héllo 🌍").foreground("cyan"),
      View.text("styled").padding(1).border({ style: "rounded", edges: "all" }),
    ]);
    try {
      direct.render(nodeForBridge(view));
      render(encoder, packed, view);
      expect(packed.screenRows()).toEqual(direct.screenRows());
    } finally {
      direct.dispose();
      packed.dispose();
    }
  });

  test("V3 preserves NodeIds across the full safe-integer range", () => {
    if (!available()) return;
    const host = new Host!(40, 4, true);
    const encoder = createPackedV3Encoder();
    try {
      for (const id of [2 ** 32 - 1, 2 ** 32, 2 ** 32 + 1, Number.MAX_SAFE_INTEGER]) {
        const node = {
          id,
          schema: 1,
          kind: 1,
          spans: [{ text: `v3-wide-${id}` }],
          wrap: 3,
          align: 1,
        } as never;
        const transaction = encoder.encodeRoots([node]);
        host.tuiPerfV3PackedRender!(transaction.words, transaction.bytes);
        expect(host.screenRows().some((row) => row.includes(`v3-wide-${id}`))).toBe(true);
      }
    } finally {
      host.dispose();
    }
  });

  test("V3 component references retain native handle identity", () => {
    if (!available()) return;
    const direct = new Host!(20, 4, true) as V3Host & { createViewSlot(initial: object): { componentId(): number | null } };
    const packed = new Host!(20, 4, true) as V3Host & { createViewSlot(initial: object): { componentId(): number | null } };
    const encoder = createPackedV3Encoder();
    try {
      const directId = direct.createViewSlot(nodeForBridge(View.spacer(0))).componentId();
      const packedId = packed.createViewSlot(nodeForBridge(View.spacer(0))).componentId();
      if (directId === null || packedId === null) throw new Error("component registration failed");
      const directView = View.component({ id: directId as never });
      const packedView = View.component({ id: packedId as never });
      direct.render(nodeForBridge(directView));
      render(encoder, packed, packedView);
      expect(packed.screenRows()).toEqual(direct.screenRows());
    } finally {
      direct.dispose();
      packed.dispose();
    }
  });

  test("V3 wide axis replacement retains sequence structure", () => {
    if (!available()) return;
    const host = new Host!(80, 2_050, true);
    const direct = new Host!(80, 2_050, true);
    const encoder = createPackedV3Encoder();
    const baseItems = Array.from({ length: 2_048 }, (_, index) => View.text(`child-${index}`));
    const base = View.vertical(baseItems);
    const changed = replacePackedAxisChild(base, 1, View.text("changed"));
    const inserted = splicePackedAxisChildren(base, 1_000, 0, [View.text("inserted")]);
    const removed = splicePackedAxisChildren(inserted, 1_000, 1, []);
    try {
      const coldRetriesBefore = packedV3Snapshot().packed_v3_cold_retries;
      render(encoder, host, base);
      resetPackedV3Counters();
      render(encoder, host, changed);
      expect(packedV3Snapshot().packed_v3_cold_retries).toBe(coldRetriesBefore);
      direct.render(nodeForDirectBridge(changed));
      expect(host.screenRows()).toEqual(direct.screenRows());
      const afterEdit = packedV3Snapshot();
      expect(afterEdit.packed_v3_seq_branch_defs).toBeGreaterThan(0);
      expect(afterEdit.packed_v3_persistent_refs).toBeGreaterThan(0);
      // The changed leaf has at most B=32 child slots; work is bounded by
      // the fixed branch factor plus the changed parent, not total width.
      expect(afterEdit.packed_v3_compile_objects_visited).toBeLessThanOrEqual(34);
      expect(afterEdit.packed_v3_patch_view_defs).toBeGreaterThan(0);
      render(encoder, host, inserted);
      direct.render(nodeForDirectBridge(inserted));
      expect(host.screenRows()).toEqual(direct.screenRows());
      render(encoder, host, removed);
      direct.render(nodeForDirectBridge(removed));
      expect(host.screenRows()).toEqual(direct.screenRows());
    } finally {
      host.dispose();
      direct.dispose();
    }
  });

  test("V3 grid cell sequence retains unchanged cells", () => {
    if (!available()) return;
    const direct = new Host!(80, 40, true);
    const packed = new Host!(80, 40, true);
    const encoder = createPackedV3Encoder();
    const base = View.grid((grid) => {
      grid.columns([{ kind: "fixed", size: 20 }, { kind: "flex" }]);
      for (let row = 0; row < 64; row += 1) {
        grid.row((cells) => {
          if (row === 0) {
            cells.cellWith({ columnSpan: 2 }, View.text(`grid-${row}-span`));
          } else {
            cells.cell(View.text(`grid-${row}-a`));
            cells.cell(View.text(`grid-${row}-b`));
          }
        });
      }
    });
    const changed = replacePackedGridCell(base, 31, 0, View.text("grid-changed"));
    try {
      render(encoder, packed, base);
      resetPackedV3Counters();
      render(encoder, packed, changed);
      direct.render(nodeForDirectBridge(changed));
      expect(packed.screenRows()).toEqual(direct.screenRows());
      const afterEdit = packedV3Snapshot();
      expect(afterEdit.packed_v3_seq_branch_defs).toBeGreaterThan(0);
      expect(afterEdit.packed_v3_patch_view_defs).toBeGreaterThan(0);
      expect(afterEdit.packed_v3_compile_objects_visited).toBeLessThanOrEqual(34);
    } finally {
      direct.dispose();
      packed.dispose();
    }
  });

  test("V3 preserves fused text lineage through a published base", () => {
    if (!available()) return;
    const direct = new Host!(40, 4, true);
    const packed = new Host!(40, 4, true);
    const encoder = createPackedV3Encoder();
    const base = View.text("lineage");
    const first = base.noWrap();
    const second = first.textAlign("center");
    try {
      render(encoder, packed, base);
      render(encoder, packed, second);
      direct.render(nodeForDirectBridge(second));
      expect(packed.screenRows()).toEqual(direct.screenRows());
      expect(packedV3Snapshot().packed_v3_patch_view_defs).toBeGreaterThan(0);
    } finally {
      direct.dispose();
      packed.dispose();
    }
  });

  test("V3 text and decoration lineage emits valid patches", () => {
    if (!available()) return;
    const direct = new Host!(80, 8, true);
    const packed = new Host!(80, 8, true);
    const encoder = createPackedV3Encoder();
    const base = View.text("patch me").padding(1);
    const maxChanged = base.maxWidth(20);
    const changed = maxChanged.noWrap();
    try {
      render(encoder, packed, base);
      render(encoder, packed, maxChanged);
      direct.render(nodeForBridge(changed));
      render(encoder, packed, changed);
      expect(packed.screenRows()).toEqual(direct.screenRows());
      const counters = packedV3Snapshot();
      expect(counters.packed_v3_patch_view_defs).toBeGreaterThan(0);
    } finally {
      direct.dispose();
      packed.dispose();
    }
  });

  test("malformed V3 transactions fail before host mutation", () => {
    if (!available()) return;
    const host = new Host!(40, 4, true);
    const encoder = createPackedV3Encoder();
    const view = View.text("before");
    try {
      host.render(nodeForBridge(view));
      const before = host.screenRows();
      const transaction = encoder.encodeRoots([nodeForBridge(View.text("after"))]);
      transaction.words[0] = 0;
      expect(() => host.tuiPerfV3PackedRender!(transaction.words, transaction.bytes)).toThrow();
      expect(host.screenRows()).toEqual(before);
    } finally {
      host.dispose();
    }
  });

  test("V3 weak slot expiry performs one generation resynchronization", () => {
    if (!available()) return;
    const host = new Host!(40, 4, true);
    const encoder = createPackedV3Encoder();
    const view = View.text("resync");
    const reset = native.tuiPerfV3ResetViewBridgeCache;
    if (reset === undefined) { host.dispose(); return; }
    try {
      render(encoder, host, view);
      reset();
      render(encoder, host, view);
      expect(host.screenRows().some((row) => row.includes("resync"))).toBe(true);
      expect(packedV3Snapshot().packed_v3_cold_retries).toBeGreaterThan(0);
    } finally {
      host.dispose();
    }
  });

  test("V3 retries one cache miss and then hard-fails", () => {
    const encoder = createPackedV3Encoder();
    const view = View.text("v3-retry");
    let calls = 0;
    renderPackedV3View(
      encoder,
      view,
      () => {
        calls += 1;
        if (calls === 1) throw Object.assign(new Error("ION_PACKED_CACHE_MISS"), { code: "ION_PACKED_CACHE_MISS" });
      },
      () => { throw Object.assign(new Error("ION_PACKED_CACHE_MISS"), { code: "ION_PACKED_CACHE_MISS" }); },
    );
    expect(calls).toBe(2);
    const broken = createPackedV3Encoder();
    expect(() => renderPackedV3View(
      broken,
      view,
      () => { throw Object.assign(new Error("ION_PACKED_CACHE_MISS"), { code: "ION_PACKED_CACHE_MISS" }); },
      () => { throw Object.assign(new Error("ION_PACKED_CACHE_MISS"), { code: "ION_PACKED_CACHE_MISS" }); },
    )).toThrow("ION_PACKED_CACHE_MISS");
  });
});
