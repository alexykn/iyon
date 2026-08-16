import { describe, expect, test } from "bun:test";

import { installIyonVirtualModules } from "../src/virtual-modules.ts";

installIyonVirtualModules();

const modules = await Promise.all([
  import("iyon:api"),
  import("iyon:core"),
  import("iyon:tui"),
]);
const [api, core, tui] = modules;

describe("T1 named native probes", () => {
  test("async_rust_resolves_to_promise", async () => {
    const pending = core.asyncSleep(0);
    expect(pending).toBeInstanceOf(Promise);
    expect(await pending).toBe("slept");
  });

  test("promise_failure_is_typed", async () => {
    await expect(core.asyncSleep(0xffffffff)).rejects.toThrow(/invalid input/);
  });

  test("explicit_cancellation_stops_work", async () => {
    const probe = new core.CancellationProbe();
    const operation = probe.run(10_000);
    probe.cancel();
    await expect(operation).rejects.toThrow(/cancelled/);
  });

  test("native_counter_finalizer_runs", async () => {
    core.resetNativeCounterStats();
    let counter: InstanceType<typeof core.NativeCounter> | undefined = new core.NativeCounter();
    expect(counter.increment()).toBe(1);
    counter = undefined;
    for (let attempt = 0; attempt < 40; attempt += 1) {
      Bun.gc(true);
      await new Promise((resolve) => setTimeout(resolve, 5));
      if (core.nativeCounterStats().live === 0 && core.nativeCounterStats().finalized > 0) {
        return;
      }
    }
    throw new Error("NativeCounter finalizer did not run");
  });

  test("json_conversion_round_trips", () => {
    const value = { nested: [null, true, 42, "text"] };
    expect(api.echoJson(value)).toEqual(value);
  });

  test("large_string_transfer", () => {
    const value = "x".repeat(1024 * 1024);
    expect(api.echoString(value)).toBe(value);
  });

  test("buffer_transfer", () => {
    expect([...api.echoBuffer(Buffer.from([0, 1, 2, 255]))]).toEqual([0, 1, 2, 255]);
  });

  test("one_hundred_concurrent_futures", async () => {
    const results = await Promise.all(Array.from({ length: 100 }, () => core.asyncSleep(0)));
    expect(results).toHaveLength(100);
  });

  test("tokio_channel_receiver", async () => {
    const queue = new core.EventQueueProbe();
    await queue.send({ id: 1 });
    await queue.send({ id: 2 });
    expect(await queue.nextEvent()).toEqual({ id: 1 });
    expect(await queue.nextEvent()).toEqual({ id: 2 });
    queue.close();
  });

  test("clean_shutdown", async () => {
    const queue = new core.EventQueueProbe();
    const waiting = queue.nextEvent();
    queue.close();
    expect(await waiting).toBeNull();
    expect(tui.tuiSmoke).toBe("iyon:tui/t1");
  });
});
