import { describe, expect, test } from "bun:test";

import { native } from "../src/native.ts";
import { View, nodeForBridge, nodeForDirectBridge } from "../src/tui/values/view.ts";
import {
  FastSharedTransport,
  createFastSharedEncoder,
  createFastSharedTransport,
  type FastSharedAbi,
  replaceFastSharedAxisChild,
  replaceFastSharedGridCell,
} from "../src/tui/fast_shared.ts";

type Host = {
  tuiPerfFastSharedAbi?: () => FastSharedAbi;
  render(view: object): void;
  screenRows(): string[];
  dispose(): void;
};

const Host = native.NativeTuiHost as unknown as (new (width: number, height: number, headless: boolean) => Host) | undefined;

function available(): boolean {
  if (Host === undefined) return false;
  const host = new Host(1, 1, true);
  try { return host.tuiPerfFastSharedAbi !== undefined; } finally { host.dispose(); }
}

describe("PERF-10 native shared-memory retained path", () => {
  test("bootstraps fixed pages and preserves Unicode rendering", () => {
    if (!available()) return;
    const direct = new Host!(40, 6, true);
    const fast = new Host!(40, 6, true);
    const transport = createFastSharedTransport(fast);
    const encoder = createFastSharedEncoder(transport);
    const view = View.vertical([View.text("héllo 🌍\0"), View.text("same")]);
    try {
      direct.render(nodeForBridge(view));
      encoder.render(view);
      expect(fast.screenRows()).toEqual(direct.screenRows());
      const first = fast.screenRows();
      encoder.render(view);
      expect(fast.screenRows()).toEqual(first);
      expect(transport.abi.pages.length).toBeGreaterThan(1);
      expect(transport.abi.op_words).toBe(10);
    } finally {
      transport.close();
      direct.dispose();
      fast.dispose();
    }
  });

  test("shares one environment runtime without cross-host FastShared ref collisions", () => {
    if (!available()) return;
    const first = new Host!(30, 4, true);
    const second = new Host!(30, 4, true);
    const firstTransport = createFastSharedTransport(first);
    const secondTransport = createFastSharedTransport(second);
    const firstEncoder = createFastSharedEncoder(firstTransport);
    const secondEncoder = createFastSharedEncoder(secondTransport);
    const firstView = View.text("first-host");
    const secondView = View.text("second-host");
    try {
      firstEncoder.render(firstView);
      secondEncoder.render(secondView);
      firstTransport.renderRef(firstEncoder.generation, 1);
      secondTransport.renderRef(secondEncoder.generation, 1);
      expect(first.screenRows().join("\n")).toContain("first-host");
      expect(second.screenRows().join("\n")).toContain("second-host");
    } finally {
      firstTransport.close();
      secondTransport.close();
      first.dispose();
      second.dispose();
    }
  });

  test("applies retained axis and grid path copies without flattening", () => {
    if (!available()) return;
    const direct = new Host!(60, 12, true);
    const fast = new Host!(60, 12, true);
    const transport = createFastSharedTransport(fast);
    const encoder = createFastSharedEncoder(transport);
    const axis = View.vertical(Array.from({ length: 2_048 }, (_, index) => View.text(`child-${index}`)));
    const axisChanged = replaceFastSharedAxisChild(axis, 1_337, View.text("changed"));
    const grid = View.grid((builder) => {
      builder.columns([{ kind: "fixed", size: 12 }, { kind: "flex" }]);
      for (let row = 0; row < 64; row += 1) builder.row((cells) => { cells.cell(View.text(`a-${row}`)); cells.cell(View.text(`b-${row}`)); });
    });
    const gridChanged = replaceFastSharedGridCell(grid, 31, 0, View.text("grid-changed"));
    try {
      for (const [expected, actual] of [[axis, axis], [axisChanged, axisChanged], [grid, grid], [gridChanged, gridChanged]] as const) {
        direct.render(nodeForDirectBridge(expected));
        encoder.render(actual);
        expect(fast.screenRows()).toEqual(direct.screenRows());
      }
    } finally {
      transport.close();
      direct.dispose();
      fast.dispose();
    }
  });

  test("rejects an ABI mismatch and malformed command before host mutation", () => {
    if (!available()) return;
    const host = new Host!(30, 4, true);
    const abi = host.tuiPerfFastSharedAbi!();
    expect(() => new FastSharedTransport({ ...abi, op_words: 11 })).toThrow("ION_FAST_SHARED_ABI_MISMATCH");
    const transport = createFastSharedTransport(host);
    try {
      host.render(nodeForBridge(View.text("stable")));
      const before = host.screenRows();
      transport.begin(0);
      transport.control[0] = 0;
      expect(() => transport.commit(0)).toThrow();
      expect(host.screenRows()).toEqual(before);
    } finally {
      transport.close();
      host.dispose();
    }
  });

  test("rejects an unpaired surrogate before host mutation", () => {
    if (!available()) return;
    const host = new Host!(30, 4, true);
    const transport = createFastSharedTransport(host);
    const encoder = createFastSharedEncoder(transport);
    try {
      host.render(nodeForBridge(View.text("stable")));
      const before = host.screenRows();
      expect(() => encoder.render(View.text("bad\uD800"))).toThrow();
      expect(host.screenRows()).toEqual(before);
    } finally {
      transport.close();
      host.dispose();
    }
  });

  test("rejects invalid opcode and forward local references atomically", () => {
    if (!available()) return;
    const host = new Host!(30, 4, true);
    const transport = createFastSharedTransport(host);
    try {
      host.render(nodeForBridge(View.text("stable")));
      const before = host.screenRows();
      transport.begin(0);
      transport.emit(0xffff, 1, 0, 1);
      expect(() => transport.commit(0)).toThrow();
      expect(host.screenRows()).toEqual(before);
      transport.begin(0);
      transport.emit(7, 0, 0, 1, 0x8000_0001);
      expect(() => transport.commit(0)).toThrow();
      expect(host.screenRows()).toEqual(before);
    } finally {
      transport.close();
      host.dispose();
    }
  });

  test("rejects duplicate destination references atomically", () => {
    if (!available()) return;
    const host = new Host!(30, 4, true);
    const transport = createFastSharedTransport(host);
    try {
      host.render(nodeForBridge(View.text("stable")));
      const before = host.screenRows();
      transport.begin(0);
      transport.emit(3, 1, 0, 1, 0);
      transport.emit(3, 1, 0, 2, 0);
      expect(() => transport.commit(0)).toThrow();
      expect(host.screenRows()).toEqual(before);
    } finally {
      transport.close();
      host.dispose();
    }
  });

  test("rejects use after host disposal", () => {
    if (!available()) return;
    const host = new Host!(30, 4, true);
    const transport = createFastSharedTransport(host);
    host.dispose();
    try {
      expect(() => transport.renderRef(0, 1)).toThrow();
    } finally {
      transport.close();
    }
  });

  test("rejects a string larger than one native page before host mutation", () => {
    if (!available()) return;
    const host = new Host!(30, 4, true);
    const transport = createFastSharedTransport(host);
    const encoder = createFastSharedEncoder(transport);
    try {
      host.render(nodeForBridge(View.text("stable")));
      const before = host.screenRows();
      expect(() => encoder.render(View.text("x".repeat(70_000)))).toThrow();
      expect(host.screenRows()).toEqual(before);
    } finally {
      transport.close();
      host.dispose();
    }
  });

  test("matches direct rendering across a deterministic retained mutation trace", () => {
    if (!available()) return;
    const direct = new Host!(60, 12, true);
    const fast = new Host!(60, 12, true);
    const transport = createFastSharedTransport(fast);
    const encoder = createFastSharedEncoder(transport);
    let state = View.vertical(Array.from({ length: 64 }, (_, index) => View.text(`item-${index}`)));
    let seed = 0x9e37_79b9;
    try {
      direct.render(nodeForDirectBridge(state));
      encoder.render(state);
      for (let step = 0; step < 64; step += 1) {
        seed = (Math.imul(seed, 1_664_525) + 1_013_904_223) >>> 0;
        const index = seed % 64;
        state = replaceFastSharedAxisChild(state, index, View.text(`changed-${step}`));
        direct.render(nodeForDirectBridge(state));
        encoder.render(state);
        expect(fast.screenRows()).toEqual(direct.screenRows());
      }
    } finally {
      transport.close();
      direct.dispose();
      fast.dispose();
    }
  });
});
