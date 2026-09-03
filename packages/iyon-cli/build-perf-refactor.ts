import { createHash } from "node:crypto";
import { access, chmod, copyFile, mkdir, mkdtemp, readdir, readFile, rename, rm, writeFile } from "node:fs/promises";
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
const LEGACY_WORKTREE = join(CACHE_ROOT, "iyon", `perf-refactor-${APP_ID}`);
const WORKTREE_STATE_FILE = ".iyon-perf-cache.json";
const TUI_PACKAGE_PATTERN = /github:alexykn\/iyon-tui#[^"\s]+/g;
const TUI_CARGO_PATTERN = /(git\s*=\s*"https:\/\/github\.com\/alexykn\/iyon-tui\.git"\s*,\s*)(?:rev|branch|tag)\s*=\s*"[^"]+"/g;

interface CommandResult {
  exitCode: number;
  stdout: string;
  stderr: string;
}

interface PersistentWorktreeState {
  readonly appHead: string;
  readonly tuiSha: string;
  readonly sourceState: string;
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
  return path === LEGACY_WORKTREE ||
    (path.startsWith(`${WORKTREE_ROOT}${sep}`) && path.includes(`${sep}app-${APP_ID}-`));
}

function registeredWorktreePaths(): string[] {
  return runChecked(["git", "worktree", "list", "--porcelain"], APP_ROOT, false).stdout
    .split(/\r?\n/)
    .filter((line) => line.startsWith("worktree "))
    .map((line) => line.slice("worktree ".length));
}

async function sourceStateKey(): Promise<string> {
  const patch = runChecked(["git", "diff", "HEAD", "--binary"], APP_ROOT, false).stdout;
  const untracked = runChecked(["git", "ls-files", "--others", "--exclude-standard"], APP_ROOT, false).stdout
    .split(/\r?\n/)
    .filter(Boolean)
    .sort();
  const hash = createHash("sha256").update(patch);
  for (const path of untracked) {
    hash.update(`\0${path}\0`);
    hash.update(await readFile(join(APP_ROOT, path)));
  }
  return hash.digest("hex");
}

async function readWorktreeState(worktree: string): Promise<PersistentWorktreeState | undefined> {
  const path = join(worktree, WORKTREE_STATE_FILE);
  let source: string;
  try {
    source = await readFile(path, "utf8");
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") return undefined;
    throw error;
  }
  const value = JSON.parse(source) as Partial<PersistentWorktreeState>;
  if (typeof value.appHead !== "string" || typeof value.tuiSha !== "string" || typeof value.sourceState !== "string") {
    throw new Error(`invalid persistent PERF worktree state: ${path}`);
  }
  return value as PersistentWorktreeState;
}

async function writeWorktreeState(worktree: string, state: PersistentWorktreeState): Promise<void> {
  await writeFile(join(worktree, WORKTREE_STATE_FILE), `${JSON.stringify(state)}\n`);
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
  let packageMatches = 0;
  for (const manifest of manifests) {
    const source = await readFile(manifest, "utf8");
    const updated = source.replace(TUI_PACKAGE_PATTERN, () => {
      packageMatches += 1;
      return `github:alexykn/iyon-tui#${tuiSha}`;
    });
    if (updated !== source) await writeFile(manifest, updated);
  }
  if (packageMatches === 0) {
    throw new Error("no @iyon/tui Git dependency was found in the branch worktree");
  }

  const cargoManifest = join(worktree, "Cargo.toml");
  const cargoSource = await readFile(cargoManifest, "utf8");
  let cargoMatches = 0;
  const cargoUpdated = cargoSource.replace(TUI_CARGO_PATTERN, (_match: string, prefix: string) => {
    cargoMatches += 1;
    return `${prefix}rev = "${tuiSha}"`;
  });
  if (cargoMatches === 0) {
    throw new Error("the iyon-tui Cargo dependency was not found in Cargo.toml");
  }
  if (cargoUpdated !== cargoSource) await writeFile(cargoManifest, cargoUpdated);
}

async function ensurePersistentWorktree(
  branch: string,
  appHead: string,
  tuiSha: string,
  sourceState: string,
): Promise<{ path: string; state: "created" | "reused"; dependenciesPrepared: boolean }> {
  const path = worktreePath(branch);
  await mkdir(WORKTREE_ROOT, { recursive: true });
  let registered = registeredWorktreePaths().includes(path);

  if (registered && !(await pathExists(path))) {
    runChecked(["git", "worktree", "prune"], APP_ROOT, false);
    registered = false;
  }
  if (registered) {
    const currentHead = run(["git", "rev-parse", "HEAD"], path);
    const cached = await readWorktreeState(path);
    const reusable = currentHead.exitCode === 0
      && currentHead.stdout.trim() === appHead
      && cached?.appHead === appHead
      && cached.tuiSha === tuiSha
      && cached.sourceState === sourceState
      && await pathExists(join(path, "node_modules"))
      && await pathExists(join(path, "Cargo.lock"));
    if (reusable) return { path, state: "reused", dependenciesPrepared: true };
    runChecked(["git", "reset", "--hard", appHead], path, false);
    return { path, state: "reused", dependenciesPrepared: false };
  }
  if (await pathExists(path)) {
    throw new Error(`cache path exists but is not registered as an app worktree: ${path}`);
  }

  runChecked(["git", "worktree", "add", "--detach", path, appHead], APP_ROOT);
  return { path, state: "created", dependenciesPrepared: false };
}

function updateTuiCargoLock(worktree: string): void {
  const result = run(["cargo", "update", "-p", "iyon-tui"], worktree);
  if (result.exitCode === 0) {
    if (result.stdout.length > 0) process.stdout.write(result.stdout);
    if (result.stderr.length > 0) process.stderr.write(result.stderr);
    return;
  }
  // The extracted application workspace keeps the TUI dependency declaration
  // for branch rewriting, but no application crate currently uses that Rust
  // crate. A fresh lockfile therefore has no iyon-tui package to update.
  if (result.stderr.includes("package ID specification `iyon-tui` did not match any packages")) return;
  if (result.stdout.length > 0) process.stdout.write(result.stdout);
  if (result.stderr.length > 0) process.stderr.write(result.stderr);
  throw new Error(`command failed (${result.exitCode}): cargo update -p iyon-tui`);
}

async function buildStable(): Promise<void> {
  runChecked(["bun", "run", "native:stage"], APP_ROOT);
  runChecked(["bun", "run", "native:tui:stage"], APP_ROOT);
  runChecked(["bun", "run", "packages/iyon-cli/build.ts"], APP_ROOT);
}

async function syncUncommittedChanges(worktree: string): Promise<void> {
  // Write uncommitted changes (staged + unstaged) as a patch file, then
  // apply to the worktree so WIP code is included without needing a commit.
  const patch = run(["git", "diff", "HEAD"], APP_ROOT);
  if (patch.exitCode !== 0) {
    throw new Error(`unable to capture uncommitted changes:\n${patch.stderr}`);
  }
  if (patch.stdout.length > 0) {
    const tmpDir = await mkdtemp(join(APP_ROOT, ".tmp-patch-"));
    try {
      const patchFile = join(tmpDir, "wip.patch");
      await writeFile(patchFile, patch.stdout);
      const result = run(["git", "-C", worktree, "apply", patchFile], APP_ROOT);
      if (result.exitCode !== 0) {
        console.warn(
          `warning: could not apply uncommitted changes to worktree (${result.stderr.trim()}) — ` +
          `building committed code only`,
        );
      }
    } finally {
      await rm(tmpDir, { recursive: true, force: true });
    }
  }

  // Copy untracked files that exist in the source but not in the worktree.
  const untracked = run(["git", "ls-files", "--others", "--exclude-standard"], APP_ROOT);
  if (untracked.exitCode === 0 && untracked.stdout.length > 0) {
    for (const line of untracked.stdout.split(/\r?\n/).filter(Boolean)) {
      const src = join(APP_ROOT, line);
      const dest = join(worktree, line);
      try {
        await mkdir(dirname(dest), { recursive: true });
        await copyFile(src, dest);
      } catch {
        // skip files that disappeared between listing and copy
      }
    }
  }
}

async function buildBranch(branch: string): Promise<void> {
  const appHead = runChecked(["git", "rev-parse", "HEAD"], APP_ROOT, false).stdout.trim();
  const tuiSha = resolveBranchSha(branch);
  const sourceState = await sourceStateKey();
  const worktree = await ensurePersistentWorktree(branch, appHead, tuiSha, sourceState);
  const output = join(APP_ROOT, "dist", "iyon");
  const temporaryOutput = `${output}.tui-${branchSlug(branch)}.tmp`;

  try {
    console.log(
      `${worktree.state} persistent worktree ${relative(APP_ROOT, worktree.path)}; building against iyon-tui/${branch} @ ${tuiSha}`,
    );

    if (!worktree.dependenciesPrepared) {
      // Sync uncommitted changes into the worktree so dev builds include WIP code.
      await syncUncommittedChanges(worktree.path);

      await switchTuiDependencies(worktree.path, tuiSha);
      runChecked(["bun", "install"], worktree.path);
      updateTuiCargoLock(worktree.path);
      await writeWorktreeState(worktree.path, { appHead, tuiSha, sourceState });
    } else {
      console.log("reused prepared dependency state and incremental build cache");
    }
    runChecked(["bun", "run", "native:stage"], worktree.path);
    runChecked(["bun", "run", "native:tui:stage"], worktree.path);
    runChecked(["bun", "run", "packages/iyon-cli/build.ts"], worktree.path);

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
  const paths = [worktreePath(branch), ...(branch === "perf-refactor" ? [LEGACY_WORKTREE] : [])];
  const registeredPaths = new Set(registeredWorktreePaths());
  let removed = false;
  for (const path of paths) {
    if (registeredPaths.has(path)) {
      runChecked(["git", "worktree", "remove", "--force", path], APP_ROOT);
      removed = true;
      continue;
    }
    if (await pathExists(path)) {
      await rm(path, { recursive: true, force: true });
      removed = true;
    }
  }
  if (!removed) {
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
  await rm(LEGACY_WORKTREE, { recursive: true, force: true });
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
