import { describe, expect, test } from "bun:test";
import { ProviderRegistry } from "@iyon/plugins";
import { runAuth } from "../src/auth.ts";

describe("CLI auth", () => {
  test("dispatches status through provider auth contributions", async () => {
    const registry = new ProviderRegistry(); registry.register({ id: "mock", auth: { status: async () => ({ provider: "mock", authenticated: false }) } }); const output: string[] = [];
    const result = await runAuth("status", { registry, output: (line) => { output.push(line); } }); expect(result[0]?.status?.authenticated).toBe(false); expect(output).toEqual(["mock: not logged in"]);
  });
});
