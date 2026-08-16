import { describe, expect, test } from "bun:test";

import {
  CancellationProbe,
  EventQueueProbe,
  asyncSleep,
  echoBuffer,
  echoJson,
  echoString,
  nativeVersion,
} from "../src/native.ts";
import { runWithAbortSignal } from "../src/smoke.ts";

describe("T1 native boundary", () => {
  test("JSON, large strings, and Buffer values remain lossless", () => {
    const value = { nested: [null, true, 42, "text"] };
    expect(echoJson(value)).toEqual(value);
    const large = "x".repeat(1024 * 1024);
    expect(echoString(large)).toBe(large);
    expect([...echoBuffer(Buffer.from([0, 1, 2, 255]))]).toEqual([0, 1, 2, 255]);
  });

  test("Promise success and failure are distinct", async () => {
    expect(await asyncSleep(0)).toBe("slept");
    await expect(asyncSleep(0xffffffff)).rejects.toThrow(/invalid input/);
  });

  test("AbortSignal calls explicit native cancel", async () => {
    const controller = new AbortController();
    const probe = new CancellationProbe();
    const operation = runWithAbortSignal(controller.signal, {
      run: () => probe.run(10_000),
      cancel: () => probe.cancel(),
    });
    controller.abort();
    await expect(operation).rejects.toThrow(/cancelled/);
  });

  test("idle event receiver closes cleanly", async () => {
    const queue = new EventQueueProbe();
    const waiting = queue.nextEvent();
    queue.close();
    expect(await waiting).toBeNull();
  });

  test("native version is a real addon marker", () => {
    expect(nativeVersion()).toBe("iyon-native/t1");
  });
});
