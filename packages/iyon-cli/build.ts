import { mkdir } from "node:fs/promises";
import { iyonVirtualModulePlugin } from "@iyon/runtime";

const root = new URL("../../", import.meta.url);
const outputDirectory = new URL("dist/", root);
await mkdir(outputDirectory.pathname, { recursive: true });
const output = new URL("iyon", outputDirectory).pathname;
const result = await Bun.build({
  entrypoints: [new URL("./src/cli-entry.ts", import.meta.url).pathname],
  outdir: outputDirectory.pathname,
  plugins: [iyonVirtualModulePlugin],
  compile: { outfile: output },
});
if (!result.success) throw new Error(`iyon standalone compilation failed:\n${result.logs.map((log) => log.message).join("\n")}`);
console.log(`built ${output}`);
