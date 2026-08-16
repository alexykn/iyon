import { describe, expect, test } from "bun:test";
import { EventHub } from "../src/events.ts";

describe("extension events", () => {
  test("preserves handler order, supports disposal, and isolates observers", () => {
    const events = new EventHub();
    const seen: string[] = [];
    const first = events.on("activation", () => { seen.push("first"); });
    events.on("activation", () => { throw new Error("observer"); });
    events.on("activation", () => { seen.push("last"); });
    const source = { packageId: "p", extensionId: "e", registrationId: "r", generation: 1, scope: "project" as const, source: { type: "local" as const, path: "/p", descriptor: "local:/p" } };
    events.emit("activation", { packageId: "p", extensionId: "e", source });
    expect(seen).toEqual(["first", "last"]);
    expect(events.errors).toHaveLength(1);
    first.dispose();
    events.emit("activation", { packageId: "p", extensionId: "e", source });
    expect(seen).toEqual(["first", "last", "last"]);
  });
});
