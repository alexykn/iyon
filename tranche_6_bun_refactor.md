# Tranche 6 — Bun extension/package runtime

## Mission

Make arbitrary TypeScript packages loadable through the final Bun extension API. A package is a distribution unit, an extension is an executable TypeScript module, and a contribution is a well-known registration made by an extension; the runtime must keep those concepts separate and must load bundled and third-party extensions through the same transactional loader. This tranche delivers package discovery and manifest validation, compatibility checks, source ownership, layered registries, activation rollback, unload/replacement, event hooks, app selection, and Scene composition through `packages/iyon-plugins/`, while leaving the product’s real providers, tools, agent, and app to later tranches.

## Prerequisites

- T0 has replaced the old architecture contract and frozen the Bun + TypeScript + N-API direction.
- T1 has created the Bun workspace/runtime, installed the virtual-module resolver for `iyon:api`, `iyon:core`, `iyon:tui`, and `iyon:plugins`, and provided the resolver initialization hook that must run before an extension module is imported. T6 consumes that hook; it does not create a second resolver.
- T2 has created `packages/iyon-sdk/` and the API-surface inventory. T6 uses the SDK as the development/type package; extension authors must not need to install the native addon.
- T3 has made `iyon-core` provider-agnostic and injectable rather than a source of hidden default product registrations. T4 has supplied the TypeScript `iyon:api` and `iyon:core` surfaces.
- T5 has supplied the `iyon:tui` bindings, including the native-backed `Scene` value and the Scene construction/composition operations used here. T6 must not duplicate terminal layout or scrollback logic in TypeScript.
- The existing Rust workspace remains green before each implementation commit. T6 does not require unfinished T7–T10 code to exist and must not use it as a prerequisite.

## Invariants (do not violate)

- The canonical runtime imports are `iyon:api`, `iyon:core`, `iyon:tui`, and `iyon:plugins`. `@iyon/sdk` supplies authoring and type declarations; it is not a required native runtime dependency for extension packages.
- `iyon-package` is only the discoverability marker. `iyon.extensions` is the authoritative list/map of executable entrypoints. `iyon.catalog` is static advisory metadata for iyon.dev and must never restrict runtime behavior or permissions.
- The public package-source model is `npm:`, `git:`, and local path. Package-source metadata belongs to discovery/install configuration, not to the extension execution API. Module and dependency resolution use Bun’s normal resolution rules.
- Load precedence is deterministic: bundled packages, then user-installed packages, then project-local packages; order within each scope is deterministic. Scope is recorded on every contribution.
- Extensions are arbitrary TypeScript running with the Iyon process privileges. Registration is not a permission boundary, and T6 must not fake a sandbox by omitting useful Bun APIs or inventing WASM, Rhai, WIT, or a Rust-owned process.
- `ExtensionAPI` is the general runtime abstraction. Tool, provider, agent, app, command, and shortcut registrations are contribution types, not separate execution/plugin base classes.
- Registries reject duplicate IDs by default. Replacement is explicit (`register(value, { replace: true })`) and produces a layered registration so unloading the upper registration restores the previous active registration.
- Every contribution carries source metadata: package id, extension id, registration id, generation, and load scope. A disposal operation may remove only the registration generation it owns; an old extension must not remove a later replacement accidentally.
- Registration calls return idempotent Disposables. Activation may return a Disposable. Extension unload disposes resources in reverse activation order, and rollback disposes all registrations/resources created by the failed activation in reverse order.
- Activation is all-or-nothing: discover package → parse manifest → validate compatibility → resolve entrypoint → begin transaction → import module → call `activate(extensionAPI)` → validate registrations → commit. Import, activation, or validation failure leaves no half-registered extension and does not stop unrelated packages from loading.
- The loader has no `is_builtin` execution branch. Bundled packages are ordinary discovered package candidates and use the exact same manifest, module-resolution, activation, transaction, and unload path as external packages.
- Apps, Scene extensions, and app selection remain separate. A selected replacement app creates its own app; it must not instantiate the built-in Iyon app and erase its Scene afterward. Scene extensions run only after the selected app has produced its base Scene.
- T5 owns Scene semantics and terminal rendering. T6 only stores and orders Scene extension callbacks and invokes the T5 Scene API; it does not add a parallel Scene representation or renderer.

## Adjacent tranche contracts

### Consumes from earlier tranches

- T1’s installed virtual-module resolver and its initialization function, plus the Bun runtime/package workspace.
- T2’s `@iyon/sdk` contracts and API-surface decisions.
- T3/T4’s provider-agnostic kernel and public `iyon:api`/`iyon:core` values.
- T5’s `iyon:tui` bindings, especially the native-backed `Scene` value and the app runtime context required to create and return one.

### Exports for later tranches

- `packages/iyon-plugins/` as the runtime implementation of `iyon:plugins`.
- `ExtensionAPI` with typed `tools`, `providers`, `agents`, `apps`, `commands`, `shortcuts`, `scene`, and `on()` surfaces.
- Package discovery/manifest/compatibility diagnostics, deterministic load scopes, extension load results, explicit unload, and source metadata.
- Layered registries whose `register()` methods return ownership-safe Disposables and whose replacement behavior restores the previous layer on unload.
- The transactional loader used by every package, plus app selection and Scene composition/replacement after the selected app returns its base Scene.
- Fixture packages and end-to-end assertions that T7–T10 can reuse as regression tests while registering the real bundled provider, tool, agent, and app contributions.

### Must not steal from neighboring tranches

- T6 must not port OpenRouter, OpenAI Codex, mock providers, built-in tools, the default agent, or the default Iyon app. T7–T10 register those as ordinary bundled packages through the loader delivered here.
- T6 must not move product behavior into `iyon-native`, `iyon-core`, `iyon-api`, or `iyon-tui`, and must not change the Rust public API inventory to make the loader easier.
- T6 must not replace T5’s Scene bindings, optimize fluent TUI calls, or add a second terminal/layout implementation; T11 owns post-parity optimization.
- T6 must not implement a permissions/security system, package manager, custom npm ecosystem, standalone executable packaging, or ecosystem hardening beyond the loader checks needed for this tranche.

## Out of scope

- Any WASM, Rhai, WIT, embedded JavaScript engine, Rust-owned process, or Rust background extension host.
- Real product ports from `crates/iyon-api`, `crates/iyon-core`, `crates/iyon-tui`, or `crates/iyon`; the existing Rust application remains a behavioral reference until T7–T10 migrate it.
- Provider HTTP/auth behavior, tool execution policy, agent loops/subagents, conversation UI, CLI command parsing, default app UX parity, Bun standalone executable generation, and removal of `crates/iyon/src/main.rs`.
- A security sandbox or permission allowlist. Extensions intentionally run with normal Bun/Iyon process privileges; a future permissions policy is layered on top of this runtime.
- Fetching, installing, authenticating, or caching npm/git packages. T6 accepts the public source descriptors and delegates installed package/module resolution to Bun.
- TUI bridge performance work, new native handles, Rust API cleanup, or changes to `ARCHITECTURE.md`.

## Commits

### Commit 1 — define package manifests, sources, discovery, and compatibility

**Why:** Establish the authoritative package boundary before any extension code can execute. Discovery must distinguish a package’s static metadata from the executable entrypoints and must produce deterministic, diagnosable candidates for the loader.

**Files:**

- `packages/iyon-plugins/package.json`
- `packages/iyon-plugins/src/errors.ts`
- `packages/iyon-plugins/src/manifest.ts`
- `packages/iyon-plugins/src/package-source.ts`
- `packages/iyon-plugins/src/discovery.ts`
- `packages/iyon-plugins/src/compatibility.ts`
- `packages/iyon-plugins/tests/manifest.test.ts`
- `packages/iyon-plugins/tests/discovery.test.ts`

**Work:**

- Create the `iyon-plugins` workspace package with strict TypeScript settings, a test script, and exports that can later back the `iyon:plugins` virtual module. Keep implementation dependencies limited to the workspace/runtime contracts already supplied by T1–T5.
- Define normalized manifest types containing package id/name, version, `keywords`, the authoritative `iyon.extensions` entrypoints, optional advisory `iyon.catalog`, and compatibility requirements. Accept both the canonical entrypoint collection and the shorthand string form such as `"./src/index.ts"`; normalize relative paths against the package root and reject malformed, missing, or ambiguous entrypoints before import.
- Require the `iyon-package` keyword for discovery, but do not interpret `catalog` as an allowlist. Preserve unknown catalog fields for static tooling without allowing them to affect runtime registration.
- Model `npm:`, `git:`, and local-path package sources as discovery metadata. Validate and normalize source identity without adding source-specific execution APIs or a custom installer. Preserve enough origin information for package/extension diagnostics and source metadata.
- Add `LoadScope` values for `bundled`, `user`, and `project`, and make discovery return candidates in that precedence order with stable sorting within each scope. Do not silently randomize filesystem enumeration or rely on object-key order for package order.
- Validate the runtime compatibility contract before resolving an extension entry: package version/engine requirements, required Iyon API/runtime ranges, and supported manifest shape should produce typed errors containing package identity and source. Compatibility failure must be distinguishable from import and activation failure.
- Keep discovery pure apart from filesystem/package metadata reads so later loader tests can supply a candidate list directly. Do not import or execute extension modules in this commit.

**Tests / verification:**

- `bun test packages/iyon-plugins/tests/manifest.test.ts packages/iyon-plugins/tests/discovery.test.ts`
- Assert `iyon-package` is required, shorthand and expanded entrypoints normalize identically, `iyon.extensions` remains authoritative when `catalog` disagrees, and invalid manifests return package/source-qualified diagnostics.
- Assert bundled → user → project ordering and deterministic same-scope ordering across repeated discovery runs; assert source descriptors normalize for npm, git, and local inputs.
- Assert incompatible package/runtime ranges fail before any entrypoint resolution and that a compatible package produces normalized extension identities.
- `cargo test --workspace` to prove the Rust baseline is unchanged.

**Must not:** import extension modules, call activation, inspect `is_builtin`, add a package installer, use catalog metadata to restrict APIs, or add a real product provider/tool/app.

### Commit 2 — implement source-owned layered contribution registries

**Why:** Give every contribution type one consistent registration and ownership model before exposing it to extension activation. Replacement and rollback depend on generation-aware layers rather than destructive map updates.

**Files:**

- `packages/iyon-plugins/src/disposable.ts`
- `packages/iyon-plugins/src/contributions.ts`
- `packages/iyon-plugins/src/registry.ts`
- `packages/iyon-plugins/src/registries.ts`
- `packages/iyon-plugins/tests/registry.test.ts`

**Work:**

- Define `Disposable` as an idempotent `dispose(): void | Promise<void>` contract and the internal disposal stack used to unwind resources in reverse order.
- Define `LoadScope`/source metadata and a registration record carrying package id, extension id, registration id, monotonically allocated generation, and scope. Keep source metadata attached to the contribution returned by lookups and diagnostics, not in an out-of-band log.
- Implement a generic layered registry with deterministic active lookup/listing, ownership-safe removal, and an explicit `replace` option. A duplicate active id without `replace: true` fails. A replacement pushes a new layer; disposing that exact generation reveals the prior layer. Disposing an already removed generation is a no-op, while disposing a stale owner cannot remove a newer registration.
- Build typed registry façades for tools, providers, agents, apps, commands, and shortcuts over the same generic implementation. Keep contribution values structural and SDK-oriented; do not make old Rust `ToolPlugin`, `ProviderPlugin`, or similar types the runtime base class.
- Define the registration options and error types so the public API only exposes `replace` while the loader injects trusted package/extension/source metadata. Make the registry API explicit that registration is lifecycle ownership, not permission enforcement.
- Provide snapshot/list/lookup operations needed by activation validation and later product tranches, with stable ordering and source metadata available for diagnostics.

**Tests / verification:**

- `bun test packages/iyon-plugins/tests/registry.test.ts`
- Name and assert cases for duplicate rejection, explicit replacement, layered fallback after unload, same-id stale-disposable protection, idempotent disposal, deterministic listing, and complete source metadata.
- Exercise all six typed registry façades with minimal fixture contribution values; assert they share the same duplicate/replacement behavior.
- `cargo test --workspace` and the package typecheck command from `packages/iyon-plugins/package.json`.

**Must not:** register real bundled defaults, call `register_builtin_defaults`, expose a permission check, mutate Rust registries, or make unloading remove a newer generation.

### Commit 3 — expose `ExtensionAPI` and typed lifecycle events

**Why:** Define the final extension-facing contract independently from package loading. Extension authors need one general API for all well-known contribution types, and the loader needs a transaction-scoped API that automatically records source ownership.

**Files:**

- `packages/iyon-plugins/src/extension-api.ts`
- `packages/iyon-plugins/src/events.ts`
- `packages/iyon-plugins/src/extension-context.ts`
- `packages/iyon-plugins/tests/extension-api.test.ts`
- `packages/iyon-plugins/tests/events.test.ts`

**Work:**

- Define and export the required shape:

  ```ts
  interface ExtensionAPI {
    readonly tools: ToolRegistry;
    readonly providers: ProviderRegistry;
    readonly agents: AgentRegistry;
    readonly apps: AppRegistry;
    readonly commands: CommandRegistry;
    readonly shortcuts: ShortcutRegistry;
    readonly scene: SceneExtensions;
    on<E extends keyof ExtensionEvents>(event: E, handler: ExtensionHandler<E>): Disposable;
  }
  ```

- Define contribution contracts in terms of the SDK/T4/T5 public values. At minimum, app contributions carry `{ id, create(ctx) { return new App(ctx); } }`; commands, shortcuts, and Scene handlers have explicit callback/context types; tool/provider/agent values remain replaceable structural contracts for T7–T9.
- Construct a transaction-scoped API that binds every registration to the current package id, extension id, scope, and generation. An extension cannot supply or alter source metadata through ordinary public registration options.
- Define typed `ExtensionEvents` for lifecycle and registry changes, including activation/activation-failure, registration/replacement, unload, and Scene/app selection events. Emit only committed lifecycle changes as committed events; rollback events must describe failure without pretending registrations became active.
- Make `on()` return a Disposable and register that subscription with the current activation scope so unload and rollback remove handlers. Guard event dispatch against mutation of the registry’s internal layer state and preserve deterministic handler order.
- Keep event hooks observational and lifecycle-oriented; do not turn them into a second execution or authorization mechanism.

**Tests / verification:**

- `bun test packages/iyon-plugins/tests/extension-api.test.ts packages/iyon-plugins/tests/events.test.ts`
- Compile a synthetic extension against the public `ExtensionAPI` shape and assert all six registries plus `scene` and `on()` are available.
- Assert registrations receive the correct package/extension/scope metadata, `replace` is the only public override switch, event handlers are disposable, and handlers registered during a failed transaction do not survive.
- Assert event ordering for register → replace → unload and that a thrown observer is reported without corrupting registry state; do not swallow activation errors.
- `cargo test --workspace`.

**Must not:** expose native addon installation requirements, make event handlers permission gates, add product-specific tool/provider/app behavior, or bind the API to a Rust-owned callback loop.

### Commit 4 — add transactional extension activation and unload

**Why:** Make extension execution safe and composable. Import or activation failures must roll back all registrations/resources and allow unrelated packages to continue loading.

**Files:**

- `packages/iyon-plugins/src/extension-module.ts`
- `packages/iyon-plugins/src/activation.ts`
- `packages/iyon-plugins/src/loader.ts`
- `packages/iyon-plugins/src/load-errors.ts`
- `packages/iyon-plugins/tests/activation.test.ts`
- `packages/iyon-plugins/tests/unload.test.ts`

**Work:**

- Normalize the authoritative manifest entrypoint to one extension module contract with an `activate(extensionAPI)` export. Reject missing/ambiguous activation exports with a package id, extension id, and entrypoint-qualified error rather than guessing a product-specific shape.
- Implement the exact activation sequence: consume a discovered candidate, re-check compatibility, resolve the entrypoint through the T1-installed virtual-module resolver, begin a contribution transaction, import the module, call `activate`, await async activation, validate all registrations, and commit only after validation succeeds.
- Track registration Disposables, `on()` subscriptions, and the optional activation Disposable in activation order. Roll back in reverse order on import, activation, or validation failure. If an activation Disposable was returned before a later validation error, dispose it as part of rollback.
- Return structured load results containing package/extension identity, committed generation/source metadata, and non-fatal failures. Continue through unrelated candidates after one candidate fails; aggregate/report failures without silently suppressing their cause.
- Implement explicit unload by package/extension identity. Unload all owned registrations/resources in reverse activation order and await asynchronous Disposables. Registry layers reveal previous registrations after a successful unload.
- Ensure failed activation does not emit committed registration events or leave event listeners, app choices, Scene handlers, or registry layers behind. Keep the loader generic: no bundled/external branch and no `is_builtin` shortcut.
- Keep the loader’s external module import path compatible with arbitrary npm dependencies; extensions may use normal Bun APIs and canonical `iyon:*` imports.

**Tests / verification:**

- `bun test packages/iyon-plugins/tests/activation.test.ts packages/iyon-plugins/tests/unload.test.ts`
- Assert successful synchronous and asynchronous activation, activation Disposable execution, import failure rollback, thrown activation rollback, registration validation rollback, reverse disposal order, package-qualified diagnostics, and continuation to a later unrelated extension.
- Assert duplicate registration fails unless `replace: true`, replacement unload restores the prior active layer, and a failed replacement leaves the prior layer active.
- Assert the test harness records the same loader call path for a bundled candidate and an external candidate and that no loader code checks `is_builtin`.
- `cargo test --workspace` and the package typecheck command.

**Must not:** catch and discard errors, leave partially active registrations, instantiate the built-in app as a fallback for a failed replacement app, execute a Rust-owned extension process, or add sandbox behavior.

### Commit 5 — implement app selection and Scene extension composition

**Why:** Complete the “do anything” application escape hatch without conflating app replacement with Scene composition. This gives later bundled and third-party apps the same selection and render pipeline.

**Files:**

- `packages/iyon-plugins/src/app-registry.ts`
- `packages/iyon-plugins/src/scene-extensions.ts`
- `packages/iyon-plugins/src/app-selection.ts`
- `packages/iyon-plugins/tests/app-selection.test.ts`
- `packages/iyon-plugins/tests/scene-extensions.test.ts`

**Work:**

- Implement `AppRegistry.register({ id, create(ctx) { return new App(ctx); } })` over the layered registry. Keep app factory creation separate from selection so registering an app does not instantiate it.
- Define explicit selection input from config/CLI-facing runtime data and a deterministic selected-app result. Selection resolves one factory and creates that app from its own context; it must never construct the built-in Iyon app and then erase or replace its Scene.
- Define `SceneExtensions` registration for ordered composers and full replacers. After the selected app produces its base T5 `Scene`, apply the extension chain in deterministic registration order, with each callback receiving the current Scene and context and returning a T5 Scene (or an awaited result if the T5/runtime contract permits async composition).
- Make `compose` preserve/transform the current Scene and `replace` explicitly supply the final Scene. Track both through the same source metadata and Disposable ownership rules as other contributions.
- Define behavior when an app or Scene callback fails: surface package/extension identity, do not render a half-composed Scene, and let the runtime’s activation/load error policy decide whether the owning extension is unloaded. Do not silently fall back to the built-in app.
- Ensure removal of a Scene extension changes the next composition chain without mutating T5’s native Scene internals or duplicating its history/scrollback state.

**Tests / verification:**

- `bun test packages/iyon-plugins/tests/app-selection.test.ts packages/iyon-plugins/tests/scene-extensions.test.ts`
- Assert a replacement app’s factory is the only factory invoked, base Scene creation precedes every Scene extension, compose order is deterministic, replace returns a full replacement Scene, and unloading one layer reveals the previous Scene extension.
- Assert app IDs duplicate-fail by default and require explicit replacement; assert a Scene callback failure includes source identity and cannot expose a partially composed result.
- Use the T5 native-backed Scene binding in the tests rather than a TypeScript shadow Scene/layout implementation.
- `cargo test --workspace`.

**Must not:** port the default Iyon app, add conversation/status/menu widgets, alter `iyon-tui`, erase a built-in Scene to simulate app replacement, or make Scene extensions a hidden app registry.

### Commit 6 — wire `iyon:plugins` to the shared runtime and scope loader

**Why:** Make the implementation reachable through the canonical virtual module and prove that virtual-module setup happens before any extension import. This is the integration point between T1’s Bun runtime and the standalone `iyon-plugins` package.

**Files:**

- `packages/iyon-plugins/src/load-scopes.ts`
- `packages/iyon-plugins/src/package-loader.ts`
- `packages/iyon-plugins/src/index.ts`
- `packages/iyon-plugins/tsconfig.json`
- `packages/iyon-runtime/src/virtual-modules.ts`
- `packages/iyon-runtime/src/index.ts`
- `packages/iyon-plugins/tests/loader.integration.test.ts`

**Work:**

- Add the public `iyon:plugins` entrypoint that exports `ExtensionAPI`, registry types, discovery/manifest types, loader creation, package loading/unloading, app selection, and Scene composition. Keep internal transaction/source details private except for read-only diagnostics.
- Extend T1’s existing virtual-module table/installation hook only enough to point `iyon:plugins` at this implementation. Preserve T1’s resolver ownership and initialization semantics; do not duplicate or bypass its module resolver.
- Build the scope loader that discovers bundled package descriptors, user-installed descriptors, and project-local descriptors, sorts them deterministically, and passes every candidate to the same Commit 4 loader. Bundled entries in this commit are stubs/fixture descriptors only.
- Ensure `installVirtualModules()` (or the T1-equivalent initialization function) completes before resolving/importing the first extension entrypoint. Add a guard/test that rejects loading if the resolver was not installed, rather than allowing a confusing `iyon:*` import failure halfway through activation.
- Use Bun’s normal package resolution for local/npm/git-installed package trees and dependency imports. The loader should receive resolved package roots/entrypoints from discovery and must not implement a second resolver or source-specific execution branch.
- Expose a runtime object whose registries and loader share one lifecycle, so app selection and Scene composition observe the same active contributions and unload state.

**Tests / verification:**

- `bun test packages/iyon-plugins/tests/loader.integration.test.ts`
- Assert canonical `iyon:plugins` import resolves after T1 virtual-module installation and that an extension importing `iyon:api`, `iyon:core`, or `iyon:tui` is imported only after installation.
- Assert all three scopes are discovered and loaded in deterministic order through one loader entrypoint; assert the same result shape for bundled and external candidates.
- Assert an uninstalled resolver fails before extension import and reports the missing runtime initialization explicitly.
- `cargo test --workspace`; run the package typecheck/build command to verify both runtime and virtual-module paths.

**Must not:** create a second virtual-module resolver, add a bundled-only loader, make bundled registrations privileged, install packages, or load real T7–T10 product extensions.

### Commit 7 — add fixture packages and end-to-end extension coverage

**Why:** Exercise the contract with realistic package boundaries before later tranches begin migrating product contributions. Fixtures must cover lifecycle edge cases, not just a successful `register()` call.

**Files:**

- `packages/iyon-plugins/tests/fixtures/packages/register-tool/package.json`
- `packages/iyon-plugins/tests/fixtures/packages/register-tool/src/index.ts`
- `packages/iyon-plugins/tests/fixtures/packages/multiple-contributions/package.json`
- `packages/iyon-plugins/tests/fixtures/packages/multiple-contributions/src/index.ts`
- `packages/iyon-plugins/tests/fixtures/packages/override-builtin/package.json`
- `packages/iyon-plugins/tests/fixtures/packages/override-builtin/src/index.ts`
- `packages/iyon-plugins/tests/fixtures/packages/activation-failure/package.json`
- `packages/iyon-plugins/tests/fixtures/packages/activation-failure/src/index.ts`
- `packages/iyon-plugins/tests/fixtures/packages/unload-restore/package.json`
- `packages/iyon-plugins/tests/fixtures/packages/unload-restore/src/index.ts`
- `packages/iyon-plugins/tests/fixtures/packages/scene-composer/package.json`
- `packages/iyon-plugins/tests/fixtures/packages/scene-composer/src/index.ts`
- `packages/iyon-plugins/tests/fixtures/packages/scene-replacement/package.json`
- `packages/iyon-plugins/tests/fixtures/packages/scene-replacement/src/index.ts`
- `packages/iyon-plugins/tests/fixtures/packages/async-activation/package.json`
- `packages/iyon-plugins/tests/fixtures/packages/async-activation/src/index.ts`
- `packages/iyon-plugins/tests/fixtures/packages/npm-dependency/package.json`
- `packages/iyon-plugins/tests/fixtures/packages/npm-dependency/src/index.ts`
- `packages/iyon-plugins/tests/fixtures/dependency-package/package.json`
- `packages/iyon-plugins/tests/fixtures/dependency-package/src/index.ts`
- `packages/iyon-plugins/tests/fixtures/bundled/index.ts`
- `packages/iyon-plugins/tests/e2e-loader.test.ts`

**Work:**

- Make each fixture a normal package with `iyon-package`, `iyon.extensions`, and a minimal manifest/catalog combination. Do not add product providers/tools/agents/apps; contribution values should be test doubles that conform to the public structural contracts.
- Cover one tool registration; multiple contribution types in one activation; explicit override of a seeded builtin layer; failure after multiple registrations; unload restoring the previous layer; Scene composition; full Scene replacement; async activation; and an extension importing a dependency through Bun’s normal package resolution.
- Keep the dependency fixture local/workspace-owned so the test does not add an unrelated third-party dependency. The assertion must still prove a package dependency import rather than a relative source import; if the earlier Bun workspace already provides a locked npm package suitable for this purpose, use that existing package instead of expanding dependencies.
- Run every fixture through the same `PackageLoader` and vary only the discovered scope/source descriptor. Mark one fixture as bundled and the others as external candidates to make the no-shortcut property observable.
- Seed a lower-layer builtin contribution in the test harness, then prove explicit `replace: true` creates a higher layer and unload returns the original value. Prove failed activation removes every registration and event subscription.

**Tests / verification:**

- `bun test packages/iyon-plugins/tests/e2e-loader.test.ts packages/iyon-plugins/tests`
- Assert all required fixture scenarios, including package/extension-qualified errors, reverse disposal, async completion before commit, dependency import, Scene compose/replacement output, and unrelated-package continuation after failure.
- Assert the bundled and external fixture manifests produce identical activation/disposal behavior and that no fixture needs a native addon installation.
- `cargo test --workspace` and package typecheck/build commands.

**Must not:** hide fixture behavior behind a special test-only loader, use an `is_builtin` flag, add real product ports, introduce a new external dependency without approval, or make catalog metadata authoritative.

### Commit 8 — gate the extension runtime in CI

**Why:** Make the tranche’s lifecycle and boundary guarantees repeatable for later provider/tool/agent/app migrations. CI should verify Rust remains green while the new TypeScript path is checked independently.

**Files:**

- `.github/workflows/ci.yml`
- `package.json`
- `packages/iyon-plugins/package.json`

**Work:**

- Add or extend the repository CI workflow with separate Rust and Bun jobs. The Rust job runs `cargo fmt --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, and `cargo test --workspace`; the Bun job installs the lockfile-defined workspace, typechecks `packages/iyon-plugins`, and runs all plugin unit/integration/e2e tests.
- Add stable root/package scripts for the exact checks so later tranches can invoke them without reproducing CI-specific paths. Keep Bun and Rust jobs independent so a TypeScript fixture failure is diagnosed separately from native regressions.
- Add a lightweight source assertion in the Bun test/check path that the extension loader has no `is_builtin` execution shortcut and that all fixture package entrypoints pass through the virtual-module initialization guard.
- Keep lockfiles/dependency manifests unchanged unless a prerequisite T1–T5 package setup already requires the script wiring; do not use this commit to upgrade dependencies or add a sandbox.

**Tests / verification:**

- Run locally exactly the CI commands: `cargo fmt --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo test --workspace`, the package typecheck, and `bun test packages/iyon-plugins/tests`.
- Assert CI exercises the bundled fixture and external fixtures through the same test target and fails on any Rust, type, activation, rollback, or dependency-resolution regression.
- `test -f tranche_6_bun_refactor.md` is the documentation check for this planning tranche; implementation CI must not alter this document.

**Must not:** migrate product behavior, loosen Rust warnings, add dependency upgrades, make CI skip fixture packages, or treat passing unit tests as permission to bypass the shared loader.
