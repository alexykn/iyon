import { createHash } from "node:crypto";
import { access, chmod, copyFile, mkdir, readdir, readFile, rename, rm, writeFile } from "node:fs/promises";
import { homedir } from "node:os";
import { dirname, join, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

const APP_ROOT = fileURLToPath(new URL("../../", import.meta.url));
const TUI_REPOSITORY = "https://github.com/alexykn/iyon-tui.git";
const CACHE_ROOT = resolve(process.env.XDG_CACHE_HOME ?? join(homedir(), ".cache"));
const WORKTREE_ROOT = resolve(
  process.env.IYON_PERF_WORKTREE_ROOT ?? join(CACHE_ROOT, "iyon", "tui-branches"),
);
const APP_ID = createHash("sha256").update(APP_ROOT).digest("hex").slice(0, 12);
const TUI_PACKAGE_PATTERN = /github:alexykn\/iyon-tui#[0-9a-f]+/g;
const TUI_CARGO_PATTERN = /(git\s*=\s*"https:\/\/github\.com\/alexykn\/iyon-tui\.git"\s*,\s*)(?:rev|branch)\s*=\s*"[^"]+"/g;

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

function runChecked(command: string[], cwd: string, printOutput = true): CommandResult {
  const result = run(command, cwd);
  if (printOutput && result.stdout.length > 0) process.stdout.write(result.stdout);
  if (printOutput && result.stderr.length > 0) process.stderr.write(result.stderr);
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

function branchSlug(branch: string): string {
  return branch.replace(/[^A-Za-z0-9._-]+/g, "-").replace(/^-+|-+$/g, "").slice(0, 48) || "branch";
}

function worktreePath(branch: string): string {
  const branchId = createHash("sha256").update(`${APP_ROOT}\0${branch}`).digest("hex").slice(0, 12);
  return join(WORKTREE_ROOT, `app-${APP_ID}-${branchSlug(branch)}-${branchId}`);
}

function isManagedWorktree(path: string): boolean {
  return path.startsWith(`${WORKTREE_ROOT}${sep}`) && path.includes(`${sep}app-${APP_ID}-`);
}

function registeredWorktreePaths(): string[] {
  return runChecked(["git", "worktree", "list", "--porcelain"], APP_ROOT, false).stdout
    .split(/\r?\n/)
    .filter((line) => line.startsWith("worktree "))
    .map((line) => line.slice("worktree ".length));
}

function validateBranch(branch: string): void {
  if (branch.length === 0 || branch.startsWith("-")) {
    throw new Error(`invalid TUI branch: ${JSON.stringify(branch)}`);
  }
  const result = run(["git", "check-ref-format", "--branch", branch], APP_ROOT);
  if (result.exitCode !== 0) {
    throw new Error(`invalid TUI branch ${JSON.stringify(branch)}: ${result.stderr.trim()}`);
  }
}

function resolveBranchSha(branch: string): string {
  validateBranch(branch);
  const result = run(["git", "ls-remote", TUI_REPOSITORY, `refs/heads/${branch}`], APP_ROOT);
  if (result.exitCode !== 0) {
    throw new Error(`unable to resolve ${TUI_REPOSITORY}#${branch}:\n${result.stderr}`);
  }
  const [sha, ref] = result.stdout.trim().split(/\s+/);
  if (!/^[0-9a-f]{40}$/.test(sha ?? "") || ref !== `refs/heads/${branch}`) {
    throw new Error(`remote branch not found: ${TUI_REPOSITORY}#${branch}`);
  }
  return sha;
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
    throw new Error("no @iyon/tui Git dependency was found in the branch worktree");
  }

  const cargoManifest = join(worktree, "Cargo.toml");
  const cargoSource = await readFile(cargoManifest, "utf8");
  const cargoUpdated = cargoSource.replace(TUI_CARGO_PATTERN, `$1rev = "${tuiSha}"`);
  if (cargoUpdated === cargoSource) {
    throw new Error("the iyon-tui Cargo dependency was not found in Cargo.toml");
  }
  await writeFile(cargoManifest, cargoUpdated);
}

async function ensurePersistentWorktree(branch: string, appHead: string): Promise<{ path: string; state: "created" | "reused" }> {
  const path = worktreePath(branch);
  await mkdir(WORKTREE_ROOT, { recursive: true });
  let registered = registeredWorktreePaths().includes(path);

  if (registered && !(await pathExists(path))) {
    runChecked(["git", "worktree", "prune"], APP_ROOT, false);
    registered = false;
  }
  if (registered) {
    runChecked(["git", "reset", "--hard", appHead], path, false);
    return { path, state: "reused" };
  }
  if (await pathExists(path)) {
    throw new Error(`cache path exists but is not registered as an app worktree: ${path}`);
  }

  runChecked(["git", "worktree", "add", "--detach", path, appHead], APP_ROOT);
  return { path, state: "created" };
}

async function buildStable(): Promise<void> {
  runChecked(["bun", "run", "native:stage"], APP_ROOT);
  runChecked(["bun", "run", "native:tui:stage"], APP_ROOT);
  runChecked(["bun", "run", "packages/iyon-cli/build.ts"], APP_ROOT);
}

async function buildBranch(branch: string): Promise<void> {
  const status = runChecked(["git", "status", "--porcelain"], APP_ROOT, false);
  if (status.stdout.trim().length > 0) {
    throw new Error("branch builds require a clean application checkout");
  }

  const appHead = runChecked(["git", "rev-parse", "HEAD"], APP_ROOT, false).stdout.trim();
  const tuiSha = resolveBranchSha(branch);
  const worktree = await ensurePersistentWorktree(branch, appHead);
  const output = join(APP_ROOT, "dist", "iyon");
  const temporaryOutput = `${output}.tui-${branchSlug(branch)}.tmp`;

  try {
    console.log(
      `${worktree.state} persistent worktree ${relative(APP_ROOT, worktree.path)}; building against iyon-tui/${branch} @ ${tuiSha}`,
    );
    await switchTuiDependencies(worktree.path, tuiSha);
    runChecked(["bun", "install"], worktree.path);
    runChecked(["cargo", "update", "-p", "iyon-tui"], worktree.path);
    runChecked(["bun", "run", "build:iyon", "--", branch], worktree.path);

    const builtOutput = join(worktree.path, "dist", "iyon");
    await mkdir(dirname(output), { recursive: true });
    await copyFile(builtOutput, temporaryOutput);
    await chmod(temporaryOutput, 0o755);
    await rename(temporaryOutput, output);
    console.log(`built ${output} against iyon-tui/${branch} @ ${tuiSha}`);
  } finally {
    await rm(temporaryOutput, { force: true });
  }
}

async function cleanBranch(branch: string): Promise<void> {
  validateBranch(branch);
  const path = worktreePath(branch);
  const registered = registeredWorktreePaths().includes(path);
  if (registered) {
    runChecked(["git", "worktree", "remove", "--force", path], APP_ROOT);
  } else if (await pathExists(path)) {
    await rm(path, { recursive: true, force: true });
  } else {
    console.log(`no cached worktree for iyon-tui/${branch}`);
    return;
  }
  runChecked(["git", "worktree", "prune"], APP_ROOT, false);
  console.log(`removed cached worktree for iyon-tui/${branch}`);
}

async function cleanAll(): Promise<void> {
  const registered = registeredWorktreePaths().filter(isManagedWorktree);
  for (const path of registered) {
    runChecked(["git", "worktree", "remove", "--force", path], APP_ROOT);
  }

  if (await pathExists(WORKTREE_ROOT)) {
    const entries = await readdir(WORKTREE_ROOT, { withFileTypes: true });
    const prefix = `app-${APP_ID}-`;
    for (const entry of entries) {
      if (!entry.name.startsWith(prefix)) continue;
      await rm(join(WORKTREE_ROOT, entry.name), { recursive: true, force: true });
    }
  }
  runChecked(["git", "worktree", "prune"], APP_ROOT, false);
  console.log(`removed all cached TUI branch worktrees for this app checkout`);
}

function argumentsForCommand(): string[] {
  return Bun.argv.slice(2).filter((argument) => argument !== "--");
}

async function main(): Promise<void> {
  const args = argumentsForCommand();
  if (args.length === 0) {
    await buildStable();
    return;
  }
  if (args[0] === "--clean") {
    if (args.length !== 2) throw new Error("usage: bun run clean:iyon -- <branch> | bun run clean:iyon -- --all");
    if (args[1] === "--all") {
      await cleanAll();
      return;
    }
    await cleanBranch(args[1]);
    return;
  }
  if (args.length !== 1) throw new Error("usage: bun run build:iyon -- <tui-branch>");
  await buildBranch(args[0]);
}

await main();
