import { createHash } from "node:crypto";
import { access, chmod, copyFile, mkdir, readdir, readFile, rename, rm, writeFile } from "node:fs/promises";
import { homedir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const APP_ROOT = fileURLToPath(new URL("../../", import.meta.url));
const TUI_REPOSITORY = "https://github.com/alexykn/iyon-tui.git";
const TUI_BRANCH = "perf-refactor";
const TUI_PACKAGE_PATTERN = /github:alexykn\/iyon-tui#[0-9a-f]+/g;
const TUI_CARGO_PATTERN = /(git\s*=\s*"https:\/\/github\.com\/alexykn\/iyon-tui\.git"\s*,\s*)(?:rev|branch)\s*=\s*"[^"]+"/g;
const CACHE_ROOT = process.env.XDG_CACHE_HOME ?? join(homedir(), ".cache");
const WORKTREE_ID = createHash("sha256").update(APP_ROOT).digest("hex").slice(0, 12);
const PERSISTENT_WORKTREE = resolve(
  process.env.IYON_PERF_WORKTREE ?? join(CACHE_ROOT, "iyon", `perf-refactor-${WORKTREE_ID}`),
);

interface CommandResult {
  exitCode: number;
  stdout: string;
  stderr: string;
}

function decode(value: Uint8Array | undefined): string {
  return value === undefined ? "" : new TextDecoder().decode(value);
}

function run(command: string[], cwd: string): CommandResult {
  const result = Bun.spawnSync({
    cmd: command,
    cwd,
    stdout: "pipe",
    stderr: "pipe",
  });
  return {
    exitCode: result.exitCode,
    stdout: decode(result.stdout),
    stderr: decode(result.stderr),
  };
}

function runChecked(command: string[], cwd: string): CommandResult {
  const result = run(command, cwd);
  if (result.stdout.length > 0) process.stdout.write(result.stdout);
  if (result.stderr.length > 0) process.stderr.write(result.stderr);
  if (result.exitCode !== 0) {
    throw new Error(`command failed (${result.exitCode}): ${command.join(" ")}`);
  }
  return result;
}

async function pathExists(path: string): Promise<boolean> {
  try {
    await access(path);
    return true;
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") return false;
    throw error;
  }
}

async function packageJsonFiles(directory: string): Promise<string[]> {
  const files: string[] = [];
  const entries = await readdir(directory, { withFileTypes: true });
  for (const entry of entries) {
    if (entry.name === ".git" || entry.name === "node_modules" || entry.name === "target" || entry.name === "dist") continue;
    const path = join(directory, entry.name);
    if (entry.isDirectory()) {
      files.push(...(await packageJsonFiles(path)));
      continue;
    }
    if (entry.isFile() && entry.name === "package.json") files.push(path);
  }
  return files;
}

function resolvePerfRefactorSha(): string {
  const result = run(["git", "ls-remote", TUI_REPOSITORY, `refs/heads/${TUI_BRANCH}`], APP_ROOT);
  if (result.exitCode !== 0) {
    throw new Error(`unable to resolve ${TUI_REPOSITORY}#${TUI_BRANCH}:\n${result.stderr}`);
  }
  const [sha, ref] = result.stdout.trim().split(/\s+/);
  if (!/^[0-9a-f]{40}$/.test(sha ?? "") || ref !== `refs/heads/${TUI_BRANCH}`) {
    throw new Error(`remote did not return a valid ${TUI_BRANCH} branch head`);
  }
  return sha;
}

async function ensurePersistentWorktree(appHead: string): Promise<"created" | "reused"> {
  await mkdir(dirname(PERSISTENT_WORKTREE), { recursive: true });
  let registered = runChecked(["git", "worktree", "list", "--porcelain"], APP_ROOT).stdout
    .split(/\r?\n/)
    .some((line) => line === `worktree ${PERSISTENT_WORKTREE}`);

  if (registered && !(await pathExists(PERSISTENT_WORKTREE))) {
    runChecked(["git", "worktree", "prune"], APP_ROOT);
    registered = false;
  }
  if (registered) {
    runChecked(["git", "reset", "--hard", appHead], PERSISTENT_WORKTREE);
    return "reused";
  }
  if (await pathExists(PERSISTENT_WORKTREE)) {
    throw new Error(
      `persistent worktree path exists but is not registered with this app checkout: ${PERSISTENT_WORKTREE}`,
    );
  }

  runChecked(["git", "worktree", "add", "--detach", PERSISTENT_WORKTREE, appHead], APP_ROOT);
  return "created";
}

async function switchTuiDependencies(worktree: string, tuiSha: string): Promise<void> {
  const manifests = await packageJsonFiles(worktree);
  let packageReplacements = 0;
  for (const manifest of manifests) {
    const source = await readFile(manifest, "utf8");
    const updated = source.replace(TUI_PACKAGE_PATTERN, `github:alexykn/iyon-tui#${tuiSha}`);
    if (updated === source) continue;
    await writeFile(manifest, updated);
    packageReplacements += 1;
  }
  if (packageReplacements === 0) {
    throw new Error("no @iyon/tui Git dependency was found in the temporary application worktree");
  }

  const cargoManifest = join(worktree, "Cargo.toml");
  const cargoSource = await readFile(cargoManifest, "utf8");
  const cargoUpdated = cargoSource.replace(TUI_CARGO_PATTERN, `$1rev = "${tuiSha}"`);
  if (cargoUpdated === cargoSource) {
    throw new Error("the iyon-tui Cargo dependency was not found in Cargo.toml");
  }
  await writeFile(cargoManifest, cargoUpdated);
}

async function main(): Promise<void> {
  const status = runChecked(["git", "status", "--porcelain"], APP_ROOT);
  if (status.stdout.trim().length > 0) {
    throw new Error("build:iyon:perf-refactor requires a clean application checkout");
  }

  const appHead = runChecked(["git", "rev-parse", "HEAD"], APP_ROOT).stdout.trim();
  const tuiSha = resolvePerfRefactorSha();
  const worktree = PERSISTENT_WORKTREE;
  const output = join(APP_ROOT, "dist", "iyon");
  const temporaryOutput = `${output}.perf-refactor.tmp`;

  try {
    const worktreeState = await ensurePersistentWorktree(appHead);
    console.log(
      `${worktreeState} persistent perf-refactor worktree: ${worktree}; building against iyon-tui/${TUI_BRANCH} @ ${tuiSha}`,
    );
    await switchTuiDependencies(worktree, tuiSha);
    runChecked(["bun", "install"], worktree);
    runChecked(["cargo", "update", "-p", "iyon-tui"], worktree);
    runChecked(["bun", "run", "build:iyon"], worktree);

    const builtOutput = join(worktree, "dist", "iyon");
    await mkdir(dirname(output), { recursive: true });
    await copyFile(builtOutput, temporaryOutput);
    await chmod(temporaryOutput, 0o755);
    await rename(temporaryOutput, output);
    console.log(`built ${output} against iyon-tui/${TUI_BRANCH} @ ${tuiSha}`);
  } finally {
    await rm(temporaryOutput, { force: true });
  }
}

await main();
