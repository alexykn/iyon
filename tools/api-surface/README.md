# `api-surface`

`api-surface` is the stable-Rust, source-driven parity gate for the four public
library targets in this workspace: `iyon-api`, `iyon-core`, `iyon-tui`, and the
`iyon` compatibility library. It uses Cargo metadata and `syn`; rustdoc JSON is
not required.

## Commands

Run the checked-in baseline from the repository root:

```sh
cargo run -p api-surface -- scan --config tools/api-surface/surface.toml
cargo run -p api-surface -- check --config tools/api-surface/surface.toml
```

`scan` resolves the configured library targets, evaluates the selected profile,
walks source modules and re-exports, and writes the canonical manifest, SDK
declarations, coverage report, and mapping report. `check` rescans in memory,
compares the schema/content hash, joins every public path and alias to a
disposition, and exits nonzero for missing, stale, signature, target, feature,
cfg, or re-export drift. Its output is:

```text
reachable: N
mapped:      N
missing:     0
stale:       0
```

The repository baseline is configured in `surface.toml`. Paths in that file are
relative to the configuration file, and generated paths are intentionally
workspace-relative so artifacts do not contain machine-local absolute paths.
The current baseline declares the default profile. A later tranche that adds a
feature or cfg profile must add it to the configuration and run the same scan,
check, and freshness workflow; an unavailable declared profile is a
configuration error, not an omitted matrix entry.

## Reachability and mappings

The scanner starts at a selected library root. It tracks module accessibility
separately from declaration visibility, resolves nested `pub use`, aliases,
private-module re-exports, and globs to a fixed point, and retains canonical
identity separately from every public spelling. It also records public fields,
enum variants, trait members, inherent members, and trait implementation
projections. Inactive cfg items remain source diagnostics but do not enter the
active surface.

Each reachable path is explicitly mapped in `mappings/*.toml`. The only valid
strategies are:

`MirrorValue`, `LazyValue`, `NativeHandle`, `NativeSync`, `NativeAsync`,
`TraitAdapter`, `TsFacade`, and `CompatibilityProjection`.

The strategy records the semantic crossing and later implementation owner. A
`stub` status describes a contract only; it does not claim that a native addon
or runtime implementation exists. There is no Ignore or Unsupported success
path, and aliases must be emitted as individual mapping records.

## Generated files and later API changes

Do not hand-edit generated files under `packages/iyon-sdk/generated`. When a
later tranche changes a public Rust item:

1. update the intentional mapping/disposition;
2. run `scan` with the affected profile;
3. review the canonical manifest and mapping/coverage reports;
4. run `check` and the SDK typecheck; and
5. commit the regenerated artifacts with missing=0 and stale=0.

`iyon:plugins` is deliberately only a declaration stub in this tranche. Kernel,
real N-API bindings, lazy View behavior, plugin loading, and product behavior
remain owned by later tranches.

## Stable and nightly tooling

The required CI workflow uses stable Rust and the source scanner. A nightly
rustdoc-JSON comparison may be run as an optional local diagnostic, but it is
not part of the normal scan/check path and does not gate parity.
