import { describe, expect, test } from "bun:test";
import { MemoryCredentialStore } from "../../../../packages/iyon-runtime/src/credentials.ts";
import { resolveApiKey, status, CREDENTIAL_ACCOUNT, CREDENTIAL_SERVICE } from "../src/auth.ts";

describe("OpenRouter auth", () => {
  test("ignores empty environment values and uses the generic store", async () => {
    const previous = process.env.OPENROUTER_API_KEY;
    process.env.OPENROUTER_API_KEY = "   ";
    const credentials = new MemoryCredentialStore();
    await credentials.set(CREDENTIAL_SERVICE, CREDENTIAL_ACCOUNT, "stored-key");
    expect(await resolveApiKey({ credentials })).toBe("stored-key");
    expect((await status({ credentials })).authenticated).toBe(true);
    if (previous === undefined) delete process.env.OPENROUTER_API_KEY; else process.env.OPENROUTER_API_KEY = previous;
  });
});
