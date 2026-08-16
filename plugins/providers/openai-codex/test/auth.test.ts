import { describe, expect, test } from "bun:test";
import { accountIdFromAccessToken, parseCallback, pkceChallenge } from "../src/auth.ts";

describe("Codex auth", () => {
  test("validates callback state and computes deterministic PKCE", () => {
    expect(pkceChallenge("verifier")).toBe("iMnq5o6zALKXGivsnlom_0F5_WYda32GHkxlV7mq7hQ");
    expect(parseCallback("http://localhost/auth/callback?code=abc&state=state", "state")).toEqual({ code: "abc" });
    expect(() => parseCallback("http://localhost/wrong?code=abc&state=state", "state")).toThrow(/callback path/);
  });

  test("does not expose malformed JWT account claims", () => {
    expect(accountIdFromAccessToken("not-a-jwt")).toBeUndefined();
  });
});
