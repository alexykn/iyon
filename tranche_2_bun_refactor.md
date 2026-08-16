# Tranche 2 — Public API scanner and binding contract

## Mission

Create the stable, source-driven contract between the four current Rust crates and the future Bun/TypeScript SDK. The tranche adds a stable-Rust API-surface scanner based on syn and cargo_metadata, resolves the externally reachable API rather than every pub token, records every reachable path and alias in a canonical manifest, and generates the SDK declaration, binding metadata, coverage, and mapping reports that later native and TypeScript tranches must consume. The scanner is the parity gate: a reachable Rust item without a deliberate TypeScript disposition, or a stale disposition for a removed or changed item, fails the build.

## Prerequisites

T0 has replaced the obsolete architecture documentation and frozen the migration invariants. T1 has already provided the Bun workspace, TypeScript configuration, the iyon-runtime/iyon-cli viability path, iyon-native smoke addon, and the iyon:* virtual-module resolution path. T1 need not provide real Rust bindings; T2 may publish generated declaration stubs and binding dispositions for capabilities that T4 and T5 implement later.

The current Rust workspace and its public inventories are the baseline to scan:

~~~
iyon-api
iyon-core
iyon-tui
iyon
~~~

The iyon-*.md inventories are migration inputs and review oracles only. They must not be parsed or treated as the scanner implementation. The iyon package scan targets its library target; private binary startup details in crates/iyon/src/main.rs are not externally reachable library API unless a future command explicitly requests a binary target.

The scanner must run on stable Rust. Rustdoc JSON is optional nightly cross-checking only and is not a prerequisite for the normal command or CI.

## Invariants (do not violate)

- The architecture handoff is decisive: this is a Bun + TypeScript + N-API migration. Do not introduce WASM, Rhai, WIT, an embedded JavaScript engine, or a Rust-owned process.
- Rust dependency direction remains iyon-api → iyon-core; iyon-tui remains independent of both; iyon-native is the later bridge. tools/api-surface is a build-time scanner and must not become a runtime/application layer.
- The scanner computes external reachability from each selected crate root. A pub item behind an inaccessible private path is not reachable merely because its declaration is public; a private implementation module can still contribute an item reached through a public re-export.
- Preserve every externally reachable path, including canonical definitions, re-export paths, aliases, nested re-exports, glob-expanded names, and trait/inherent projection paths. Do not collapse aliases into one display path.
- The scanner must account for public and private modules, pub use, pub use through private modules, nested and glob re-exports, aliases, struct fields, enum variants and variant fields, free functions, constants, statics, traits, associated items, inherent methods, trait implementation projections, cfg, and feature selection.
- Manifest identity is stable and path-aware. A source location alone is not an API identity; a renamed item, re-exported item, signature change, visibility change, or cfg/feature availability change must be observable as drift.
- Every reachable item has a binding strategy. The allowed strategies are MirrorValue, LazyValue, NativeHandle, NativeSync, NativeAsync, TraitAdapter, TsFacade, and CompatibilityProjection. There is no permanent Ignore or Unsupported disposition.
- A semantic TypeScript projection is complete coverage when literal Rust representation is inappropriate. The generator must not require one N-API export for every Rust method; for example, a lazy View.bold facade is a valid mapping.
- T2 records declarations and dispositions; it does not implement kernel, TUI, plugin, provider, or application behavior. Generated declarations may be stubs, but they must describe the contract and identify the later implementation owner.
- Generated output is deterministic: stable ordering, normalized signatures, normalized paths, explicit schema version, and no machine-local absolute paths in committed artifacts.
- CI must report and enforce:

  ~~~
  reachable Rust APIs: N
  mapped TS APIs:      N
  missing:             0
  stale:               0
  ~~~

- A normal check must fail on a new reachable Rust API without a mapping, a removed Rust API with a stale mapping, signature drift, re-export drift, or cfg/feature drift.
- Do not modify ARCHITECTURE.md, iyon-api.md, iyon-core.md, iyon-tui.md, or iyon.md as part of T2. Do not revive any filename or artifact from the aborted Rhai effort.

## Out of scope

- KernelSession, kernel refactoring, provider-independent execution semantics, or changes to iyon-core public API. T3 owns those changes and will re-run this scanner and update mappings.
- Any real N-API implementation of iyon-api, iyon-core, or iyon-tui; those belong to T4 and T5.
- Lazy View implementation, native TUI handles, terminal runtime integration, or bridge optimization.
- Plugin loading, transactional activation, contribution registration, package discovery, provider/tool/agent/app migration, and the iyon:plugins runtime. T6–T10 own those concerns.
- WASM, Rhai, WIT, a Rust-owned process, an embedded Bun runtime, a Rust HTTP/provider layer, JSON serialization of stateful TUI objects, or a second built-in extension loader.
- Rewriting or regenerating the hand-authored inventory documents. They remain input evidence, not generated scanner output.
- Adding compatibility mappings for future T3 names before they exist. The current manifest maps the current API; T3 updates it when it adds kernel types.
- Adding tests outside the scanner/SDK scope or changing unrelated Rust behavior.

## Adjacent tranche contracts

### Consumes from earlier tranches

T2 consumes T1’s Bun workspace and TypeScript package conventions, the existing iyon:* virtual-module resolution, and the native smoke/build commands. It consumes the current Cargo workspace, package names, library targets, feature declarations, and the four inventory documents as baseline evidence. It may depend on T1’s package scripts and TypeScript compiler, but must not assume any real native binding exists.

### Exports for later tranches

T2 exports the following stable artifacts and commands:

- tools/api-surface as a reusable stable-Rust scanner library and CLI.
- A versioned canonical manifest schema containing crate, target/profile, item kind, canonical identity, every reachable path/alias, normalized signature, source span, availability, and binding disposition.
- Generated per-crate declarations for iyon:api, iyon:core, and iyon:tui, plus a small SDK declaration surface for iyon:plugins.
- Binding implementation metadata that tells T4/T5 whether an item is expected to be a value mirror, lazy facade, native handle, native sync/async operation, trait adapter, TypeScript facade, or compatibility projection.
- Generated coverage and mapping reports, with zero missing and zero stale entries for the current baseline.
- A check command and CI job that later tranches must keep green after re-running the scanner and updating their intentional mappings.

### Must not steal from neighboring tranches

T2 must not implement any native operation just to make a generated declaration compile, must not define the KernelSession contract early, must not implement View laziness, and must not add plugin-loader or product behavior. A generated declaration can use a named placeholder type or CompatibilityProjection and record the later owner; it must not hide that work as an ignored item.

## Commits

### Commit 1 — Scaffold the stable API-surface workspace tool

**Why:** Establish a buildable scanner package and a narrow CLI/library boundary before adding reachability logic. Registering the tool in the Cargo workspace makes it available to every later tranche without coupling it to a runtime crate.

**Files:**

~~~
Cargo.toml
Cargo.lock
tools/api-surface/Cargo.toml
tools/api-surface/src/lib.rs
tools/api-surface/src/main.rs
tools/api-surface/src/error.rs
~~~

**Work:**

- Add tools/api-surface to the Cargo workspace members.
- Add the required scanner dependencies through workspace dependency declarations where appropriate: syn with the full AST/visit features needed for item and attribute inspection, cargo_metadata, serde, serde_json, toml, and the existing error/CLI dependencies. Do not add a runtime dependency from any product crate to the scanner.
- Define the initial library boundary (scan, check, and output modules can be private initially) and a typed ApiSurfaceError that carries package, source path, item path, or configuration context. Errors must be returned to the CLI; no catch-all or silent fallback path is allowed.
- Define the CLI shape without pretending to complete the scan: scan accepts a workspace manifest/config, selected package/target, feature profile, and output directory; check accepts the same inputs plus the checked-in generated artifacts. The initial implementation may return a clear “scanner not configured” error until later commits add the subcommands, but the crate and help text must compile.
- Keep the command usable through cargo run -p api-surface -- ... so CI does not need a separate installed binary.

**Tests / verification:**

- cargo fmt --check.
- cargo check -p api-surface.
- cargo test -p api-surface with a smoke assertion that the CLI parses --help and rejects an unknown package/configuration with a nonzero typed error.
- cargo build --workspace to ensure the workspace registration does not change existing crate behavior.

**Must not:**

- Add a second workspace or a Rust executable that owns application startup.
- Add API declarations, N-API bindings, generated artifacts, or a dependency from iyon-* crates to api-surface.
- Parse iyon-*.md files at runtime.

### Commit 2 — Model scan profiles, Cargo targets, and cfg evaluation

**Why:** Reachability is only meaningful for a concrete package target and feature/cfg profile. This commit makes input selection explicit and gives the later resolver a deterministic module/source graph.

**Files:**

~~~
tools/api-surface/src/lib.rs
tools/api-surface/src/model.rs
tools/api-surface/src/metadata.rs
tools/api-surface/src/cfg.rs
tools/api-surface/src/parse.rs
tools/api-surface/tests/metadata.rs
tools/api-surface/tests/cfg.rs
tools/api-surface/tests/fixtures/cfg-reachability/Cargo.toml
tools/api-surface/tests/fixtures/cfg-reachability/src/lib.rs
tools/api-surface/tests/fixtures/cfg-reachability/src/private.rs
~~~

**Work:**

- Add typed schema primitives in model.rs: CrateId, TargetId, ApiItemId, ApiKind, ApiPath, SourceSpan, Visibility, Availability, ScanProfile, RustTarget, and normalized RustSignature. Keep fields serializable and use ordered collections for deterministic output.
- Implement CargoMetadataLoader in metadata.rs using cargo_metadata with --no-deps semantics for the selected workspace packages. Resolve package names to library targets, source roots, declared features, default features, and dependency package names. Reject ambiguous or missing library targets instead of silently selecting a binary.
- Make the profile explicit: package, library target, selected features, default-feature choice, target triple, and explicit --cfg values. Capture the resolved feature set and target cfg values in the manifest header so a changed profile is drift, not an invisible scanner behavior change.
- Implement cfg parsing/evaluation for feature = "...", target predicates, all, any, and not, retaining the original expression and its evaluated state. Unknown cfg keys must use the explicit profile/environment or produce a diagnostic; they must never be treated as unconditionally active. Preserve inactive items as source metadata while excluding them from the active reachable set for that profile.
- Implement SourceLoader/ModuleTree in parse.rs: load inline modules and external mod name; files using Rust’s name.rs/name/mod.rs conventions, retain module paths and spans, parse cfg/cfg_attr attributes, and report duplicate or missing module files with source context.
- Keep the parser source-based and stable-Rust compatible. Do not invoke rustdoc JSON or compiler-private APIs in the normal path.

**Tests / verification:**

- Unit-test profile evaluation for nested all/any/not, enabled and disabled features, target predicates, and an explicit cfg override.
- Fixture tests must show that inactive modules/items are retained as diagnostics but do not enter the active surface, while the same fixture under a feature profile does enter it.
- Test library-target selection against the four real package manifests and assert that iyon resolves to crates/iyon/src/lib.rs.
- cargo fmt --check, cargo test -p api-surface, cargo check --workspace.

**Must not:**

- Treat cfg(test) or test-only modules as product API in the default profile.
- Infer reachability from inventory markdown or from grep-like pub matching.
- Freeze a single feature profile as the permanent API. The profile must be part of the scan input and output so T3 can re-run it.

### Commit 3 — Resolve externally reachable Rust items and aliases

**Why:** This is the core correctness gate. It distinguishes the public API from declarations that merely contain pub, and it preserves the names downstream TypeScript users can actually import or call.

**Files:**

~~~
tools/api-surface/src/lib.rs
tools/api-surface/src/reachability.rs
tools/api-surface/src/normalize.rs
tools/api-surface/src/model.rs
tools/api-surface/tests/reachability.rs
tools/api-surface/tests/fixtures/reachability/Cargo.toml
tools/api-surface/tests/fixtures/reachability/src/lib.rs
tools/api-surface/tests/fixtures/reachability/src/private.rs
tools/api-surface/tests/fixtures/reachability/src/nested.rs
~~~

**Work:**

- Implement a module/item graph rooted at the selected library crate root. Track module accessibility separately from declaration visibility so a public item under a private parent can be reached through a public root re-export, while an otherwise public item with no reachable path is excluded.
- Resolve direct pub use entries, aliases (as), nested list imports, glob imports, and re-exports through private modules. Expand globs against the active module export set, iterate until a fixed point, and detect/report cycles or ambiguous names with the contributing paths.
- Emit an item record for each externally reachable declaration and an alias/path record for every reachable spelling. Include public modules, type aliases, structs, tuple/unit/named struct fields, enums, variants and variant fields, free functions, const, static, traits, trait associated types/constants/methods, and their source spans and attributes.
- Walk inherent impl blocks and include public methods, associated functions, associated constants, and associated types only when the receiver type is externally reachable. Preserve generic parameters, where clauses, receivers, asyncness, unsafeness, constness, and return/error types in normalized signatures.
- Walk trait implementations and record implementation projections: the implemented trait path, receiver path, associated type/constant assignments, provided/default methods, and public methods callable through the trait. Represent Type::method, Trait::method, and Type as Trait relationships explicitly rather than flattening them into unrelated methods.
- Resolve local type paths sufficiently to connect re-exported definitions and impl receivers; retain external dependency paths as typed signature references without attempting to scan dependency crates not selected as inputs.
- Normalize paths and signatures without losing Rust meaning: stable separators, explicit generic/lifetime placeholders, canonical whitespace, stable field/variant ordering, and a distinction between aliases and canonical definition identity.
- Make all reachability decisions explainable in an optional trace object: root path, re-export chain, visibility boundary, cfg decision, and impl/trait projection that made an item reachable.

**Tests / verification:**

- Fixture coverage must include a private parent with a public root re-export, nested re-exports, aliases, a glob re-export, a private pub declaration that stays absent, struct and enum field/variant paths, free functions, const/static, traits and associated items, inherent methods, and trait implementation projections.
- Assert that every alias is retained, that canonical and alias IDs are linked, and that no private-only item is emitted.
- Run the scanner against each current crate and compare representative paths with the corresponding inventory document without parsing the document. The test should assert known examples such as iyon_api::ModelApi, iyon_core::ids::SessionId, iyon_tui::View, and iyon::tui::build_app are present with their aliases/projections where applicable.
- cargo fmt --check, cargo test -p api-surface, cargo check --workspace.

**Must not:**

- Count every source declaration beginning with pub as API.
- Expand derived trait implementations into invented Rust methods.
- Treat private module declarations, binary main, test-only code, or dependency APIs as part of the selected crate’s external surface.
- Modify any current crate source to make the scanner easier to write.

### Commit 4 — Emit the canonical manifest and deterministic reports

**Why:** Later binding work needs one machine-readable source of truth, not separate handwritten lists for declarations, coverage, and mappings. This commit makes scanner output reviewable before adding SDK generation.

**Files:**

~~~
tools/api-surface/src/lib.rs
tools/api-surface/src/render.rs
tools/api-surface/src/check.rs
tools/api-surface/src/model.rs
tools/api-surface/tests/manifest.rs
tools/api-surface/tests/fixtures/manifest/Cargo.toml
tools/api-surface/tests/fixtures/manifest/src/lib.rs
~~~

**Work:**

- Define the versioned ApiManifest schema. It must include schema version, scanner version, crate/package/target identity, source root, scan profile, active cfg/features, ordered items, ordered paths/aliases, normalized signatures, availability, source spans, reachability traces when requested, and a stable content hash for each item.
- Distinguish canonical item identity from path identity. A re-export can share the canonical item ID while retaining its own public path, alias spelling, visibility, and source location. A signature or availability change changes the relevant hash.
- Add JSON serialization with stable ordering and a schema-version field. Use explicit enums for item kinds and strategy/disposition fields; reject unknown schema values when reading generated artifacts.
- Generate the following reports from the same manifest and no independent discovery pass:
  - per-crate declaration data for iyon-api, iyon-core, iyon-tui, and iyon;
  - a coverage report with reachable count, mapped count, missing paths/items, stale mappings, aliases, and profile details;
  - a mapping report containing every item/path, strategy, TypeScript module/export path, implementation owner, and notes;
  - a human-readable summary with the required N/N/0/0 counters.
- Add scan output and check comparison modes. scan writes only the configured output directory; check computes a fresh in-memory result and compares it to checked-in artifacts, failing on missing, stale, signature, path/alias, target, feature, or cfg drift.
- Keep output paths configurable, but provide the repository configuration used by later commits. Never embed absolute workspace paths in generated JSON.

**Tests / verification:**

- Snapshot or structural tests must prove deterministic output across repeated scans and prove that alias order, item order, and JSON formatting are stable.
- Mutate fixture inputs in test-only temporary copies to demonstrate each failure class: new reachable item, removed item, changed signature, changed re-export alias, and changed feature/cfg profile.
- Validate generated JSON against the Rust schema by deserializing it back and reject a mismatched schema version.
- cargo fmt --check, cargo test -p api-surface, cargo build --workspace.

**Must not:**

- Generate declarations by scraping markdown or by maintaining a second item inventory.
- Make coverage pass by excluding an item, adding an ignore bucket, or counting only canonical definitions while dropping aliases.
- Make rustdoc JSON a required dependency of scan or check.

### Commit 5 — Add the explicit binding-disposition contract

**Why:** Reachability alone does not prevent an ad-hoc TypeScript API. This commit makes every current Rust capability choose a deliberate binding strategy and records the later implementation owner without implementing that owner.

**Files:**

~~~
tools/api-surface/src/binding.rs
tools/api-surface/src/lib.rs
tools/api-surface/src/check.rs
tools/api-surface/mappings/iyon-api.toml
tools/api-surface/mappings/iyon-core.toml
tools/api-surface/mappings/iyon-tui.toml
tools/api-surface/mappings/iyon.toml
tools/api-surface/tests/binding.rs
~~~

**Work:**

- Define a versioned binding record keyed by stable canonical item ID plus public path where necessary. Each record includes strategy, Rust path, TypeScript virtual module, TypeScript export path, implementation owner tranche, status (stub/planned/implemented as metadata only), and a concise semantic note.
- Implement strict loading and validation of the four TOML mapping files. Require an entry for every active reachable item/path, allow one strategy record to cover aliases only when each alias is still emitted explicitly, and reject duplicate or ambiguous mapping keys.
- Enforce the finite strategy set: MirrorValue, LazyValue, NativeHandle, NativeSync, NativeAsync, TraitAdapter, TsFacade, and CompatibilityProjection. Reject Ignore, Unsupported, empty strategies, and unknown future strings in the current schema.
- Assign current baseline dispositions according to ownership: literal protocol/value types can use MirrorValue; stateful TUI values and handles use LazyValue/NativeHandle; future bridge calls use NativeSync/NativeAsync; Rust traits use TraitAdapter; product-facing semantic shapes use TsFacade; non-literal current APIs use CompatibilityProjection with an explanation. Provider types remain recorded because they are current public API, while T7 later removes product use in stages.
- Keep implementation metadata honest. A stub disposition says which later tranche implements it; it does not claim a native export already exists. For example, current View methods may be mapped to a lazy TsFacade/LazyValue projection and owned by T5 without adding a View implementation here.
- Have check join manifest paths with mappings and produce precise missing/stale diagnostics, including the alias path and strategy owner.

**Tests / verification:**

- Unit-test all valid strategy values and rejection of Ignore, Unsupported, duplicate paths, missing entries, stale paths, and invalid TypeScript module names.
- Add a fixture mapping that deliberately uses a semantic projection and assert it counts as mapped coverage while retaining its explanation.
- Run the current four-crate scan with the mapping files and assert missing = 0 and stale = 0 before declaration generation.
- cargo fmt --check, cargo test -p api-surface, cargo build --workspace.

**Must not:**

- Implement any N-API method, Rust kernel/session type, lazy View object, plugin loader, or provider migration.
- Add a permanent escape hatch that turns an unrepresented item into success.
- Treat a generated TypeScript stub as evidence that its runtime behavior exists.

### Commit 6 — Create packages/iyon-sdk and generate virtual-module declarations

**Why:** Give editors and later TypeScript tranches a checked-in, generated contract immediately, while keeping runtime implementation ownership in T4/T5/T6 and later.

**Files:**

~~~
packages/iyon-sdk/package.json
packages/iyon-sdk/src/index.ts
packages/iyon-sdk/src/virtual-modules.d.ts
packages/iyon-sdk/generated/iyon-api.d.ts
packages/iyon-sdk/generated/iyon-core.d.ts
packages/iyon-sdk/generated/iyon-tui.d.ts
packages/iyon-sdk/generated/iyon-plugins.d.ts
tools/api-surface/src/tsgen.rs
tools/api-surface/src/lib.rs
tools/api-surface/src/render.rs
tools/api-surface/tests/tsgen.rs
~~~

**Work:**

- Add the SDK package using T1’s existing Bun workspace conventions. Export the generated declaration entry point and make the package usable by editor/type-check tooling without requiring a native addon at development time.
- Generate declarations from the canonical manifest and binding records, never from handwritten per-crate lists. Preserve every Rust public alias as a TypeScript export path or named compatibility projection. Emit stable import-safe names and explicit placeholder types where Rust generics, lifetimes, trait objects, or callbacks need a semantic projection.
- Generate or include the four virtual modules with these exact declarations:

  ~~~ts
  declare module "iyon:api" { /* generated declarations */ }
  declare module "iyon:core" { /* generated declarations */ }
  declare module "iyon:tui" { /* generated declarations */ }
  declare module "iyon:plugins" { /* T1-compatible type stub only */ }
  ~~~

- Keep iyon:plugins as a declaration stub/extension contract owned by later runtime work; do not add package discovery, activation, registration, or privileged built-in behavior here.
- Ensure generated declarations distinguish value-like exports, lazy semantic values, native-handle placeholders, async return types, trait adapter shapes, and compatibility projections. A declaration must not imply that a method is a direct N-API symbol when its mapping strategy says it is a facade.
- Make packages/iyon-sdk/generated/ reproducible from the scanner command and include a generated header with schema/profile metadata. Do not hand-edit generated declarations.

**Tests / verification:**

- Structural tests assert that all four module declarations exist, every mapped path has a declaration disposition, aliases are present, and no generated declaration contains an Ignore/Unsupported placeholder.
- Run the T1 TypeScript typecheck command against a small fixture importing iyon:api, iyon:core, iyon:tui, and iyon:plugins, including representative value, field, enum, method, async, trait-adapter, and compatibility-projection types.
- Run cargo test -p api-surface and the repository’s Bun SDK typecheck/build command; then cargo build --workspace.

**Must not:**

- Add runtime behavior, native addon loading, plugin activation, or a second declaration source.
- Claim that generated stubs are T4/T5 implementations.
- Change T1’s virtual-module resolver semantics; integrate with its existing package/module path.

### Commit 7 — Generate the current baseline artifacts and parity fixtures

**Why:** Close the tranche against the actual current Rust API, not only synthetic examples, and make the zero-missing/zero-stale exit criterion reviewable in the repository.

**Files:**

~~~
tools/api-surface/surface.toml
packages/iyon-sdk/generated/api-manifest.json
packages/iyon-sdk/generated/coverage.json
packages/iyon-sdk/generated/mapping-report.json
packages/iyon-sdk/generated/iyon-api.d.ts
packages/iyon-sdk/generated/iyon-core.d.ts
packages/iyon-sdk/generated/iyon-tui.d.ts
packages/iyon-sdk/generated/iyon-plugins.d.ts
tools/api-surface/tests/current_surface.rs
tools/api-surface/tests/fixtures/current-surface/**
~~~

**Work:**

- Add the repository scanner configuration listing exactly iyon-api, iyon-core, iyon-tui, and iyon, their library targets, the default profile plus the supported feature profile(s), mapping directory, and SDK output directory. Keep profile selection explicit and reproducible.
- Run the scanner over the current crates and check in the canonical combined manifest plus the generated per-crate declaration and report artifacts. Generated records must include all current reachable fields, variants, methods, associated items, aliases, and trait/inherent projections that the scanner finds.
- Add current-surface assertions around the known public shapes from the inventories: private-module root re-exports in iyon-api, public ids and tools paths in iyon-core, public projection/stream/text/presentation surfaces in iyon-tui, and the iyon::tui re-exports/functions/fields in iyon.
- Keep the inventory documents out of the runtime test path. Review tests can use explicit expected path assertions and the documents can be used by reviewers to inspect completeness.
- Add a generated-artifact freshness test that runs the same config in a temporary output directory and compares normalized JSON/declaration output with the checked-in artifacts.
- Record the actual baseline counts in the generated reports; do not hardcode a guessed number in the plan or tests. The important assertions are that the counts agree and missing/stale are zero.

**Tests / verification:**

- cargo run -p api-surface -- scan --config tools/api-surface/surface.toml.
- cargo run -p api-surface -- check --config tools/api-surface/surface.toml and assert the required report lines, including missing: 0 and stale: 0.
- cargo test -p api-surface --test current_surface and the full cargo test -p api-surface.
- Run the SDK typecheck/build command and cargo build --workspace.

**Must not:**

- Replace the checked-in baseline with a hand-curated list that omits difficult items.
- Add T3 KernelSession or other future public names to make the current baseline look forward-compatible.
- Update the four inventory markdown files to make a mismatch disappear.

### Commit 8 — Enforce API parity in CI and document the operational contract

**Why:** Make the scanner gate permanent so later tranches cannot silently grow an ad-hoc TypeScript surface or leave stale declarations behind.

**Files:**

~~~
.github/workflows/api-surface.yml
tools/api-surface/README.md
tools/api-surface/src/main.rs
tools/api-surface/src/check.rs
~~~

**Work:**

- Add a focused CI workflow that installs/builds the existing Rust and Bun toolchains, runs scanner unit/integration tests, runs the configured scan/check, runs the SDK typecheck, and builds the workspace. Use stable Rust for the required path.
- Make CI execute both the normal profile and the declared feature/cfg matrix. If a profile is intentionally unavailable on the runner, fail with a configuration error rather than silently dropping it.
- Ensure the check output prints reachable, mapped, missing, and stale counts and exits nonzero for any signature, alias/re-export, feature, or cfg drift. Keep nightly rustdoc JSON as an optional separate diagnostic job or documented local command; it must not gate the stable path.
- Document the scanner’s input/output contract, configuration, mapping strategy meanings, generated-file policy, stable-vs-nightly distinction, and the required workflow for T3+ when public Rust APIs change: re-run scanner, add/update an intentional disposition, regenerate declarations/reports, and keep missing/stale at zero.
- Keep the CLI’s exit codes and paths stable enough for later tranches and external CI callers; errors must identify the affected crate/item/path and the expected remediation.

**Tests / verification:**

- Validate the workflow YAML syntax and run its commands locally in the same order where the repository’s CI tooling permits.
- cargo fmt --check, cargo test -p api-surface, cargo build --workspace.
- Run the Bun SDK typecheck and the exact stable scanner check command from CI; confirm a clean working tree after regeneration.
- Review the generated report for reachable = mapped and missing/stale = 0, and ensure the optional nightly cross-check is not required for completion.

**Must not:**

- Make CI depend on a nightly-only rustdoc JSON command.
- Add unrelated repository workflows or change product release/deployment behavior.
- Weaken the gate by allowing unknown strategies, stale mappings, omitted aliases, or profile drift.

