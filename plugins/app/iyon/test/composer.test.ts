import { describe, expect, test } from "bun:test";
import { ComposerPasteStore, isLargePaste, normalizePaste } from "../src/composer.ts";

describe("composer paste policy", () => {
  test("normalizes line endings and tabs", () => { expect(normalizePaste("a\r\nb\rc\td")).toBe("a\nb\nc    d"); });
  test("uses both large-paste thresholds", () => {
    expect(isLargePaste("x".repeat(1000))).toBe(false);
    expect(isLargePaste("x".repeat(1001))).toBe(true);
    expect(isLargePaste(Array.from({ length: 10 }, () => "x").join("\n"))).toBe(false);
    expect(isLargePaste(Array.from({ length: 11 }, () => "x").join("\n"))).toBe(true);
  });
  test("expands markers and clears stored payloads", () => {
    const store = new ComposerPasteStore(); const payload = "payload\n".repeat(200); const marker = store.displayText("", payload);
    expect(marker).toContain("[Pasted Content"); expect(store.expand(`before ${marker} after`)).toBe(`before ${payload} after`); expect(store.size).toBe(0);
  });
  test("avoids marker collisions", () => {
    const store = new ComposerPasteStore(); const payload = "x".repeat(1001); const base = `[Pasted Content ${payload.length} chars]`;
    expect(store.displayText(base, payload)).toBe(`${base} #1`); expect(store.displayText(`${base} #1`, payload)).toBe(`${base} #2`);
  });
});
