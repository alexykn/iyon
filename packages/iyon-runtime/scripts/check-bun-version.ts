import { readFile } from "node:fs/promises";

const packageJson = JSON.parse(await readFile(new URL("../../../package.json", import.meta.url), "utf8")) as {
  packageManager?: string;
  devDependencies?: Record<string, string>;
};
const expectedVersion = (await readFile(new URL("../../../.bun-version", import.meta.url), "utf8")).trim();
const expectedRevision = (await readFile(new URL("../../../tools/bun-revision.txt", import.meta.url), "utf8")).trim();
const errors: string[] = [];
if (Bun.version !== expectedVersion) errors.push(`Bun version ${Bun.version} !== ${expectedVersion}`);
if (packageJson.packageManager !== `bun@${expectedVersion}`) errors.push("packageManager is not pinned to the .bun-version");
if (packageJson.devDependencies?.["bun-types"] !== expectedVersion) errors.push("bun-types is not pinned to the .bun-version");
if (Bun.revision !== expectedRevision) {
  errors.push(`Bun revision ${Bun.revision} !== ${expectedRevision}`);
}
if (errors.length > 0) throw new Error(errors.join("; "));

console.log(JSON.stringify({ version: Bun.version, revision: Bun.revision, expectedRevision }));
