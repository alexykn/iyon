# Deferred after bun-refactor approval

`bun_refactor.md` is approved as implemented. These leftovers are intentional and come **after** the performance work. Do not treat them as current blockers.

## After performance

- **TUI bridge / Tranche 11.** Recursive `materializeView` is the accepted first cut. Record overhead vs the Rust baseline and optimize from the dedicated performance handoff, not from this list.
- **Hardening / Tranche 12.** 100-extension stress, catalog/index, extension docs, distribution verification beyond the existing standalone `dist/iyon`.

## Plugin ecosystem (after performance)

Internal capability exists (`PackageLoader`, registries, activation, fixtures). Production does not yet behave as a third-party extension platform.

- Wire **user** and **project** package discovery into `createProductionStages()` (`packages/iyon-cli/src/bootstrap.ts` currently passes bundled roots only).
- Make **`npm:` / `git:`** real package sources. Today they are labels; discovery always reads `package.json` from a filesystem root.
- Call **`SceneExtensions.apply()`** on the product render path. `apply()` and dogfood tests exist; `plugins/app/iyon` still does `tui.render(new Scene(...))` directly.

## Accepted as-is (do not “fix” unless a later API lock requires it)

- **Scene `replace` vs `compose`.** `compose(scene, ctx)` can ignore `scene` and return a new one, so it covers overwrite. `replace(ctx)` is weaker extra API.
- **Kernel projection names.** `session.snapshot().entries` plus local filter/lower is the same usage as `entries()` / `messages()` / `toModelMessages()`.
- **No `ARCHITECTURE.md`.** Final architecture is not locked; `bun_refactor.md` is the working contract.

## Deferred PERF-12 — Markdown stream spike bypass

> **Not implemented; deferred.** This is a follow-up for a possible Markdown
> streaming freeze, not part of the completed PERF-10 work.

When Markdown is mid-paragraph, mid-table, or mid-list, its stable frontier may
stall while bytes continue arriving. The proposed fix is a generic host-pipeline
spike detector that watches pending smoother units and consecutive revisions
without stable-frontier progress. Once both thresholds are exceeded (suggested
defaults: 200 pending grapheme units and 3 stalled revisions), temporarily
render the unstable tail as plain raw text so the viewport continues advancing.

The raw region remains unstable and is replaced by the normal Markdown
projection once a newline or block boundary advances stabilization. The design
requires recovery/cold-reference equivalence tests for paragraphs and tables,
a no-spike test for newline-terminated streams, and counters for activations,
raw bytes, and later Markdown upgrades. The likely implementation scope is the
host Markdown/Smooth/TextRenderer pipeline; no `iyon-core` or `iyon-api` work
is implied.
