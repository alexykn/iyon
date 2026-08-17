import { describe, expect, test } from "bun:test";
import { runSelectedApp } from "../src/runner.ts";

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
});
