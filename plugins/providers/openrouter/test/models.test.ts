import { describe, expect, test } from "bun:test";
import { capabilitiesFromCatalog, parseModelCatalog } from "../src/models.ts";

describe("OpenRouter models", () => {
  test("narrows reasoning candidates from catalog metadata", () => {
    expect(capabilitiesFromCatalog({ reasoning: { supported_efforts: ["low", "medium", "not-a-level"] } }).reasoning).toEqual(["low", "medium"]);
    expect(parseModelCatalog({ data: [{ id: "model", name: "Model", reasoning: { supported_efforts: ["high"] } }] })[0]).toMatchObject({ id: "model", name: "Model" });
  });
});
