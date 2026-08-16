import { describe, expect, test } from "bun:test";
import { CliArgumentError, parseArgs } from "../src/args.ts";

describe("CLI args", () => {
  test("defaults to run and accepts auth commands", () => { expect(parseArgs([])).toEqual({ type: "run" }); expect(parseArgs(["run"])).toEqual({ type: "run" }); expect(parseArgs(["auth", "status"])).toEqual({ type: "auth", command: "status" }); });
  test("rejects unknown commands", () => { expect(() => parseArgs(["wat"])).toThrow(CliArgumentError); expect(() => parseArgs(["auth"])).toThrow(CliArgumentError); });
});
