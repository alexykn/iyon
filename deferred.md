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
