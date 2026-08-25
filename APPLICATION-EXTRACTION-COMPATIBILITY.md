# S5 application extraction compatibility record

S5 completed the temporary application-side compatibility paths created by S2.
The generic TUI is now consumed from `alexykn/iyon-tui` at exact revision
`e322f10dff490c1423d988982c0782c22774f85d`.

| Former temporary path | S5 result |
|---|---|
| `crates/iyon-tui/**` | Deleted; Rust uses the exact external `iyon-tui` git dependency |
| TUI portions of `crates/iyon-native/**` | Deleted; remaining addon is `crates/iyon-core-native` |
| `packages/iyon-runtime/src/tui/**` | Deleted; TypeScript uses exact external `@iyon/tui` |
| TUI tests/benches under `packages/iyon-runtime/**` | Deleted; authoritative TUI tests/benches remain in `alexykn/iyon-tui` |
| `tools/tui-abi/**`, `tools/tui-abi-gen/**`, `PERF-11-generated-abi-reference.md` | Deleted; ABI ownership is exclusively `alexykn/iyon-tui` |
| TUI ownership snapshots and S0 baseline evidence | Deleted from the application checkout; S2 extraction provenance remains at the repository root |
| TUI portions of root manifests, locks, scripts, and workflows | Root manifests/lock/scripts now resolve exact external pins; CI files were not modified per tranche instructions |
| `IYON-TUI-REPOSITORY-SEPARATION-HANDOFF.md` | Canonical copy remains in `alexykn/iyon-tui`; this file records application completion |

The application native artifact is `packages/iyon-runtime/native/iyon-core-native.node`.
The external package owns `@iyon/tui` loading and `iyon-tui-native.node` staging.
No local TUI source, generated ABI module, or shared `NativeAddon` contract is
retained.
