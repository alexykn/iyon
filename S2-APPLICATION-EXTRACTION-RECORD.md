# S2 application extraction record

**Status:** COMPLETE on branch `perf-refactor`

## Repository identity

| Role | Repository | GitHub repository ID | Created |
|---|---|---|---|
| Canonical original/TUI history | `alexykn/iyon-tui` | `R_kgDOTw9laA` | 2026-08-07 |
| Filtered application history | `alexykn/iyon` | `R_kgDOUDBIpA` | 2026-08-24 |

The first repository is the original `alexykn/iyon` repository renamed without
history rewriting. The second was verified first as
`alexykn/iyon-app-extract`, then renamed to `alexykn/iyon` only after the
original name was released. Repository IDs prove the names were assigned to the
intended histories rather than relying on GitHub redirects.

## Extraction provenance

- Source branch/head: `perf-refactor` at
  `55f232738aa362d8eb2c45e2e6e7e26468abe2ec`.
- Tool: `git-filter-repo 2.47.0`, invoked through `uv run`.
- Filtered source head before provenance commit:
  `ffe17687c159aece8a3b4648e3cfcdf7119312e9`.
- Provenance commit: `2f0053186e756a043d25d38a6499d99efd5bf146`.
- `SOURCE-SHA-MAP.jsonl`: 713 unique source SHAs; 613 mapped filtered commits;
  100 commits explicitly recorded as pruned (`filtered_sha: null`).
- Every old SHA resolves in `alexykn/iyon-tui`; every non-null filtered SHA
  resolves in this repository.
- Source pre-separation tag target `ba33316c95cf7c45743acb0957232912757d8a77`
  maps to filtered target `2eedb6a3581c28a65b473a925124256f1002186d`.
  The public annotated tag remains canonical only in `alexykn/iyon-tui`.

The exact command and 38 included path roots are recorded both per JSONL row
and in `EXTRACTION-PROVENANCE.json`.

## Temporary compatibility

S2 is behavior-neutral, so this checkout temporarily retains local TUI crate,
native, runtime, ABI generator, benchmark, and boundary-check paths needed by
the mixed workspace. `APPLICATION-EXTRACTION-COMPATIBILITY.md` assigns each to
its canonical TUI owner and a mandatory S3–S5 removal tranche. No retained path
creates shared ownership or authorizes new deep imports.

## Local verification

- `bun install --frozen-lockfile`: PASS (one intentionally absent
  `@iyon/tui-consumer-fixture` workspace noted by Bun).
- `bun run native:stage`: PASS, darwin-arm64 addon staged and loaded.
- `bun run check:ownership`: all 11 ownership gates PASS.
- `bun run typecheck`: PASS.
- `cargo test --workspace --exclude api-surface`: 1,079 pass / 0 fail / 3
  ignored, matching S0's non-API-surface baseline.
- Plugin framework: 30 pass / 0 fail.
- Application plugin suite: 113 pass / 1 known baseline failure
  (`production_successful_ls_is_green_finished`), unchanged from S0.
- Provenance integrity: all 713 source SHAs and all 613 mapped destination SHAs
  resolve; JSON/JSONL parse and `git diff --check` pass.

## Hosting state

Both repositories are public. Both retain `main`, `bun-refactor`, and
`perf-refactor`; default branch remains `main`. The migration work and
provenance artifacts are on `perf-refactor`. The application repository has no
issues, pull requests, releases, or tags at cutover. No CI result is used as
evidence; all checks above were run locally.
