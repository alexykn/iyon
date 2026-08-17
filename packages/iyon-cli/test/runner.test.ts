import { describe, expect, test } from "bun:test";
import { requestProcessSignal, runSelectedApp } from "../src/runner.ts";

describe("selected runner", () => {
  test("runs app and agent through the shared lifecycle", async () => {
    const calls: string[] = []; const app = { start: async () => { calls.push("start"); }, stop: async () => { calls.push("stop"); }, dispatch: () => { calls.push("event"); } }; const agent = { run: async () => { calls.push("agent"); } };
    const session = { nextEvent: async () => null, close: () => {} } as never;
    await runSelectedApp({ app, agent, session }); expect(calls).toEqual(["start", "stop"]);
  });

  test("ctrl_c_exit_shows_goodbye_and_returns_success", async () => {
    let goodbye = false;
    const app = {
      run: async () => { goodbye = true; },
      start: async () => {},
      stop: async () => { throw new Error("terminal already restored"); },
    };
    const session = { nextEvent: async () => null, close: () => {} } as never;

    await expect(runSelectedApp({ app, agent: { run: async () => {} }, session })).resolves.toBeUndefined();
    expect(goodbye).toBe(true);
  });

  test("idle_process_signal_routes_ctrl_c_and_stops_once", async () => {
    const calls: string[] = [];
    let releaseRun: (() => void) | undefined;
    const app = {
      start: async () => {},
      run: async () => await new Promise<void>((resolve) => { releaseRun = resolve; }),
      handleAction: async (action: unknown) => {
        calls.push((action as { type: string }).type);
        releaseRun?.();
      },
      stop: async () => { calls.push("stop"); },
    };
    const session = { nextEvent: async () => null, close: () => {} } as never;
    const running = runSelectedApp({ app, agent: { run: async () => {} }, session });
    await Promise.resolve();
    await requestProcessSignal();
    await running;
    expect(calls).toEqual(["ctrlC", "stop"]);
  });
});
