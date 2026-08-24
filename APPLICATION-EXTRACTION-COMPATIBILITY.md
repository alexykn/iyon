# Temporary extraction compatibility paths

S2 creates the application repository without changing behavior. The following
TUI-owned paths are retained temporarily because the current workspace still
builds one mixed native addon and one mixed TypeScript runtime. This is not
shared ownership and must not become a permanent third architecture.

| Temporary path/surface | Canonical owner | Removal tranche |
|---|---|---|
| `crates/iyon-tui/**` | `alexykn/iyon-tui` | S5: replace local path with exact external crate revision/version, then delete local copy |
| TUI portions of `crates/iyon-native/**` | `alexykn/iyon-tui/crates/iyon-tui-native` | S5: retain only `iyon-core-native`; delete TUI modules/ABI/generated files |
| `packages/iyon-runtime/src/tui/**` | `@iyon/tui` | S5: consume exact external package and delete local subtree |
| TUI tests/benches under `packages/iyon-runtime/**` | `alexykn/iyon-tui` | S5: delete after app compatibility suite uses the external package |
| `tools/tui-abi`, `tools/tui-abi-gen/**`, `PERF-11-generated-abi-reference.md` | `alexykn/iyon-tui` | S5: delete after the app no longer builds the TUI addon |
| `tools/ownership/**` TUI snapshot and `docs/repository-separation/s0/**` | `alexykn/iyon-tui` canonical evidence | S5: replace with app-only external-consumer checks; retain only extraction provenance needed by this repository |
| TUI portions of root manifests, locks, scripts, and workflows | independently derived per repository | S3–S5: reduce as each local TUI dependency disappears |
| `IYON-TUI-REPOSITORY-SEPARATION-HANDOFF.md` | `alexykn/iyon-tui` canonical record | S5: retain a link or app-specific completion record; do not fork normative architecture |

Until removal:

- no new application code may import these internals;
- all TUI changes land in `alexykn/iyon-tui` first and are consumed through an
  exact revision or released package;
- local copies exist only to preserve the S2 behavior-neutral build.
