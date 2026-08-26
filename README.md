# Iyon

Iyon is an agent application whose generic terminal UI framework is consumed
from the external [`alexykn/iyon-tui`](https://github.com/alexykn/iyon-tui)
repository.

## Requirements

- Bun 1.4.0
- Rust/Cargo

Install dependencies from the repository root:

```sh
bun install
```

## Builds

The normal build uses the stable TUI revision pinned in this repository:

```sh
bun run build:iyon
./dist/iyon --help
```

To build against the newest commit on any TUI branch, pass the branch name:

```sh
bun run build:iyon -- perf-refactor
bun run build:iyon -- main
bun run build:iyon -- feature/my-branch
```

Branch builds require a clean application checkout. The branch head is resolved
at build time and used for both the Bun package and Rust crate. Each branch gets
its own persistent worktree, `node_modules`, and Cargo target directory under
`~/.cache/iyon/tui-branches/` (or `$XDG_CACHE_HOME`). Subsequent builds reuse
those artifacts incrementally. The compatibility alias below is equivalent to
the first branch example:

```sh
bun run build:iyon:perf-refactor
```

A separate local `iyon-tui` clone is not required.

## Cache cleanup

Remove one branch's persistent worktree and build cache:

```sh
bun run clean:iyon -- perf-refactor
```

Remove all branch worktrees and caches for this application checkout:

```sh
bun run clean:iyon -- --all
```

These commands do not remove the global Bun or Cargo download caches, or the
last copied `dist/iyon` executable.
