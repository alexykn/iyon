# PERF-11v3 — Bun 1.4 generated native View ABI, unified retained cache, and zero structural encoding

**Status:** authoritative implementation handoff; supersedes all earlier PERF-11 drafts  

> **Research authority:** The Bun 1.4 release details, FFI behavior, and performance data in this handoff come from current online research conducted immediately after the release. Treat those claims and measurements as authoritative inputs for this work; do not re-verify or second-guess them from model memory. Evaluation should focus on implementation correctness, end-to-end benchmark results, and whether the stated acceptance gates are met.

**Repository:** `alexykn/iyon`  
**Baseline branch:** `perf-refactor`

## Exact execution tranches

This experiment has **13 implementation tranches**. Each tranche below names the exact document steps or sections it contains. Do not infer tranche boundaries from prose elsewhere in the handoff.

| Tranche | Exact scope in this document | Required result before proceeding |
|---|---|---|
| **1** | Bun preflight: `§2.1–§2.4` and `STEP 0.1–STEP 0.5` — install/pin Bun 1.4.0, record `bun --revision`, qualify engine-native FFI, rerun controls, then create the generator crate/schema/output model | Bun interfaces and controls are validated under the exact pinned runtime; generator foundation is deterministic |
| **2** | `STEP 0.6–STEP 0.9` — generated Rust wrappers, generated Bun signatures, `buffer_length` lowering, and ABI conformance tests | Generated ABI calls pass layout, calling-convention, pointer, scalar, buffer, JIT, and failure tests |
| **3** | `STEP 0.10` and the first generated vertical slice from `§22 PERF-11.1` — `runtime_noop`, `render_ref`, spacer, text-layout patch, common patch, and release refs | The generated vertical slice works end to end and is benchmarked before broader semantic generation |
| **4** | `§22 PERF-11.2` — environment runtime and unified semantic/native cache | One runtime/cache serves direct, V2, V3, V4, and generated paths without identity or lifetime regressions |
| **5** | `§22 PERF-11.3` — exact/render and scalar generated calls | Exact identity and scalar retained edits work through generated FFI with zero structural encoding |
| **6** | `§22 PERF-11.4` — `PathRef`/lens system and depth-specialized path calls | Path validation, NodeId preservation, subtree cutoff, and cache recovery pass |
| **7** | `§22 PERF-11.5` — tiny JS backing, pending states, and lazy fusion | JS construction no longer rebuilds rich bridge nodes on the selected fast path |
| **8** | `§22 PERF-11.6` — native typed multi-edit transactions | Multi-edit staging, common-ancestor sharing, atomic commit, abort, and limits pass |
| **9** | `§22 PERF-11.7` — PersistentSeq, axis, and grid families | Wide replace/insert/remove remain persistent and logarithmic with no flattening |
| **10** | `§22 PERF-11.8` — native builders and small-arity cold constructors | New-graph construction is correct and competitive; large/unsupported graphs still use fallback |
| **11** | `§22 PERF-11.9` — cstring/buffer string variants, retained strings, and style atoms | Unicode, embedded NUL, ownership, and string-cost measurements are correct |
| **12** | `§22 PERF-11.10` and `§21` — unified exact/scalar/path/transaction/builder/V4 routing across every View-bearing boundary in `§21A` | The complete hybrid route is integrated without changing fallback or host-mutation semantics |
| **13** | `§22 PERF-11.11` and `§20` — authoritative decision benchmark, lifetime/memory audit, and final report | End-to-end gates decide whether the rewrite is adopted or discarded |

`§22 PERF-11.0` is the Bun 1.4 preflight included in Tranche 1. `§22 PERF-11.1` is deliberately split across Tranches 1–3 because its generator foundation, ABI conformance, and vertical slice have separate gates. `§22 PERF-11.12` is **not part of the experiment**: it is conditional production cleanup performed only after Tranche 13 passes.

The highest-risk boundaries remain isolated: Tranches 4, 8, 9, 12, and 13 require separate commits and verification. Related work may be performed in one implementation session only when the exact tranche checks above still run independently.

**Last public branch head inspected:** `4672d247ab6679e702855a06f9c661a97c903784` (`feat: final perf 10 docs`)  
**Required runtime baseline:** exact pinned Bun 1.4.0 release, matching `bun-types`, and exact recorded Bun revision  
**Primary objective:** remove ordinary JS-side structural encoding, command-page writing, and generic native decoding from retained View updates  
**Secondary objective:** move enough semantic construction into the native retained graph that total end-to-end latency is materially lower than PERF-7v2 direct, not merely lower after subtracting an accounting phase  
**Must retain:** every correctness, identity, caching, weak-lifetime, subtree-cutoff, persistent-sequence, recovery, full-schema, and atomic-host-mutation guarantee established from PERF-7v2 through PERF-10

---

## 0. Executive decision

PERF-10 proved that the native retained side is worth keeping, but the JS feed architecture is still wrong.

The measured shape supplied for PERF-10 is approximately:

```text
matched normal matrix vs PERF-7v2 direct

Total:           10.1% slower
Commit:          16.9% faster
Native:          49.5% faster
JS construction: 97.5% slower
Encoding:        about 9.1 us added on the matched subset
```

Subtracting the measured encoding phase suggests a small win, but not a decisive one. More importantly, moving the same work into a different timing bucket would not improve real performance.

The new architecture must make the work disappear.

The target retained path is:

```text
public immutable TypeScript View
        |
        | compact backing state, no bridge-node reconstruction
        v
one generated semantic C ABI call
        |
        | Bun 1.4 JSC-native FFI hot call
        | unboxed scalar arguments in registers
        | no generic command buffer
        v
native environment ViewRuntime
        |
        | resolve existing NativeRef / NodeId cache
        | apply exact semantic edit
        | path-copy Rust View / PersistentSeq
        | publish caches atomically
        v
host installs new root
```

For multi-edit updates, JavaScript must not build a generic edit packet. Instead:

```text
native edit transaction begin
    + one generated typed add-edit call per changed leaf
    + native trie construction
    + one native commit
```

Bun 1.4 changes the trade-off materially. Bun's engine-native FFI implementation moves marshalling into JavaScriptCore, promotes hot calls to direct native calls from JIT-compiled code, reports about 0.70 ns for a no-op call on the release benchmark machine, and adds `buffer_length` so pointer and byte length are captured from the same TypedArray at the call boundary.

Therefore:

> Generated interface breadth is now a performance tool. Prefer many monomorphic semantic functions over one compact runtime interpreter.

This document calls the production candidate **Native Shadow V3**.

---

# 1. Non-negotiable result gate

Native Shadow V3 is not successful merely because:

```text
encoding_ns == 0
```

It is successful only if end-to-end performance improves.

Required result:

```text
normal retained matrix:
    materially faster than PERF-7v2 direct and the local Bun 1.4 direct rerun

realistic weighted trace:
    >= 15% faster than the best prior complete candidate

common retained modes:
    no statistically credible regression > 3%

exact identity:
    no slower than the best existing V3/V4 exact-ref result

cold / rebuilt:
    preserve the best V3/V4 bulk path within 5%
```

Preferred result:

```text
SHARED_PATH and normal retained trace:
    >= 20% faster than local Bun 1.4 direct
```

Do not approve production based on a phase-subtraction estimate.

---

# 2. Bun 1.4 is a hard prerequisite

## 2.1 Pin exact version and revision

Update the workspace from both current loose/stale pins:

~~~text
`bun-types`: latest
FastShared `FAST_SUPPORTED_BUN_VERSION`: 1.3.11
~~~

to an exact release match.

Required repository changes:

```text
package.json
    "packageManager": "bun@1.4.0"
    "bun-types": "1.4.0"

.bun-version
    1.4.0

tools/bun-revision.txt
    exact output of `bun --revision`
```

CI must verify:

```bash
bun --version
bun --revision
```

and fail if either differs from the pinned compatibility tuple.

Do not accept:

```text
>= 1.4
latest
canary
```

for authoritative benchmark or production release builds.

## 2.2 Rerun every baseline under Bun 1.4

Bun 1.4 changes FFI and may also change ordinary JS/JIT behavior. Therefore old Bun 1.3 candidate numbers are historical only.

Before any Native Shadow decision, rerun from one clean source revision:

```text
direct
V2 packed
V3
V4
FastShared/PERF-10
```

under the exact same Bun 1.4 binary.

Do not compare:

```text
Native Shadow on Bun 1.4
vs
PERF-7v2 direct on Bun 1.3
```

as the production decision.

Historical PERF-7v2 remains useful for architectural context only.

## 2.3 Qualify the Bun 1.4 FFI assumptions

Before implementing the View API, add a tiny native fixture and verify:

```text
[ ] JIT is enabled
[ ] `linkSymbols` or `CFunction` works with pointers from the loaded N-API image
[ ] hot no-op call reaches the expected sub-5 ns range on the target machine
[ ] scalar u32/i32 calls stay monomorphic
[ ] `buffer` + `buffer_length` reports the same view pointer/byte length
[ ] `cstring` accepts a JS string and returns exact UTF-8 semantics for supported cases
[ ] invalid signature is caught by the ABI conformance test
```

Do not assume the release-note machine's 0.70 ns result transfers exactly to Iyon. Use it as the reason to test a call-dense design.

## 2.4 Use `bun:ffi` APIs from the pinned typings

Bun's current source documentation uses:

```ts
import { CFunction, linkSymbols } from "bun:ffi";
```

The release post may show convenience imports from `bun`. The implementation must compile against the exact pinned `bun-types` and use the API actually present there.

Do not maintain handwritten compatibility wrappers for multiple Bun FFI generations in the hot path.

---

## 2.5 Tranche 1 implementation record

Tranche 1 was completed on the `perf-refactor` branch. Bun 1.4.0 is installed side by side with the existing Bun installation at:

```text
$HOME/.bun/versions/bun-v1.4.0/bun
```

The repository pins and verifies that installation through:

```text
package.json                         packageManager: bun@1.4.0
.bun-version                         1.4.0
tools/bun-revision.txt               34cbb9a40b4bd1bd767d134a7065e66c2432a676
packages/iyon-runtime/scripts/check-bun-version.ts
```

The full `bun --revision` value for the pinned binary is:

```text
34cbb9a40b4bd1bd767d134a7065e66c2432a676
```

The Tranche 1 implementation is recorded in these commits:

```text
8b9ac93  perf(tui): bootstrap Bun 1.4 ABI generator
36f8091  bench(tui): record Bun 1.4 tranche baseline
ee12b51  build(tui): declare generated ABI policy metadata
afd2d39  bench(tui): record Bun 1.4 FFI probe
da9be23  fix(tui): align Bun 1.4 ABI tranche with handoff
01ae31d  bench(tui): rerun Bun 1.4 tranche controls
3e5b54c  build(tui): make ABI generation layout-safe
cfa15fb  bench(tui): refresh Bun 1.4 FFI qualification
2eb7569  fix(tui): model explicit Bun buffer length pairs
0fd00f1  bench(tui): pin final Bun 1.4 control revision
3173972  build(tui): retain ABI schema source spans
c3c7a49  build(tui): generate checked ABI wrapper shells
3c4fcb0  build(tui): validate generated native reference handles
827b2ab  build(tui): harden generated ABI validation
b349fbb  build(tui): pin generated Rust formatting
d14e9f4  test(tui): refresh ABI generator snapshot
7bf0d41  bench(tui): use full exact-identity sample count
97722e7  bench(tui): rerun exact Bun 1.4 identity controls
060d9c6  bench(tui): enforce exact identity iteration policy
0913858  bench(tui): enforce exact identity controls
```

The final Tranche 1 review also hardened the generated foundation: Rust output is formatted by the pinned Rust 1.97.1 `rustfmt` toolchain, with deterministic `prettyplease` fallback; schema validation rejects incompatible lowering/type pairs and missing buffer-used lanes; generated checked wrappers validate full 53-bit NodeId pairs, NativeRef ranges, pointer alignment, buffer capacity/element-width/used-count bounds, enum discriminants, cstring pointers, and panic containment; host-pointer imports and cstring lowering are generated for future slices. The current generated reference records generator BLAKE3 `fd3bcd32d6995e625fada939bf2fd398b6dac2ec14400458b75f612cdc4d0d6d`.

**Tranche 1 status: COMPLETE.** Bun preflight (`§2.1–§2.4`) and generator bootstrap (`STEP 0.1–STEP 0.5`) are implemented, generated artifacts are checked in and deterministic, and the required Bun 1.4 controls/FFI probes pass. Production Native Shadow routing remains intentionally disabled until later tranches.

## 2.6 Tranche 2 implementation record

Tranche 2 (`STEP 0.6–STEP 0.9`) was reviewed against the full handoff and completed in commit `1559191bab6d05c53384d7bda394438de69b446b`.

The review found and corrected four gaps in the local foundation: the generated Rust wrappers had only layout/count coverage, no conformance family was generated from the canonical schema, the real Bun probe exercised handwritten fixtures rather than generated conformance symbols, and the handoff still marked Tranche 2 as unstarted. The correction adds ten schema-driven conformance fixtures covering u8/u16/u32/i32/f32/f64, pointer, `buffer` + `buffer_length`, cstring, and a representative 16-scalar maximum arity. It emits native C functions, exact Bun `linkSymbols` signatures, C declarations, manifest/reference metadata, and generated tests from the same schema.

Generated wrapper tests now prove handwritten implementation delegation, null/handle/NodeId/enum failure values, buffer capacity/alignment/used-count failures, and the i32/u32 result conventions. The Bun 1.4 probe loads the generated conformance pointer table from the staged N-API image and passes every scalar, pointer, same-TypedArray `buffer_length`, Unicode cstring, JIT, and sub-5 ns no-op check. The clean probe record is captured at source revision `c4c213220f946fc3098b124ac13531b5c5e7f4b5`, with native artifact SHA-256 `28598f95a720d0963aa8f117a846101f92734f3605c466a51a8365e710cd206e`, schema BLAKE3 `d243e278b8f4640f3ae5de70c311edd1a444f7a8f6359fdf90aea70187aa9951`, and generator BLAKE3 `fd3bcd32d6995e625fada939bf2fd398b6dac2ec14400458b75f612cdc4d0d6d`.

**Tranche 2 status: COMPLETE.** The generated ABI and conformance calls pass layout, calling-convention, scalar, pointer, buffer, cstring, JIT, and failure tests. No semantic vertical-slice implementation or production Native Shadow routing is claimed; those begin with Tranche 3 (`STEP 0.10`).

The canonical ABI input and generator sources are located at:

```text
tools/tui-abi/view_abi.toml
tools/tui-abi-gen/Cargo.toml
tools/tui-abi-gen/src/main.rs
tools/tui-abi-gen/src/model.rs
tools/tui-abi-gen/src/validate.rs
tools/tui-abi-gen/src/render_rust.rs
tools/tui-abi-gen/src/render_typescript.rs
tools/tui-abi-gen/src/render_header.rs
tools/tui-abi-gen/src/render_manifest.rs
tools/tui-abi-gen/templates/generated_banner.txt
tools/tui-abi-gen/templates/generated_typescript_bindings_header.txt
tools/tui-abi-gen/templates/generated_typescript_calls_header.txt
tools/tui-abi-gen/templates/generated_c_header_preamble.txt
tools/tui-abi-gen/templates/generated_reference_header.txt
```

The checked-in generated ABI artifacts are:

```text
crates/iyon-native/src/generated/view_abi_types.rs
crates/iyon-native/src/generated/view_abi_exports.rs
crates/iyon-native/src/generated/view_abi_conformance.rs
crates/iyon-native/src/generated/view_abi_table.rs
crates/iyon-native/include/iyon_view_abi.h
crates/iyon-native/tests/generated_view_abi.rs
packages/iyon-runtime/src/tui/generated/view_abi.ts
packages/iyon-runtime/src/tui/generated/view_abi_conformance.ts
packages/iyon-runtime/src/tui/generated/view_calls.ts
packages/iyon-runtime/src/tui/generated/view_abi_manifest.json
packages/iyon-runtime/tests/generated/view_abi_layout.test.ts
packages/iyon-runtime/bench/generated/view_abi_cases.ts
PERF-11-generated-abi-reference.md
```

The Bun 1.4 control and FFI qualification records are separate from the generated ABI outputs:

```text
packages/iyon-runtime/bench/PERF-11-bun14-results.jsonl
packages/iyon-runtime/bench/PERF-11-ffi-probe.json
```

`PERF-11-ffi-probe.json` records the generated conformance function count (10), schema/generator hashes, clean source revision, native artifact hash, and passing unsigned/signed/floating scalar, maximum-arity, pointer, same-TypedArray buffer-length, cstring, JIT, and sub-5 ns checks.

The exact Bun 1.4 baseline record contains 651 JSONL records: 650 normal records across direct, packed/V2, V3, V4, and FastShared, plus one 100-operation synthetic trace. Normal cases use 50 warmups, 500 measured iterations, and 10,000 exact-identity iterations. Every record includes Bun `1.4.0`, revision `34cbb9a40b4bd1bd767d134a7065e66c2432a676`, `git_dirty: false`, and matching benchmark-source/native-artifact hashes. The final control run was captured at clean source revision `060d9c688953a9f909e8a118761188fb35d251cc`. Its benchmark-source SHA-256 is `56748dff51f0af681e94b896e6bbcca96fc7d6aa18d952886ac9c67ead93dd36`, and the timing native artifact SHA-256 is `637ccf6720726486b2c3cd789288e52f2ed5701d789673cf516afe2bed266e37`. The run used `PERF_COUNTERS=0`, `PERF_WARMUP=50`, `PERF_NORMAL=500`, `PERF_EXACT=10000`, the two sizes, thirteen selected workloads, five required patterns, and a separate 100-operation synthetic trace.

Regenerate or verify the artifacts with:

```bash
bun run generate:tui-abi
bun run check:tui-abi
```

`check:tui-abi` renders into a temporary directory and compares every generated artifact byte-for-byte with the checked-in files. The authoritative CI check also runs `git diff --exit-code` on the generated paths.

Documentation commits recording the handoff and review state are `1e56c92`, `ad82d5a`, `4715436`, `46889ff`, `31ee207`, `153eef2`, `5239a94`, `d14e9f4`, and the benchmark commits `7bf0d41`, `97722e7`, `060d9c6`, and `0913858`. Tranche 2 implementation is `1559191`, with final conformance evidence correction `c4c2132`.

---

# STEP 0 — Build the ABI generator before implementing fast functions

This step is mandatory. Do not hand-author a family of Rust exports and a separate family of Bun signatures and promise to generate them later. The first native fast function must already come through the generator.

The reason is performance as much as maintainability:

~~~text
one generic runtime operation format
    -> smaller handwritten source
    -> repeated runtime interpretation

one generated monomorphic function per semantic operation family
    -> more generated source
    -> less runtime work
    -> Bun 1.4 can specialize the exact hot call site
~~~

The generator owns ABI mechanics. Handwritten Rust owns semantic algorithms.

## STEP 0.1 — Create a dedicated Rust generator crate

Create:

~~~text
tools/tui-abi-gen/
    Cargo.toml
    src/main.rs
    src/model.rs
    src/validate.rs
    src/render_rust.rs
    src/render_typescript.rs
    src/render_header.rs
    src/render_manifest.rs
    templates/
~~~

Workspace package name:

~~~text
tui-abi-gen
~~~

Use a normal Rust binary, not a proc macro and not `build.rs`, for the top-level generation pass.

Reasons:

~~~text
can inspect multiple workspace schemas
can emit Rust + TypeScript + C + JSON together
can run in --check mode
can provide good source-located diagnostics
can be tested independently
avoids build.rs rewriting the working tree
~~~

Add a thin workspace command:

~~~json
{
  "scripts": {
    "generate:tui-abi": "cargo run -q -p tui-abi-gen -- generate",
    "check:tui-abi": "cargo run -q -p tui-abi-gen -- check"
  }
}
~~~

Authoritative CI check:

~~~bash
bun run generate:tui-abi
git diff --exit-code -- \
  crates/iyon-native/src/generated \
  crates/iyon-native/include \
  packages/iyon-runtime/src/tui/generated \
  packages/iyon-runtime/tests/generated
~~~

`check:tui-abi` should generate into a temporary directory, byte-compare every output, and print the first stale path. The `git diff` command is a second guard.

## STEP 0.2 — Use these generator libraries

Use the following Rust crates. Pin them through `Cargo.lock`; do not fetch code or templates during generation.

~~~text
serde + serde_derive
    deserialize the canonical schema

toml
    deserialize the strict human-authored schema

toml_edit
    preserve declaration order/source spans for diagnostics and explain output

indexmap with serde support
    deterministic declaration order where maps are useful

serde_path_to_error
    report the exact schema path that failed to deserialize

miette
    source spans, labels, and actionable diagnostics

thiserror
    generator error taxonomy

clap
    `generate`, `check`, `print-manifest`, and `explain <function>` commands

proc-macro2 + quote + syn
    construct and parse generated Rust tokens

prettyplease
    deterministic formatting of generated Rust syntax trees

askama
    compile-time checked templates for TypeScript, C headers, and Markdown reference output

blake3
    stable ABI/schema/signature hashes

insta
    snapshot-test representative generated outputs and diagnostics

static_assertions
    generated Rust size/alignment/discriminant assertions

bytemuck with derive
    only for generated pointer-free POD buffer structs

cargo_metadata
    resolve workspace/output paths without assuming the current directory
~~~

Do not use the following as the canonical generator:

~~~text
cbindgen alone
    cannot generate Bun signatures, semantic wrappers, or fallback tables

bindgen
    solves C-to-Rust import, not this multi-target source-of-truth problem

WIT/wit-bindgen
    introduces a component-model ABI that is not Bun's C ABI

FlatBuffers / Cap'n Proto / protobuf
    regenerate a runtime serialization layer, which is exactly what the warm path is removing

one-off regex over Rust source
    cannot express ownership, fallback, hotness, or JS lowering rules reliably
~~~

`cbindgen` may be run as a non-authoritative audit that the generated C header agrees with exported Rust layouts, but the canonical schema remains the source of truth.

## STEP 0.3 — Create one canonical schema

Create:

~~~text
tools/tui-abi/view_abi.toml
~~~

Do not make the existing packed wire constants the primary ABI schema. Import shared semantic enum values from `bridge-schema.json` where appropriate, but define direct-call ownership and lowering separately.

Top-level schema shape:

~~~toml
[abi]
name = "iyon_tui_view"
version = 1
semantic_schema = 1
minimum_bun = "1.4.0"
result_encoding = "u32_high_bit_status"

[[handle]]
name = "RuntimePtr"
rust = "*mut NativeViewRuntime"
typescript = "Pointer"
nullable = false
lifetime = "environment"

[[handle]]
name = "ViewRef"
rust = "u32"
typescript = "number"
valid = "1..0x7fffffff"
kind = "view"

[[enum]]
name = "WrapMode"
source = "bridge-schema.json#wrap"
repr = "u32"

[[function]]
name = "view_text_layout_patch_root"
family = "scalar_patch"
hotness = "critical"
implementation = "view_text_layout_patch_root_impl"
fallback = "v4"
return = "ViewRefResult"

[[function.arg]]
name = "runtime"
type = "RuntimePtr"
lowering = "ptr"

[[function.arg]]
name = "base"
type = "ViewRef"
lowering = "u32"

[[function.arg]]
name = "node_id_low"
type = "u32"
lowering = "u32"

[[function.arg]]
name = "node_id_high"
type = "u32"
lowering = "u32"

[[function.arg]]
name = "wrap"
type = "WrapMode"
lowering = "u32"

[[function.arg]]
name = "align"
type = "HorizontalAlign"
lowering = "u32"
~~~

The exact TOML model may differ, but the schema must express all of these concepts:

~~~text
ABI version
semantic schema version
minimum/qualified Bun versions
fixed-width primitive types
opaque environment/host pointers
kind-checked NativeRef types
enums and flags
POD buffer structs
function families
exact argument order
Bun FFI lowering per argument
ownership and borrow duration
thread-affinity requirement
hotness class
fallback operation
result encoding
maximum lengths/counts
whether a function may allocate native memory
whether a function may mutate host state
handwritten implementation symbol
arity specializations
benchmark registration
~~~

The schema is a build-time specification, not a runtime reflection format. Production must not iterate it.

## STEP 0.4 — Define lowering modes explicitly

Every generated argument must use one of a small set of lowering modes:

~~~text
u8 / u16 / u32 / i32 / f32 / f64
    unboxed scalar

node_id_pair
    two generated u32 parameters; halves are already stored in the JS backing

native_ref
    one u32, kind checked in native code

runtime_ptr / host_ptr
    one stable environment-owned opaque pointer

buffer
    Bun `buffer`; TypedArray/DataView only

buffer_length_of = "arg_name"
    Bun 1.4 `buffer_length`; caller passes the same view twice

buffer_used
    explicit used byte/element count when the reusable view capacity exceeds used data

cstring_ephemeral
    NUL-free string copied/consumed before return; never retained as a pointer

pod_slice
    `buffer` + generated `buffer_length` + POD element validation

status_only
    i32 status result

native_ref_result
    u32 high-bit encoded success/error result
~~~

Do not allow arbitrary FFI type strings in the schema. The generator maps semantic lowering modes to the exact pinned Bun 1.4 signature.

## STEP 0.5 — Generate these outputs

The generator must emit all of the following in one deterministic pass:

~~~text
crates/iyon-native/src/generated/view_abi_types.rs
    repr(C) structs/enums/constants/result encoding

crates/iyon-native/src/generated/view_abi_exports.rs
    extern C wrappers, catch_unwind shells, generated validation, handwritten impl calls

crates/iyon-native/src/generated/view_abi_table.rs
    static function table and descriptor

crates/iyon-native/include/iyon_view_abi.h
    external C reference header

packages/iyon-runtime/src/tui/generated/view_abi.ts
    exact Bun 1.4 linkSymbols/CFunction declarations; no rest arguments

packages/iyon-runtime/src/tui/generated/view_calls.ts
    fixed-arity success-path helpers and cold error/fallback helpers

packages/iyon-runtime/src/tui/generated/view_abi_manifest.json
    canonical signatures, layouts, hashes, capabilities

packages/iyon-runtime/tests/generated/view_abi_layout.test.ts
    TS layout/signature/conformance tests

crates/iyon-native/tests/generated_view_abi.rs
    Rust layout/discriminant/function-table tests

packages/iyon-runtime/bench/generated/view_abi_cases.ts
    generated boundary microbenchmark registry

PERF-11-generated-abi-reference.md
    generated human-readable inventory
~~~

The generator must include a banner in every generated file:

~~~text
DO NOT EDIT. Generated from tools/tui-abi/view_abi.toml
schema_blake3 = ...
generator_blake3 = ...
~~~

## STEP 0.6 — Keep semantic implementations handwritten

Generated wrapper:

~~~rust
#[unsafe(no_mangle)]
pub unsafe extern "C" fn iyon_view_text_layout_patch_root_v1(
    runtime: *mut NativeViewRuntime,
    base: u32,
    node_id_low: u32,
    node_id_high: u32,
    wrap: u32,
    align: u32,
) -> u32 {
    generated_catch_unwind(|| {
        let runtime = generated_runtime(runtime)?;
        let base = generated_view_ref(base)?;
        let node_id = generated_node_id(node_id_low, node_id_high)?;
        let wrap = generated_wrap(wrap)?;
        let align = generated_align(align)?;
        view_text_layout_patch_root_impl(runtime, base, node_id, wrap, align)
    })
}
~~~

Handwritten implementation:

~~~rust
fn view_text_layout_patch_root_impl(
    runtime: &mut NativeViewRuntime,
    base: ViewRef,
    node_id: u64,
    wrap: WrapMode,
    align: HorizontalAlign,
) -> FastResult<ViewRef> {
    // Semantic algorithm only.
}
~~~

Generated code must never contain View reconstruction policy beyond mechanical validation and delegation. Handwritten code must never repeat FFI signatures, Bun type mappings, or struct offsets.

## STEP 0.7 — Generate monomorphic Bun 1.4 call sites

Do not reproduce PERF-10's generic wrapper:

~~~ts
function callPointer(pointer: number, args: readonly string[]): (...args: number[]) => number
~~~

Do not use:

~~~text
rest arguments
spread
Reflect.apply
function-name maps
per-call signature construction
polymorphic argument shapes
returned object wrappers
~~~

Generated TypeScript should bind once:

~~~ts
export const symbols = linkSymbols({
  viewTextLayoutPatchRoot: {
    ptr: abi.viewTextLayoutPatchRoot,
    args: ["ptr", "u32", "u32", "u32", "u32", "u32"],
    returns: "u32",
  },
  viewAxisCreateBuffer: {
    ptr: abi.viewAxisCreateBuffer,
    args: ["ptr", "u32", "u32", "buffer", "buffer_length", "u32"],
    returns: "u32",
  },
} as const).symbols;
~~~

And expose fixed-shape helpers:

~~~ts
export function textLayoutPatchRoot(
  runtime: Pointer,
  base: number,
  idLow: number,
  idHigh: number,
  wrap: number,
  align: number,
): number {
  const result = symbols.viewTextLayoutPatchRoot(runtime, base, idLow, idHigh, wrap, align);
  return result < 0x8000_0000 ? result : coldResultFailure(result);
}
~~~

The success branch must not allocate.

## STEP 0.8 — Use Bun 1.4 `buffer_length` correctly

For a generated buffer pair:

~~~ts
symbols.viewAxisCreateBuffer(
  runtime,
  idLow,
  idHigh,
  children,
  children,
  usedWords,
);
~~~

The first `children` supplies the pointer through `buffer`. The second supplies the byte length through `buffer_length`. Native validates:

~~~text
reported byte capacity >= usedWords * 4
usedWords <= operation-specific maximum
pointer alignment matches u32/POD alignment
usedWords is a multiple of the generated record width
all refs/enums/ranges are valid
~~~

Do not pass `children.byteLength` as a separate JS number when `buffer_length` can bind length to the same object.

## STEP 0.9 — Generate ABI conformance tests before semantic tests

Copy the principle from Bun's engine-native FFI conformance tests: use position-weighted arguments so a calling-convention or argument-order error cannot accidentally pass.

Generate native functions such as:

~~~c
uint32_t iyon_abi_conformance_u32_8_v1(
    uint32_t a0,
    uint32_t a1,
    uint32_t a2,
    uint32_t a3,
    uint32_t a4,
    uint32_t a5,
    uint32_t a6,
    uint32_t a7
);
~~~

Return a deterministic noncommutative combination. Test every supported scalar class, pointer/buffer pair, result mode, and representative maximum arity before invoking View semantics.

Run conformance under:

~~~text
interpreter/warmup tier if available
hot DFG tier
hot FTL tier
release build
ASan/debug native build
macOS arm64
Linux x64 before production adoption
~~~

## STEP 0.10 — First generated vertical slice

Generate and implement only these first:

~~~text
runtime_noop
render_ref
view_spacer_create
view_text_layout_patch_root
view_common_patch_root
release_refs_buffer
ABI conformance functions
~~~

This slice proves:

~~~text
Bun 1.4 engine-native binding
runtime pointer lifetime
result encoding
NodeId publication
unified Rust cache
lease release
zero structural encoding
~~~

Do not generate the full schema until this slice beats the current direct and FastShared controls in its intended cases.

---

# 3. Freeze the complete correctness contract before changing transport

Create:

```text
PERF-11v2-INVARIANTS.md
```

and turn every item below into a test, counter assertion, or compile-time assertion.

The new transport is invalid if any item is removed merely because the old mechanism no longer fits.

---

# 4. PERF-7v2 guarantees that must remain

## 4.1 Full semantic NodeId domain

Keep:

```text
1 .. 2^53 - 1
```

Reject:

```text
0
negative
fractional
NaN
Infinity
2^53 and above
```

NodeId and NativeRef remain different identities.

## 4.2 Environment-local semantic cache

The canonical semantic cache remains:

```rust
NodeId -> WeakView
```

owned by one N-API/Bun environment lifetime.

Required behavior:

```text
lookup NodeId
    -> live WeakView:
           return existing View

    -> expired WeakView:
           remove stale entry
           reconstruct authoritative payload/edit
           insert new WeakView
```

Under no circumstances may Native Shadow delete this cache architecture.

If direct-native Views no longer arrive as bridge payloads, the same cache must be repopulated by direct constructors and edit functions.

## 4.3 Same cache for every transport

These must publish into the same semantic cache:

```text
direct structured decoder
V2
V3
V4
Native Shadow generated constructors
Native Shadow path edits
Native Shadow edit transactions
```

No per-host semantic cache.

No benchmark-only semantic cache.

No transport-specific strong cache pretending to be equivalent.

## 4.4 Weak lifetime and recovery

The semantic cache remains weak.

A native acceleration handle may hold an explicit JS lease, but that is ownership corresponding to a live JS wrapper, not a forever cache.

When all strong owners disappear:

```text
WeakView may expire
semantic cache lookup may miss
transport must recover correctly
```

## 4.5 Atomic host mutation

Every complete operation must stage before changing the host.

On failure:

```text
no root replacement
no History mutation
no ViewSlot mutation
no ScrollPane mutation
no terminal state change
```

Retry/recovery occurs before one final host mutation.

## 4.6 Retry exactly once

On a recoverable stale NativeRef/cache generation:

```text
fast call returns CACHE_MISS
JS performs one V4/full native recovery
operation completes once
```

A second cache miss after authoritative cold recovery is a hard bug.

## 4.7 Exact identity O(1)

Repeated installation of the exact same immutable root must remain:

```text
O(1)
```

Preferred behavior:

```ts
if (root === lastInstalledRoot && !forceRedraw) return;
```

Otherwise:

```text
one generated render_ref FFI call
one NativeRef lookup
```

No packet.

No path walk.

No strings.

## 4.8 Stable subtree cutoff

A changed ancestor with a huge stable subtree must not visit stable descendants.

The size of the stable subtree must not affect changed-path work.

---

# 5. PERF-8 guarantees that must remain

## 5.1 Persistent sequence semantics

Wide row/column/grid updates remain persistent path-copy operations.

Required complexity:

```text
replace one child in N-wide parent:
    O(log_B N)
```

Do not convert a direct native edit into:

```rust
old.children.to_vec()
```

or any equivalent flatten/copy.

## 5.2 Native retained representation

The Rust View graph remains immutable and structurally shared.

A patch creates a new semantic View.

It never mutates the old NodeId's meaning.

## 5.3 Lineage

Keep the semantic knowledge required to identify:

```text
base native View
changed field/leaf
changed parent path
changed PersistentSeq path
```

Do not recompute lineage by diffing two arbitrary complete trees after construction.

## 5.4 Multi-edit ancestor sharing

When several changed leaves share ancestors, each changed ancestor must be rebuilt once.

The new Native EditTxn must build the changed-path trie in Rust rather than making JS serialize one.

---

# 6. PERF-9/PERF-10 gains that must remain

## 6.1 V3/V4 bulk transport remains available

Keep the best validated full-schema materializer for:

~~~text
cold or rebuilt closure where direct construction loses
unsupported generated operation
cache generation recovery
non-Bun or unqualified-runtime fallback
correctness oracle and differential testing
~~~

Native Shadow must publish V3/V4 results into the same semantic cache and NativeRef table used by generated calls. V3/V4 is a bulk construction algorithm, not a separate retention architecture.

## 6.2 PERF-10's native gains must be preserved

PERF-10 demonstrated substantially lower native and commit time. Do not throw away:

~~~text
thread-affine native execution
native-owned retained Views
paged/dense ref lookup
fixed native PersistentSeq path-copy
exact-ref rendering
counter-free timing build
single host mutation
~~~

The new design removes the JS command compiler and generic op interpreter around those gains.

## 6.3 Eliminate the current FastShared encoder rather than hiding it

The following current work must disappear from common retained updates:

~~~text
FastSharedEncoder recursive compileView/compileSequence walk
WeakMap state lookup for every changed object
control.fill / meta.fill
new Uint32Array view per emitted op
10-word opcode record writes
payload callback and FastMetaWriter writes
local-wire-ref encoding
operation-count/header publication
native generic opcode dispatch
~~~

Moving any of that work into Rust or renaming it `construction` is not elimination.

## 6.4 JavaScript remains the semantic NodeId authority

Do not silently replace PERF-7v2's semantic identity with a different native-only identity.

Each immutable JS semantic View receives a full safe-integer NodeId exactly once. Its backing stores:

~~~ts
nodeIdLow: number;
nodeIdHigh: number;
~~~

at construction time. Generated calls pass those two cached u32 values. They must not split the Number again on every materialization.

Native `NativeRef` is a separate acceleration/lease handle. It never replaces NodeId.

## 6.5 Function identity replaces opcode encoding

Common operation:

~~~text
view_text_layout_patch_root(...)
~~~

not:

~~~text
emit OP_PATCH_TEXT
emit destination
emit base
emit mask
native switch(opcode)
~~~

Generated source volume is allowed. Runtime interpretation is not.

## 6.6 Scalars first

The hottest generated calls use only:

~~~text
runtime pointer
host pointer where required
u32 NativeRefs
cached NodeId low/high u32
small enum/flag scalars
small fixed arity child refs
~~~

No BigInt, returned struct, JS object, rest argument, or temporary TypedArray belongs in a one-leaf retained update.

## 6.7 Use a one-word success result

Generated create/patch functions return `u32`:

~~~text
0x00000001 .. 0x7fffffff
    successful NativeRef

0x80000000 .. 0xffffffff
    encoded error/fallback status
~~~

The success path reads one Number and takes one predictable branch. Detailed diagnostics live in a runtime status cell and are read only on failure.

Host-only functions return `i32` status.

## 6.8 Path descriptors are retained native objects

A common deep edit must not rebuild `PathStep[]` in JavaScript.

Create immutable environment-local `PathRef` objects once and store them in View lineage sidecars. A `PathRef` contains expected kinds and selectors; it owns no View and therefore does not keep a semantic graph alive.

Hot path topology is passed as one `PathRef`, but semantic NodeIds still belong to JS.

For root edits, pass one cached NodeId pair. For shallow path edits, generate depth-specialized scalar calls carrying one NodeId pair per rebuilt JS View. For deep edits, use bottom-up direct materialization or a native typed edit transaction.

No path or NodeId buffer is encoded at commit.

## 6.9 Buffers are reserved for genuinely variable payloads

Use Bun 1.4 `buffer` + `buffer_length` only for:

~~~text
large child-ref lists
large splice inputs
multi-span text descriptors
Diff hunks/lines
grid tracks/cells
batched lease releases
rare multi-edit topology
~~~

Generate scalar arity specializations for common small cardinalities before selecting a buffer path.

---

# 7. Environment-global NativeViewRuntime

A `View` can be created before a host exists and can be shared across hosts in one environment. Therefore the canonical native store cannot be per-host.

Create one environment runtime:

```rust
struct NativeViewRuntime {
    // PERF-7v2 semantic cache. Meaning must remain unchanged.
    semantic_cache: HashMap<u64, WeakView>,

    // Dense acceleration/JS-lease tables.
    view_slots: NativeViewSlotTable,
    sequence_slots: NativeSequenceSlotTable,
    style_slots: NativeStyleSlotTable,

    path_store: PathStore,
    builder_store: BuilderStore,
    edit_txn_store: EditTxnStore,

    next_native_ref: u32,
    generation: u32,
    owner_thread: ThreadId,
    alive: AtomicBool,
    status: FastStatusCell,
}
```

N-API environment initialization creates it once and registers the cleanup hook.

Every host stores a handle/reference to the same environment runtime.

## 7.1 Replace the current separate FastSession cache

The current PERF-10 implementation has its own per-host `FastSession` containing its own `nodes` map and `FastSlotTable`, while direct/V3/V4 use the environment `ViewBridgeCache`.

That split is forbidden in Native Shadow V3.

Migration:

~~~text
current environment ViewBridgeCache.semantic nodes
+ current V3/V4 packed slots
+ current FastSession nodes/slots
    -> one NativeViewRuntime
~~~

Delete transport ownership from the cache types. Generated constructors, direct structured decode, V2, V3, V4, History, ViewSlot, ScrollPane, and every host all resolve/publish through one runtime.

The cache implementation may be refactored, but these semantics may not change:

~~~text
NodeId -> WeakView
expired weak removal
full safe-integer NodeId
same environment lifetime
one cold recovery
no all-time strong cache
~~~

## 7.2 Use a stable environment-owned runtime pointer on the Bun hot path

The maximum-performance Bun 1.4 signature should pass:

~~~text
NativeViewRuntime* as `ptr`
~~~

not a static 1024-entry session-handle lookup on every call.

Lifetime contract:

~~~text
N-API environment initialization allocates Box<NativeViewRuntime>
its address never changes
N-API bootstrap returns the pointer and generated function table
host disposal does not free the environment runtime
environment cleanup marks runtime dead, tears down hosts, then frees runtime
no FFI call is legal after environment teardown
~~~

If the project already owns N-API instance data, extend that environment state. Otherwise use `napi_set_instance_data`/the napi-rs equivalent. If that conflicts with another module, the existing cleanup-hook registry may own the Box, but it must be consulted only at bootstrap; generated hot calls use the stable pointer directly.

Runtime header fields:

~~~rust
#[repr(C)]
struct NativeViewRuntimeHeader {
    magic: u32,
    abi_version: u32,
    semantic_version: u32,
    alive: AtomicU32,
}
~~~

Generated checked builds validate magic/version/alive/thread. Timing builds retain the minimum alive/thread checks required for safe lifetime use.

## 7.3 Use a stable host pointer for host-mutating calls

`NativeTuiHost` already boxes its `TuiHost`, giving a stable address. Generate host-mutating functions with:

~~~text
NativeViewRuntime* runtime
NativeHost* host
~~~

The host allocation remains alive until its JS/N-API object is finalized. `dispose()` tombstones the host and closes terminal state; it must not leave a dangling pointer that can be reused by a later object.

Exact render target:

~~~c
int32_t iyon_host_render_ref_v1(
    NativeViewRuntime *runtime,
    NativeHost *host,
    uint32_t view_ref
);
~~~

This removes the global session table and per-call host/cache discovery.

---

# 8. Preserve and extend Rust caching correctly

## 8.1 Semantic cache stays weak

Keep:

```rust
semantic_cache: HashMap<NodeId, WeakView>
```

Every successfully created native View is inserted after semantic validation.

When the View dies:

```text
WeakView expires
lookup removes stale entry
```

## 8.2 NativeRef table adds leases without replacing weak caching

A JS `View` wrapper may refer to a standalone native View that no parent/host currently owns. It needs an explicit lease.

Use:

```rust
struct NativeViewSlot {
    node_id: u64,
    weak: WeakView,
    leased: Option<View>,
    js_lease_count: u32,
    kind: ViewKindTag,
}
```

Rules:

```text
js_lease_count > 0:
    leased contains strong View

js_lease_count == 0:
    drop leased
    keep weak acceleration entry
```

This is not a strong forever cache. The strong reference corresponds to a live JS native backing.

## 8.3 Host ownership and JS ownership are independent

If JS releases a root but a host/history/slot owns it:

```text
weak still upgrades
```

If nobody owns it:

```text
weak expires
```

## 8.4 Batched releases

Use `FinalizationRegistry` only to enqueue NativeRefs.

Do not call FFI from each finalizer.

Drain:

```text
before a later native call
at threshold
on microtask/idle callback
on runtime shutdown
```

Generated API:

~~~c
int32_t iyon_view_release_many_v1(
    NativeViewRuntime *runtime,
    const uint32_t *refs,
    size_t refs_capacity_bytes,
    uint32_t used_ref_count
);
~~~

Bind the pointer and capacity as Bun 1.4 `buffer` + `buffer_length`, pass the same reusable `Uint32Array` twice, and pass the used count separately.

Refs are not reused in one generation, preventing delayed release ABA.

## 8.5 Bulk paths publish into the same cache and slots

V3/V4 materialization must publish:

```text
NodeId -> WeakView
NativeRef -> same View/WeakView slot
```

There must not be one cache for generated ABI and another for packed bulk.

## 8.6 Generation reset

Transport/native-ref generation reset:

```text
may clear/refill NativeRef acceleration table
must not clear the semantic NodeId cache merely because ref numbering changed
```

Every JS backing stores the runtime generation beside NativeRef. On mismatch:

```text
lookup NodeId in the same semantic cache
    -> live: mint/lease a current-generation NativeRef
    -> expired/missing: one V3/V4 cold recovery
```

## 8.7 Audit cache threading before removing locks

The generated Bun path is thread-affine and should not take `Arc<Mutex<ViewBridgeCache>>` on every call. Do not remove synchronization by assumption.

Before refactoring:

~~~text
search every read/write of ViewBridgeCache
classify caller thread
prove all semantic cache mutation occurs on the environment owner thread
ensure background layout/paint holds strong View values rather than looking up cache entries
~~~

Target:

~~~text
JS-owner-thread NativeViewRuntime state:
    semantic cache
    NativeRef/PathRef/StyleRef tables
    builders/edit transactions
    no hot mutex

cross-thread retained/render state:
    immutable Arc-backed View values
    ordinary host synchronization already owned by TUI runtime
~~~

If a genuine concurrent cache caller remains, isolate it behind a cold-path lock or message rather than putting a mutex back into every generated call.

---

# 9. Make JavaScript View wrappers tiny and JIT-friendly

## 9.1 Remove rich BridgeViewNode construction from the hot path

The fast Bun path should not build:

```text
full BridgeViewNode object
cloned decoration objects
child wrapper objects
packed metadata objects
command records
```

Use a stable-shape private backing.

Recommended:

```ts
const kBacking = Symbol("iyon.view.backing");

class View {
  readonly kind = "view" as const;
  readonly [kBacking]: ViewBacking;

  private constructor(backing: ViewBacking) {
    this[kBacking] = backing;
    Object.freeze(this);
  }
}
```

Prefer this to a module-level `WeakMap<View, BridgeViewNode>` on the hot path. Measure private field vs symbol field; keep the faster stable monomorphic shape.

## 9.2 Backing states

```ts
type ViewBacking =
  | NativeBacking
  | PendingCreateBacking
  | PendingPatchBacking;

interface NativeBacking {
  readonly state: 0;
  readonly generation: number;
  readonly ref: number;
  readonly nodeIdLow: number;
  readonly nodeIdHigh: number;
  readonly path?: number;
}

interface PendingCreateBacking {
  readonly state: 1;
  readonly recipe: GeneratedCreateRecipe;
}

interface PendingPatchBacking {
  readonly state: 2;
  readonly base: View;
  readonly fused: GeneratedFusedPatch;
  readonly path?: number;
}
```

Do not use string tags in the production hot shape if numeric tags benchmark better.

## 9.3 Lazy fusion remains important

Bun 1.4 makes calls cheap; Rust allocations are not free.

This:

```ts
View.text("x")
  .bold()
  .foreground("green")
  .padding(1)
  .maxWidth(40)
```

must not necessarily create five native Views.

Fuse until a materialization boundary:

```text
PendingCreate Text
+ fused common fields
-> one generated native constructor
```

For an existing native View:

```text
NativeRef
+ padding
+ foreground
+ maxWidth
-> one fused native patch call
```

Benchmark immediate-native and lazy-fused candidates. The expected production winner is lazy fusion for fluent chains.

## 9.4 Do not encode fused patches

A fused patch is a stable JS object with scalar fields. The generated materializer directly passes those fields as C arguments.

Do not translate it into a command buffer first.

## 9.5 Native result metadata is lazy

A one-path edit may create a leaf and several ancestors. Return only the root NativeRef on the common path.

If a retained intermediate JS View later needs its ref:

```text
lookup by NodeId in the semantic cache
or materialize its own pending backing
```

Do not return and write an array of every ancestor ref unless a measured use case requires it.

---

# 10. Generated operation families

Generate families rather than one generic dispatcher.

## 10.1 Exact/render

```text
render_ref
force_render_ref
host_install_ref
```

## 10.2 Common scalar patches

```text
edit_text_layout_*
edit_padding_*
edit_size_rules_*
edit_bounds_*
edit_gap_*
edit_clamp_*
edit_common_fields_*
```

## 10.3 Child/path edits

```text
edit_container_child_*
edit_hanging_child_*
edit_axis_child_*
edit_axis_splice_*
edit_grid_cell_*
edit_diff_hunk_*
```

## 10.4 Style values

```text
style_create_bits
style_patch_bits
border_create_builtin
border_create_custom
style_state_atom
```

## 10.5 Direct constructors

```text
view_text_create_*
view_spacer_create
view_container_create
view_clamp_create
view_hanging_create
view_axis_create_*
view_grid_create_*
view_diff_create_*
view_component_create
```

## 10.6 Builder transactions

```text
build_begin
build_abort
build_text_begin
build_text_add_span_*
build_axis_begin
build_axis_add_child
build_grid_begin
build_grid_add_track
build_grid_add_cell
build_finish_view
build_commit_render
```

## 10.7 Multi-edit transactions

```text
edit_begin
edit_add_text_layout
edit_add_replace_text
edit_add_decoration
edit_add_axis_child
edit_add_axis_splice
edit_add_grid_cell
edit_add_diff_hunk
edit_commit_render
edit_abort
```

## 10.8 Lease/lifecycle

```text
lease_clone
release_many
runtime_flush_releases
```

## 10.9 First production ABI inventory

The generator should emit the following initial interface families. Names may be normalized, but signatures and lowering intent should remain equivalent.

### Runtime and host

~~~c
uint32_t iyon_runtime_noop_v1(NativeViewRuntime *runtime);

int32_t iyon_host_render_ref_v1(
    NativeViewRuntime *runtime,
    NativeHost *host,
    uint32_t view_ref
);

int32_t iyon_host_force_render_ref_v1(
    NativeViewRuntime *runtime,
    NativeHost *host,
    uint32_t view_ref
);
~~~

### Exact lookup and leases

~~~c
uint32_t iyon_view_ref_for_node_id_v1(
    NativeViewRuntime *runtime,
    uint32_t node_id_low,
    uint32_t node_id_high
);

int32_t iyon_view_release_many_v1(
    NativeViewRuntime *runtime,
    const uint32_t *refs,
    size_t refs_capacity_bytes,
    uint32_t used_ref_count
);
~~~

Bind `refs` as `buffer`, `refs_capacity_bytes` as `buffer_length`, and pass the same `Uint32Array` for both generated JS arguments.

### Scalar leaf/wrapper constructors

~~~c
uint32_t iyon_view_spacer_create_v1(
    NativeViewRuntime *runtime,
    uint32_t node_id_low,
    uint32_t node_id_high,
    uint32_t rows
);

uint32_t iyon_view_container_create_v1(
    NativeViewRuntime *runtime,
    uint32_t node_id_low,
    uint32_t node_id_high,
    uint32_t child_ref
);

uint32_t iyon_view_content_max_create_v1(
    NativeViewRuntime *runtime,
    uint32_t node_id_low,
    uint32_t node_id_high,
    uint32_t child_ref,
    uint32_t max_rows
);

uint32_t iyon_view_component_create_v1(
    NativeViewRuntime *runtime,
    uint32_t node_id_low,
    uint32_t node_id_high,
    uint32_t handle_low,
    uint32_t handle_high
);
~~~

### Text

Arbitrary text, safe variant:

~~~c
uint32_t iyon_view_text_create_utf8_v1(
    NativeViewRuntime *runtime,
    uint32_t node_id_low,
    uint32_t node_id_high,
    const uint8_t *bytes,
    size_t bytes_capacity,
    uint32_t used_bytes,
    uint32_t style_ref,
    uint32_t wrap,
    uint32_t align,
    uint32_t common_value_ref
);
~~~

Bind `bytes`/`bytes_capacity` as `buffer`/`buffer_length`. Rust copies exactly `used_bytes` once into final immutable native storage before return.

Optional NUL-free micro-optimized variant:

~~~c
uint32_t iyon_view_text_create_cstring_v1(
    NativeViewRuntime *runtime,
    uint32_t node_id_low,
    uint32_t node_id_high,
    const char *text,
    uint32_t style_ref,
    uint32_t wrap,
    uint32_t align,
    uint32_t common_value_ref
);
~~~

Route only strings proven NUL-free. Never retain the call-arena pointer.

Text layout patch, zero structural encoding:

~~~c
uint32_t iyon_view_text_layout_patch_root_v1(
    NativeViewRuntime *runtime,
    uint32_t base_ref,
    uint32_t node_id_low,
    uint32_t node_id_high,
    uint32_t wrap,
    uint32_t align
);

uint32_t iyon_view_text_layout_patch_path_v1(
    NativeViewRuntime *runtime,
    uint32_t base_root_ref,
    uint32_t path_ref,
    uint32_t node_id_low,
    uint32_t node_id_high,
    uint32_t wrap,
    uint32_t align
);
~~~

### Common-field patch

Generate a fixed scalar form rather than a payload blob:

~~~c
uint32_t iyon_view_common_patch_root_v1(
    NativeViewRuntime *runtime,
    uint32_t base_ref,
    uint32_t node_id_low,
    uint32_t node_id_high,
    uint32_t mask,
    uint32_t padding_tr,
    uint32_t padding_bl,
    uint32_t width_rule,
    uint32_t height_rule,
    uint32_t min_width,
    uint32_t max_width,
    uint32_t min_height,
    uint32_t max_height,
    uint32_t decoration_ref
);
~~~

Generate smaller specialized calls for common single-property edits if the broad call prevents optimal register/JIT lowering or performs measurably worse.

### Styles and decoration

~~~c
uint32_t iyon_style_create_bits_v1(
    NativeViewRuntime *runtime,
    uint32_t flags,
    uint32_t attribute_present,
    uint32_t attribute_true,
    uint32_t foreground_ref,
    uint32_t background_ref,
    uint32_t theme_atom_ref
);

uint32_t iyon_decoration_create_v1(
    NativeViewRuntime *runtime,
    uint32_t flags,
    uint32_t padding_tr,
    uint32_t padding_bl,
    uint32_t style_ref,
    uint32_t border_ref,
    uint32_t background_ref,
    uint32_t width_height_rules,
    uint32_t min_width,
    uint32_t max_width,
    uint32_t min_height,
    uint32_t max_height
);
~~~

### Axis/PersistentSeq

Small arity, generated for measured cardinalities 0 through 8:

~~~c
uint32_t iyon_view_column_create_2_v1(
    NativeViewRuntime *runtime,
    uint32_t node_id_low,
    uint32_t node_id_high,
    uint32_t gap,
    uint32_t track0,
    uint32_t child0,
    uint32_t track1,
    uint32_t child1
);
~~~

Variable input:

~~~c
uint32_t iyon_view_axis_create_buffer_v1(
    NativeViewRuntime *runtime,
    uint32_t node_id_low,
    uint32_t node_id_high,
    uint32_t axis_kind,
    uint32_t gap,
    const AxisChildInputV1 *children,
    size_t children_capacity_bytes,
    uint32_t used_child_count
);
~~~

Warm structural edit:

~~~c
uint32_t iyon_view_axis_set_child_v1(
    NativeViewRuntime *runtime,
    uint32_t base_axis_ref,
    uint32_t node_id_low,
    uint32_t node_id_high,
    uint32_t child_index,
    uint32_t track_word,
    uint32_t child_ref
);

uint32_t iyon_view_axis_set_child_path_v1(
    NativeViewRuntime *runtime,
    uint32_t base_root_ref,
    uint32_t path_ref,
    uint32_t node_id_low,
    uint32_t node_id_high,
    uint32_t child_index,
    uint32_t track_word,
    uint32_t child_ref
);
~~~

The Rust implementation uses the canonical retained PersistentSeq path-copy. It must not reconstruct a flat child array.

### Path interning

~~~c
uint32_t iyon_path_root_v1(NativeViewRuntime *runtime);

uint32_t iyon_path_child_v1(
    NativeViewRuntime *runtime,
    uint32_t parent_path_ref,
    uint32_t step_kind,
    uint32_t expected_view_kind,
    uint32_t selector
);
~~~

Path creation is amortized during JS lineage construction, not repeated during render.

### Native builders

~~~c
uint32_t iyon_axis_builder_begin_v1(
    NativeViewRuntime *runtime,
    uint32_t axis_kind,
    uint32_t expected_children
);

int32_t iyon_axis_builder_push_v1(
    NativeViewRuntime *runtime,
    uint32_t builder_ref,
    uint32_t track_word,
    uint32_t child_ref
);

uint32_t iyon_axis_builder_finish_v1(
    NativeViewRuntime *runtime,
    uint32_t builder_ref,
    uint32_t node_id_low,
    uint32_t node_id_high,
    uint32_t gap
);
~~~

Builder handles are mutable and short-lived. Finished Views are immutable.

### Multi-edit transaction

~~~c
uint32_t iyon_edit_txn_begin_v1(
    NativeViewRuntime *runtime,
    uint32_t base_root_ref,
    uint32_t expected_edit_count
);

int32_t iyon_edit_txn_add_text_layout_v1(
    NativeViewRuntime *runtime,
    uint32_t txn_ref,
    uint32_t path_ref,
    uint32_t node_id_low,
    uint32_t node_id_high,
    uint32_t wrap,
    uint32_t align
);

uint32_t iyon_edit_txn_commit_render_v1(
    NativeViewRuntime *runtime,
    NativeHost *host,
    uint32_t txn_ref
);

int32_t iyon_edit_txn_abort_v1(
    NativeViewRuntime *runtime,
    uint32_t txn_ref
);
~~~

With Bun 1.4, a handful of direct typed add calls is preferable to JS constructing a generic trie packet. Native builds the trie and clones common ancestors once.

---

# 11. Native path lenses eliminate remaining JS path encoding

## 11.1 Path creation

When a JS immutable operation creates lineage, derive a native PathRef once.

Examples:

```text
root path = 0
container child = path_child(root, CONTAINER_CHILD, 0)
column child 7 = path_child(root, COLUMN_CHILD, 7)
```

Store the PathRef in the new backing/lineage.

## 11.2 Path interning cost is amortized

`path_child` may use a native hash map keyed by:

```text
(parent PathRef, step kind, expected kind, selector)
```

The same structural location reuses the PathRef.

Benchmark interning on construction. If lookup is too expensive, allocate monotonic path nodes without interning and rely on JS sidecar reuse.

## 11.3 Single-edit call and NodeId preservation

The primary zero-encode route is bottom-up direct materialization:

~~~text
materialize changed leaf with its JS NodeId
materialize changed parent from native child refs with its JS NodeId
repeat to root
render root ref
~~~

Every call creates exactly one JS semantic View and therefore receives exactly one cached NodeId pair. This is the cleanest identity-preserving path under Bun 1.4.

A one-call path lens is allowed only when it also receives the NodeId of every JS semantic View it reconstructs. It may not invent native-only semantic IDs.

Generate scalar depth specializations for common depths. Example depth two shape:

~~~c
uint32_t iyon_edit_axis_child_d2_v1(
    NativeViewRuntime *runtime,
    uint32_t base_root_ref,
    uint32_t step0_kind,
    uint32_t step0_selector,
    uint32_t step1_kind,
    uint32_t step1_selector,
    uint32_t changed_axis_id_low,
    uint32_t changed_axis_id_high,
    uint32_t ancestor0_id_low,
    uint32_t ancestor0_id_high,
    uint32_t ancestor1_id_low,
    uint32_t ancestor1_id_high,
    uint32_t child_index,
    uint32_t track_word,
    uint32_t new_child_ref
);
~~~

This is intentionally a broad generated signature. Bun 1.4 can lower the monomorphic scalars directly; JavaScript writes no path or NodeId buffer.

For a deep path that would exceed the benchmarked scalar-arity limit, choose one of:

~~~text
bottom-up direct materialization, one semantic call per changed JS View
native EditPlanRef built incrementally by typed calls as JS constructs lineage
V3/V4 fallback for unusual large changed closure
~~~

Do not rebuild a `PathStep[]` or `NodeIdPair[]` during render.

## 11.4 Path validation

Native must validate:

```text
runtime generation
base ref live
path ref live
expected node kind per step
selector in range
axis/grid semantic constraints
```

Validation occurs before publication/host mutation.

## 11.5 Path cache does not retain Views

Path nodes contain only selectors/kinds/parent path refs.

They must not hold strong View references.

---

# 12. Multi-edit without JS encoding: native typed edit transaction

A realistic render can change several branches. Applying independent path edits may clone common ancestors repeatedly. The solution is a native typed transaction, not a JavaScript-encoded trie.

## 12.1 Begin

~~~c
uint32_t iyon_edit_txn_begin_v1(
    NativeViewRuntime *runtime,
    uint32_t base_root_ref,
    uint32_t expected_edit_count
);
~~~

Native holds a strong staged base root. No host or cache publication occurs.

## 12.2 Add every changed semantic node with its JS NodeId

Each generated function appends one typed semantic change to native transaction memory.

~~~c
int32_t iyon_edit_txn_add_text_layout_v1(
    NativeViewRuntime *runtime,
    uint32_t txn_ref,
    uint32_t path_ref,
    uint32_t node_id_low,
    uint32_t node_id_high,
    uint32_t wrap,
    uint32_t align
);

int32_t iyon_edit_txn_add_axis_child_v1(
    NativeViewRuntime *runtime,
    uint32_t txn_ref,
    uint32_t path_ref,
    uint32_t node_id_low,
    uint32_t node_id_high,
    uint32_t child_index,
    uint32_t track_word,
    uint32_t child_ref
);
~~~

If an ancestor receives a new semantic identity merely because one child changed, the corresponding JS parent View contributes its NodeId through a typed ancestor/axis/common edit call. Native never fabricates the identity of a JS View.

JavaScript performs no struct packing, op-record writing, path-array writing, or NodeId-array writing.

## 12.3 Native builds the changed-path trie

On each add, native stores:

~~~text
PathRef
operation family
NodeId
scalar payload / native refs
~~~

At commit:

~~~text
resolve path nodes
merge common prefixes
walk base graph only along changed edges
apply leaf/ancestor edits
rebuild each changed ancestor once
path-copy canonical PersistentSeq nodes
validate every supplied NodeId
~~~

## 12.4 Commit

~~~c
uint32_t iyon_edit_txn_commit_render_v1(
    NativeViewRuntime *runtime,
    NativeHost *host,
    uint32_t txn_ref
);
~~~

Success returns the new root NativeRef. Failure returns encoded status; no host mutation and no published destination refs.

## 12.5 Why multiple FFI calls are acceptable under Bun 1.4

The typed calls contain only pointers, u32 refs, cached NodeId halves, and scalar values. Engine-native FFI can promote hot monomorphic sites to direct C calls.

This is expected to be cheaper than:

~~~text
allocating JS edit arrays
writing generic records
encoding local indexes
serializing NodeId arrays
calling one generic decoder
~~~

The benchmark must prove it. The one-call-per-change design is rejected if native call volume outweighs the eliminated JS compiler work.

## 12.6 Transaction limits and lifecycle

Generate and enforce:

~~~text
MAX_EDIT_COUNT
MAX_CHANGED_PATH_DEPTH
MAX_STAGED_NATIVE_OBJECTS
MAX_NEW_TEXT_BYTES
~~~

On limit:

~~~text
return FAST_FALLBACK before publication
route complete current root through V3/V4
~~~

Abort uncommitted transactions on explicit error, host disposal, and environment cleanup.

---

# 13. Native builders can eliminate cold JS encoding too

PERF-10 encoding should not simply move to a different cold packet if direct builders are faster.

Benchmark a generated native builder architecture.

## 13.1 Map existing callback builders directly

Current public APIs such as:

```ts
View.vertical((column) => {
  column.child(a);
  column.child(b);
});
```

map naturally to:

```text
axis_builder_begin
axis_builder_add_child(a.ref)
axis_builder_add_child(b.ref)
axis_builder_finish
```

No JS child array and no later serialization are required.

## 13.2 Mutable native builders, immutable finished Views

Builders are transaction-local mutable objects.

Finished Views remain immutable.

On abort/failure, discard builder state.

## 13.3 Generate small-arity constructors

For common tiny structures, one call is better than a builder sequence.

Generate explicit variants:

```text
column_0
column_1
column_2
column_3
column_4
row_0 .. row_4
hanging_3
text_1_span
text_2_span
text_3_span
text_4_span
```

Arguments are scalar NativeRefs/style refs/cstrings where possible.

For larger arity:

```text
native builder
or buffer + buffer_length
or V4 bulk
```

The schema lists exact arity variants.

## 13.4 Call-dense cold candidate

Bun reports OpenTUI FFI-dense Yoga workloads improving substantially under the engine-native implementation. Therefore benchmark one native call per builder action instead of assuming it is too expensive.

## 13.5 Cold routing

Choose by measured estimate:

```text
small/moderate pending graph:
    generated native builder

large graph / huge text / unsupported:
    V4 bulk
```

Useful routing dimensions:

```text
new node count
builder call estimate
span count
string bytes
grid cell count
```

---

# 14. Strings: use Bun 1.4 features without hiding cost

## 14.1 Structural updates with no new text

Operations such as:

```text
wrap
alignment
padding
gap
size bounds
existing-child replacement
style ref replacement
```

must transfer no text and perform zero encoding.

## 14.2 Direct `cstring` variants

Bun 1.4 accepts a JavaScript string for a `cstring` argument and transcodes it in the engine's call arena.

Generate direct cstring variants for strings whose semantic domain forbids embedded NUL, for example if validated:

```text
theme keys
style-state keys/values
route names
some metadata atoms
```

For text payload, use a wrapper branch:

```text
no embedded NUL:
    cstring fast variant

contains embedded NUL:
    length-preserving buffer variant
```

Benchmark `indexOf("\0")` cost versus unconditional buffer encoding.

## 14.3 `cstring` is not zero-cost

Bun's own benchmark notes that passing a JS string re-encodes it on every call. Report this as text transcoding.

Do not claim it is zero-copy.

## 14.4 Buffer variants

For arbitrary strings including NUL:

```text
reusable UTF-8 scratch
buffer + buffer_length
one final copy into immutable native storage
```

No command/string table.

No temporary Rust `String` followed by another copy.

## 14.5 Specialized text arities

Generate:

```text
text_create_1_cstring
text_create_2_cstring
text_create_3_cstring
text_create_4_cstring

text_create_1_buffer
text_create_2_buffer
...
```

Each span includes a native StyleRef scalar.

For many spans, use native TextBuilder or V4 bulk.

## 14.6 Native retained string representation

Do not keep `Arc<dyn SharedUtf8Source>` per span as the final hot representation.

Use a compact value:

```rust
enum RetainedStr {
    Inline(InlineStr12),
    PageSlice {
        page: Arc<NativeUtf8Page>,
        offset: u32,
        len: u32,
        prefix: u32,
    },
    Owned(Box<str>),
    Static(&'static str),
}
```

Short strings become allocation-free.

Long strings share immutable native pages.

## 14.7 No writable JS alias retained as `&str`

Rust may only expose a lifetime-bound `&str` when bytes are guaranteed immutable for that lifetime.

A JS convention that it will not write again is insufficient.

For borrowed FFI input, copy once into final native immutable ownership unless a runtime-proven detach/revoke mechanism is implemented and tested separately.

## 14.8 Native atoms

Materialize repeated style/theme/border values once:

```text
StyleRef
BorderRef
ThemeAtomRef
DecorationRef where beneficial
```

Spans and Views pass u32 refs instead of repeated strings/objects.

Do not globally intern arbitrary user text.

---

# 15. Native-first semantic construction is the primary candidate

The principal candidate is not a commit-time path encoder. Rust becomes the realized semantic View as soon as a value crosses a natural construction boundary.

## 15.1 Hybrid eager construction with local fusion

Use this rule:

~~~text
fluent modifiers on a not-yet-materialized value
    -> fuse into a tiny fixed-shape PendingCreate/PendingPatch backing

child inserted into a parent builder
root rendered/pushed/stored
native-only query requested
    -> materialize through one generated semantic function

parent builder finish
    -> create native parent from native child refs
~~~

This preserves fusion without retaining a second rich JS View DAG.

Example:

~~~ts
View.text("status")
  .bold()
  .foreground("green")
  .padding(1)
~~~

becomes one pending fixed-shape recipe and one generated text/common-field creation call when consumed. It must not create four native intermediates and must not serialize four operations later.

## 15.2 Already-native semantic mutations

An already-native View mutation becomes a tiny `PendingPatch`:

~~~ts
{
  state: NATIVE_PATCH,
  baseRef,
  nodeIdLow,
  nodeIdHigh,
  commonMask,
  scalar fields,
  optional PathRef,
}
~~~

Materialization calls the exact generated patch function. No bridge-node clone, object spread, command page, or generic payload writer is permitted.

## 15.3 Builders materialize children at insertion

`ChildrenBuilder.child(view)` and equivalent APIs should materialize/finalize the child once, retain its NativeRef, and append the ref/track to the builder strategy.

Generate and benchmark three builder lowerings:

~~~text
arity-specialized parent constructors
    0..8 children as scalar refs/track words

native mutable builder
    begin / push child / finish immutable View
    likely best for callback builders and large lists under Bun 1.4

buffer constructor
    reusable Uint32Array + buffer_length
    control for medium/large flat lists
~~~

The selected production path may vary by cardinality. None may re-encode complete child View payloads.

## 15.4 Immediate-native control candidate

Keep an immediate-native benchmark control:

~~~text
View.text -> native create
padding -> native immutable patch
foreground -> native immutable patch
~~~

Bun 1.4 makes call overhead tiny, but repeated Rust allocation can still lose. This control quantifies the value of local fusion.

## 15.5 Encoding accounting requirement

For all operations that introduce no new JS string/blob data:

~~~text
structural_encoding_ns = 0
bytes_written_to_transport = 0
op_records_written = 0
~~~

Native construction/patch time remains in construction/native timing. The benchmark must not relabel a command compiler as construction.

## 15.6 Remove the duplicate production JS graph after adoption

During differential development, retain the current BridgeViewNode path behind a test/fallback feature.

After Native Shadow wins and V3/V4 can recover from compact recipes:

~~~text
remove rich BridgeViewNode from ordinary production View backing
remove FastSharedEncoder from production
remove per-transport WeakMaps
retain direct oracle only in tests
~~~

---

# 16. Native algorithms

## 16.1 Single-path edit

```rust
fn edit_path(
    runtime: &mut NativeViewRuntime,
    base_root: View,
    path: PathRef,
    edit: TypedEdit,
) -> Result<View, FastError> {
    let mut frames = SmallVec::<[PathFrame; 16]>::new();
    let mut current = base_root;

    for step in runtime.path_store.iter(path) {
        let (frame, child) = descend_checked(current, step)?;
        frames.push(frame);
        current = child;
    }

    let mut rebuilt = apply_typed_edit(runtime, current, edit)?;

    for frame in frames.into_iter().rev() {
        rebuilt = frame.replace_child_persistent(runtime, rebuilt)?;
    }

    Ok(rebuilt)
}
```

No unchanged subtree traversal.

## 16.2 Multi-edit commit

```text
collect typed edits by PathRef
construct path trie from native path nodes
validate all paths and payloads
apply leaves
rebuild bottom-up
publish cache/refs once
install root once
```

## 16.3 Persistent axis edit

```text
resolve RetainedAxis
PersistentSeq::set/splice
clone O(log_B N) sequence nodes
create new axis View
```

## 16.4 Grid edit

```text
resolve retained grid cell sequence
path-copy changed cell path
share tracks and unchanged cells
create new grid View
```

## 16.5 Common-field patch

Operate directly on Rust common View fields.

Do not reconstruct a `Decorated` wrapper packet if Rust semantically stores decoration on the View node.

---

# 17. Publication and cache atomicity

Every one-shot edit or edit transaction follows:

```text
1. validate runtime and handles
2. resolve all base refs
3. validate paths and payloads
4. construct new immutable Views/sequences in local staging
5. validate NodeId collisions against semantic cache
6. allocate NativeRefs
7. prepare cache publication
8. install/render root at the chosen atomic point
9. publish refs/cache consistently
10. return root ref
```

If host installation can fail, choose one of:

```text
install before exposing refs
or
rollback-capable publication
```

Never leave:

```text
published refs without host commit
partially updated host
partially sealed builder/edit transaction
```

---

# 18. Safety bounds

## 18.1 Pointer and handle safety

Use two different identity classes deliberately.

~~~text
NativeViewRuntime* / NativeHost*
    stable environment/host-owned opaque pointers
    passed as Bun `ptr`
    never derived from a movable Rust object
    lifetime ends only after JS/N-API teardown makes further calls impossible

ViewRef / PathRef / StyleRef / BuilderRef / EditTxnRef
    u32, zero invalid, kind checked, environment-local
~~~

Do not expose pointers to individual Rust Views, sequences, styles, or builders. Only the stable runtime/host roots cross as pointers.

Generated checked builds validate the runtime header and owner thread. Production timing builds retain the minimum checks required to reject a disposed runtime/host without reintroducing a global lookup.

## 18.2 Function pointers

N-API returns exact pointers once.

Bootstrap verifies:

```text
ABI magic
version
manifest hash
function count
nonzero pointer
struct size/alignment
capability bits
```

Call a generated probe before enabling the table.

An invalid FFI pointer can crash the process; treat bootstrap validation as mandatory.

## 18.3 No callbacks on the hot path

Generated View functions never call JavaScript.

They are synchronous and bounded.

## 18.4 Leaf-call discipline

Model functions after leaf FFI principles:

```text
short
non-blocking
no JS runtime calls
no callbacks
no waits
no terminal polling
```

Split long work:

```text
fast semantic construction
separate render scheduling/layout if necessary
```

## 18.5 Panic policy

No Rust panic may unwind across C ABI.

Generated wrappers use checked conversions and panic-free semantic functions.

Generate two wrapper modes:

~~~text
checked/debug/counter build:
    catch_unwind at the C ABI edge
    convert panic to FAST_INTERNAL
    full diagnostics

fast timing/production build:
    no catch_unwind in the hot wrapper
    semantic implementation is audited panic-free
    panic = "abort" is acceptable for impossible invariant failure
~~~

Expected input errors, cache misses, fallback, allocation failure, and disposed handles must always return statuses and may never rely on panic.

## 18.6 Buffer safety

For every generated buffer input:

```text
pointer/length supplied through buffer + buffer_length
length <= configured maximum
length multiple of element size
alignment verified or unaligned-safe reader used
no retained pointer after call unless native owns/copies memory
```

Use `zerocopy` or explicit byte decoding for unaligned slow paths. Use aligned typed arrays for hot POD lanes.

## 18.7 Limits

Generate constants and enforce them on both sides:

```text
MAX_PATH_DEPTH = 128
MAX_EDIT_COUNT = 256
MAX_BUILDER_CHILDREN = 1_000_000
MAX_SPANS = 1_000_000
MAX_STRING_BYTES_PER_CALL = configured bounded maximum
MAX_TXN_STAGED_OBJECTS = configured bounded maximum
```

Oversized inputs return fallback/invalid before mutation.

## 18.8 Fuzzing

Fuzz handwritten semantic entry points with generated valid/invalid inputs.

Targets:

```text
path traversal
edit transaction merge
PersistentSeq splice
handle lifetime/release
string variants
cache miss/recovery
ABI buffer validation
```

Run ASan/LSan/UBSan-capable builds where supported.

---

# 19. Timing and diagnostics

## 19.1 Phases

Record separately:

```text
js_api_construction_ns
js_fusion_ns
text_transcode_ns
ffi_call_ns
native_semantic_ns
native_publish_ns
host_commit_ns
total_ns
```

For scalar/native-ref retained edits:

```text
structural_encoding_ns = 0
```

Do not create an encoding timer around scalar argument reads merely to preserve an old schema.

## 19.2 Timing build

Compile out:

```text
per path-step counters
per ref counters
per node counters
per sequence-node counters
atomic word/op counters
```

## 19.3 Structural build

Count:

```text
paths resolved
View nodes constructed
PersistentSeq leaves/branches cloned
stable nodes visited
cache hits/misses
lease upgrades/releases
builder calls
FFI calls
V4 fallbacks
```

Run few iterations.

## 19.4 Verify no hidden JS encoding

Add JS counters:

```text
command_words_written
command_buffers_touched
path_arrays_written
node_id_arrays_written
```

For direct scalar/PathRef retained cases all must be zero.

---

# 20. Benchmark program under the 1800-second limit

Split the decision by question.

## 20.1 PERF-11.0 — Bun 1.4 rebaseline

```text
candidates:
    direct
    V2
    V3/V4
    FastShared

sizes:
    20
    200

modes:
    IDENTICAL_IDENTITY
    SHARED_PATH
    LARGE_SHARED_SUBTREE_CUTOFF
    SHARED_DEEP
    REBUILT_EQUIVALENT
```

Warmup 50, measured 500 for normal, 10,000 exact.

## 20.2 PERF-11.1 — FFI boundary

At least 1,000,000 iterations after JIT warmup:

```text
no-op
u32 -> u32
4 scalar args
8 scalar args
16 scalar args
PathRef edit stub
buffer + buffer_length
cstring short ASCII
cstring Unicode
reusable UTF-8 buffer
```

Verify call-site warmup and inspect first-use separately.

## 20.3 PERF-11.2 — generator vertical slice

```text
render_ref
text layout root
text layout depth 1..4
text layout PathRef
```

Compare:

```text
V2
FastShared
scalar generated
PathRef generated
```

## 20.4 PERF-11.3 — JS construction

Measure:

```text
current BridgeViewNode
immediate native
lazy fused native
```

Workloads:

```text
plain text
styled text
fluent decoration chain
row/column builder
mixed realistic
```

The new design must not repeat PERF-10's ~2x JS construction regression.

## 20.5 PERF-11.4 — retained matrix

```text
SHARED_PATH
SHARED_DEEP
LARGE_SHARED_SUBTREE_CUTOFF
TEXT_METADATA_PATCH
DECORATION_PATCH
```

Sizes 20 and 200; selected 2,000 cases.

## 20.6 PERF-11.5 — multi-edit

Edits per render:

```text
2
4
8
16
64
```

Compare:

```text
separate one-shot path calls
native edit transaction
FastShared one command batch
V4
```

## 20.7 PERF-11.6 — wide parent

Widths:

```text
2,048
10,000
100,000
```

Operations:

```text
replace
insert
remove
```

Required:

```text
O(log N) counters
```

## 20.8 PERF-11.7 — cold/new graph

Compare:

```text
V4 bulk
generated call-dense native builder
generated small-arity constructors + builder fallback
```

Sizes:

```text
20
200
2,000
10,000
```

## 20.9 PERF-11.8 — strings

```text
short ASCII
short Unicode
embedded NUL
long text
1-4 spans
many spans
diff lines
style/theme atoms
```

Compare:

```text
cstring fast variant
buffer + buffer_length
TextEncoder scratch
V4 string lane
```

## 20.10 PERF-11.9 — realistic trace

Use the final hybrid router only.

Record actual route distribution:

```text
JS no-op
render_ref
scalar direct
PathRef direct
edit transaction
native builder
V4 bulk
cache recovery
```

---

# 21. Routing policy

Initial order:

```text
1. same installed JS root and no forced redraw
       -> JS no-op

2. existing native root, root-only supported patch
       -> generated scalar root function

3. existing native root, shallow path <= generated depth
       -> generated scalar depth function

4. existing native root, cached PathRef
       -> generated PathRef function

5. 2..N supported changed paths
       -> native edit transaction

6. small pending new graph
       -> generated native builder / small-arity constructors

7. large or unsupported graph
       -> V4 bulk

8. cache/ref miss
       -> one V4 cold recovery
```

Thresholds are benchmark-selected and stored in one generated/runtime policy module.

---

# 21A. Migrate every View-bearing boundary

Do not optimize only `Tui.render`.

Before implementation, run repository searches for:

~~~text
BridgeViewNode
nodeForBridge
nodeForDirectBridge
decode_view
View-bearing Object parameters
View[] / animation frame inputs
~~~

The generated/native-ref production path must cover at minimum:

~~~text
Tui.render

History.push
History.freeze

ViewSlot initial value
ViewSlot.setView
ViewSlot.setAnimation
ViewSlot.stopAnimation

ScrollPane initial content
ScrollPane.setContent

component/native View registration paths
any additional View-bearing boundary found at implementation time
~~~

Rules:

~~~text
one semantic state mutation -> one native state-mutation call
input View already represented by NativeRef
no intermediate native View wrapper object
no fallback to property traversal on the qualified Bun path
V3/V4 fallback uses the same NativeViewRuntime cache
~~~

Animation/forest inputs:

~~~text
small frame count:
    generated arity-specialized set_animation_N calls

large frame count:
    reusable Uint32Array of NativeRefs
    buffer + buffer_length + used count
~~~

Frames that already have NativeRefs must not be serialized again.

---

# 22. Implementation tranches

## PERF-11.0 — Bun 1.4 and baseline

```text
pin Bun 1.4.0 and bun-types 1.4.0
record Bun revision
qualify engine-native FFI
commit complete PERF-10 sources
rerun controls under Bun 1.4
```

Commit:

```text
bench(tui): qualify Bun 1.4 retained transport baselines
```

## PERF-11.1 — generator bootstrap

~~~text
create tools/tui-abi-gen Rust binary
add serde/toml/indexmap/serde_path_to_error/miette/thiserror/clap
add quote/syn/prettyplease for Rust output
add Askama templates for TS/C/reference output
add BLAKE3 manifest/signature handshake
add insta snapshots and static layout assertions
create tools/tui-abi/view_abi.toml
emit Rust/C/TS/manifest/layout/conformance/benchmark files
add generate:tui-abi and check:tui-abi
add stale-output CI
generate only noop/render_ref/spacer/text-layout/common-patch/release first
no production routing yet
~~~

Commit:

```text
build(tui): generate Bun 1.4 native View ABI
```

## PERF-11.2 — environment runtime and cache preservation

```text
inventory every cache caller/thread
extract NodeId -> WeakView cache into NativeViewRuntime
merge V3/V4 slots and FastShared nodes/slots into the same runtime
keep environment cleanup and stable runtime pointer lifetime
remove hot static session-table lookup
add NativeRef slots with JS leases
add status cell and function-table bootstrap
prove background consumers retain strong Views instead of cache lookups
no production routing
```

Commit:

```text
refactor(tui): unify semantic cache and native View runtime
```

## PERF-11.3 — exact and scalar direct calls

```text
render_ref
JS exact no-op semantic test
text layout root
common fields root
Bun 1.4 boundary benchmark
```

Commit:

```text
perf(tui): call retained scalar edits through generated FFI
```

## PERF-11.4 — PathRef/lens system

```text
PathStore
path_child generation
JS lineage PathRef cache
depth 1..4 scalar variants
general PathRef variants
path validation
```

Commit:

```text
perf(tui): retain native edit lenses for changed View paths
```

## PERF-11.5 — tiny JS backing and lazy fusion

```text
stable-shape View backing
PendingCreate
PendingPatch
fused common fields
remove rich bridge construction from selected fast paths
construction benchmark
```

Commit:

```text
perf(runtime): remove View command construction from retained updates
```

## PERF-11.6 — native edit transaction

```text
edit_begin
edit_add generated families
native changed-path trie
single atomic commit
multi-edit benchmark
```

Commit:

```text
perf(tui): build multi-edit retained updates natively
```

## PERF-11.7 — PersistentSeq and grid families

```text
axis replace/splice
wide path-copy
grid cell path-copy
no flatten counters
wide benchmark
```

Commit:

```text
perf(tui): preserve persistent sequence edits through generated ABI
```

## PERF-11.8 — native builders

```text
builder handles
callback builder mapping
small-arity generated constructors
cold/new graph benchmark
V4 routing threshold
```

Commit:

```text
perf(tui): construct new View graphs through native builders
```

## PERF-11.9 — string/style specialization

```text
cstring variants
buffer_length variants
embedded NUL fallback
RetainedStr compact storage
StyleRef/BorderRef/ThemeAtomRef
string benchmarks
```

Commit:

```text
perf(tui): specialize Bun 1.4 string and style calls
```

## PERF-11.10 — unified hybrid

```text
route exact/scalar/path/txn/builder/V4
one cold recovery
same cache publication
all View-bearing host boundaries
```

Commit:

```text
perf(tui): route View mutations through generated native ABI
```

## PERF-11.11 — authoritative decision

```text
complete retained matrix
wide matrix
cold guardrail
realistic trace
memory/lifetime
write PERF-11 review
```

Commit:

```text
bench(tui): complete Bun 1.4 native View ABI decision
```

## PERF-11.12 — production cleanup only if it wins

```text
remove FastShared production path
remove losing V2/V3 production path as appropriate
keep one test oracle temporarily
remove duplicate rich JS IR where safe
retain V4 only if chosen bulk path
```

Commit:

```text
perf(tui): adopt generated native-first View runtime
```

---

# 23. Mandatory acceptance checklist

## Generator/bootstrap

~~~text
[ ] exact Bun 1.4.0 and exact revision pinned
[ ] FastShared 1.3.11 pin removed from the new candidate
[ ] generator exists before first semantic fast export
[ ] canonical TOML schema is the only handwritten ABI signature source
[ ] generated Rust wrappers compile and delegate to handwritten semantic impls
[ ] generated TS uses exact fixed signatures; no rest/spread/Reflect.apply
[ ] generated C header and manifest match Rust layouts
[ ] ABI conformance suite passes hot JIT tiers
[ ] buffer arguments use Bun 1.4 buffer_length pairing
[ ] generate/check commands are deterministic
[ ] CI rejects stale generated output
~~~

## Cache/identity preservation

~~~text
[ ] one environment NativeViewRuntime serves direct, V2, V3, V4, and generated ABI
[ ] NodeId -> WeakView semantic cache remains present
[ ] expired weak entries are removed and recoverable
[ ] FastShared's separate nodes/slot semantic cache is not preserved as a second architecture
[ ] JavaScript remains semantic NodeId authority
[ ] NodeId low/high halves are cached once in backing metadata
[ ] NativeRef remains separate from NodeId
[ ] JS leases keep native Views alive only while required
[ ] dropping a lease never destroys a View still retained by host/History/ViewSlot/parent
[ ] V3/V4 recovery republishes into the same NativeRef/cache tables
[ ] cache miss triggers exactly one cold recovery
~~~

## Zero-encode hot path

~~~text
[ ] exact identity writes no command or payload bytes
[ ] text-layout-only edit writes no command or payload bytes
[ ] common scalar patch writes no command or payload bytes
[ ] existing-child replacement writes no complete View payload
[ ] common deep edit passes a PathRef scalar, not a rebuilt path array
[ ] no FastShared op record is emitted in generated direct candidate
[ ] no per-op Uint32Array view is allocated
[ ] no generic opcode dispatch runs natively
[ ] structural_encoding_ns is exactly zero for no-new-data retained operations
~~~

```text
Bun 1.4
[ ] packageManager pinned to 1.4.0
[ ] bun-types pinned to 1.4.0
[ ] exact revision recorded and checked
[ ] engine-native FFI probe passes
[ ] JIT-enabled requirement tested

Generator
[ ] canonical schema exists
[ ] bridge discriminants cross-validated
[ ] Rust/C/TS generated from same schema
[ ] generator check detects drift
[ ] layout/offset tests pass
[ ] BLAKE3 manifest handshake passes
[ ] generated functions have fallback mapping
[ ] generated code is not hand-edited

Cache/identity
[ ] same environment NodeId -> WeakView cache preserved
[ ] expired weak entries removed
[ ] NodeId full safe-integer domain preserved
[ ] NativeRef slots share same View objects
[ ] strong JS lease drops to weak retention
[ ] no strong forever cache
[ ] V4 publishes to same cache/slot system
[ ] cache miss triggers one cold recovery
[ ] second miss hard-fails

Zero structural encoding
[ ] scalar root edit writes no command buffer
[ ] shallow path edit writes no path array
[ ] PathRef edit writes no path array
[ ] multi-edit uses typed FFI calls, not JS command records
[ ] command_words_written == 0 for retained direct cases
[ ] structural_encoding_ns == 0 for retained direct cases

JS construction
[ ] fast View backing has stable shape
[ ] rich BridgeViewNode not built on selected fast path
[ ] fluent modifiers fuse before materialization
[ ] construction regression versus PERF-7v2 eliminated

Persistent structures
[ ] stable subtree descendants never visited
[ ] wide replace/insert/remove are O(log N)
[ ] layout consumes persistent sequence without flattening
[ ] grid cell update shares unchanged cells/tracks

Strings
[ ] no string work for non-text scalar edits
[ ] cstring only used with correct NUL semantics
[ ] arbitrary text has length-preserving variant
[ ] buffer inputs use buffer_length twin
[ ] no temporary Rust String then final copy
[ ] retained UTF-8 immutability is sound
[ ] no Arc<dyn SharedUtf8Source> per hot span in final candidate

Safety
[ ] invalid pointers never bound after handshake
[ ] handle kind/runtime checked
[ ] no callbacks on hot path
[ ] no Rust panic crosses FFI
[ ] buffer bounds/alignment validated
[ ] oversized transaction falls back before mutation
[ ] malformed calls leave host/cache unchanged
[ ] fuzz/ASan lifetime tests pass

Performance
[ ] timing build has no hot atomic counters
[ ] exact identity no regression
[ ] normal retained matrix >=10% faster than PERF-7v2 direct
[ ] target >=15% faster reached or explicit rejection
[ ] representative trace >=10% faster
[ ] cold/bulk no >5% regression from best candidate
[ ] wide structural work logarithmic
```

---

# 24. Banned shortcuts

Reject the tranche if it does any of the following:

```text
encodes generated calls into a generic opcode buffer
uses one universal view_call(opcode, args) dispatcher
builds PathStep arrays on every common retained update
passes NodeId arrays on every update without proving necessity
replaces the environment weak cache with a separate native cache
destroys weak-expiry recovery
holds every View strongly forever
flattens PersistentSeq for native convenience
keeps both a rich JS tree and a full native tree permanently without profiling
calls FFI from every FinalizationRegistry callback
uses callbacks from Rust into JS on the hot path
uses BigInt handles on normal calls
returns C structs on normal calls
loads a second native library image
benchmarks Bun 1.4 candidate against Bun 1.3 control as final evidence
calls engine string transcoding 'zero cost'
moves JS encoding into native parsing and calls it eliminated
accepts a result that is only faster after subtracting measured phases
```

---

# 25. Expected steady-state traces

## 25.1 Exact identity

```text
JS:
    root === lastInstalledRoot
    no forced redraw

result:
    return
```

Alternative:

```text
render_ref(runtime, root_ref)
    -> slot weak upgrade
    -> host install/dirty
```

## 25.2 Text layout change

```text
JS:
    PendingPatch(base=rootRef, pathRef=P, wrap=noWrap, align=center)

Bun 1.4:
    direct generated C call

Rust:
    resolve rootRef
    walk PathRef
    clone text metadata only
    rebuild changed ancestors
    path-copy sequences where needed
    publish semantic cache + refs
    install root

JS structural encoding:
    none
```

## 25.3 Two-branch edit

```text
edit_begin(base)
edit_add_text(pathA, ...)
edit_add_decoration(pathB, ...)
edit_commit_render()

Rust:
    native trie shares common root rebuild
```

## 25.4 Cold graph

```text
router estimates graph

small:
    native builder calls

large:
    V4 bulk

both:
    publish into same cache/ref runtime
```

## 25.5 Cache resurrection

```text
JS holds View backing
native weak entry expired

fast call:
    returns CACHE_MISS before mutation

JS:
    V4 cold materializes current root
    same semantic cache repopulated
    ref metadata refreshed
    retry once if required
```

---

# 26. Required final PERF-11 report

Report:

```text
exact candidate SHAs
Bun 1.4 version and revision
native artifact hash
generator/schema/manifest hash
ABI version
function count and enabled families
routing thresholds

per candidate/mode/workload:
    JS API construction
    JS fusion
    text transcode
    structural encoding
    FFI
    native semantic
    publication
    host commit
    total
    median/p95/p99 where valid
    confidence intervals

route counts:
    no-op
    render_ref
    scalar
    shallow-depth
    PathRef
    edit transaction
    native builder
    V4
    recovery

cache/lifetime:
    semantic cache entries
    live weak upgrades
    stale removals
    JS leases
    release batches
    slot pages

persistent structure:
    sequence leaves/branches cloned
    stable items visited
    wide scaling

final decision:
    exact gate passed/failed
    exact percentage against PERF-7v2 direct
```

A conclusion such as:

> Native is faster if encoding is ignored.

is not acceptable.

The candidate either wins end to end or it does not.

---

# 27. Bottom line

PERF-10 established that Rust-side retained updates are fast enough to be valuable, but JavaScript still spends too much time constructing and encoding a second representation.

Bun 1.4 removes the historical reason to keep the C ABI artificially small. Its engine-native FFI can compile hot typed call sites into direct C calls. Therefore the optimal next experiment is:

```text
many generated semantic functions
+ monomorphic scalar signatures
+ cached native PathRefs
+ native edit transactions
+ native callback builders
+ one shared environment cache/runtime
+ V4 bulk fallback
```

The decisive rule is:

> Generated source volume is cheap. Runtime interpretation, duplicated semantic construction, and JS serialization are expensive.

For normal retained updates, the architecture should contain no structural encoder at all.

---

# Source appendix

## Iyon PERF-10 baseline

Current reviewed branch head:

https://github.com/alexykn/iyon/commit/4672d247ab6679e702855a06f9c661a97c903784

PERF-10 review:

https://github.com/alexykn/iyon/blob/4672d247ab6679e702855a06f9c661a97c903784/PERF-10-performance-review.md

Current FastShared TypeScript encoder:

https://github.com/alexykn/iyon/blob/4672d247ab6679e702855a06f9c661a97c903784/packages/iyon-runtime/src/tui/fast_shared.ts

Current FastShared native implementation (including its separate per-host nodes/slot cache and static session table):

https://github.com/alexykn/iyon/blob/4672d247ab6679e702855a06f9c661a97c903784/crates/iyon-native/src/tui/fast_shared.rs

Current environment direct/V3/V4 cache owner:

https://github.com/alexykn/iyon/blob/4672d247ab6679e702855a06f9c661a97c903784/crates/iyon-native/src/tui.rs

## Bun 1.4 engine-native FFI

Bun engine-native FFI pull request merged for Bun 1.4:

https://github.com/oven-sh/bun/pull/35246

Merge commit:

https://github.com/oven-sh/bun/commit/01b81aa4003b375c53fd22eb0fc4de592ac892e0

Bun FFI documentation at the corresponding source revision:

https://github.com/oven-sh/bun/blob/01c4e2fd6d94adf2e9157d1e6329c328eb37dfae/docs/runtime/ffi.mdx

Relevant documented properties:

```text
dlOpen/linkSymbols/CFunction/JSCallback implemented in JavaScriptCore
hot DFG/FTL sites become direct native calls
buffer_length pairs pointer and byte length from one TypedArray
cstring arguments may accept JS strings directly
JIT required
```

## React Native Fabric and Codegen

https://reactnative.dev/architecture/render-pipeline

https://reactnative.dev/docs/the-new-architecture/what-is-codegen

https://reactnative.dev/blog/2024/10/23/the-new-architecture-is-here

Relevant lessons:

```text
immutable native shadow tree
structural sharing
synchronous native interface
generated boilerplate/type safety
```

## Dart leaf FFI

https://api.dart.dev/dart-ffi/Native/isLeaf.html

Relevant lesson:

```text
short, synchronous, non-blocking native calls can use a reduced-overhead calling sequence
```

## OpenTUI

https://github.com/anomalyco/opentui

https://github.com/anomalyco/opentui/blob/main/packages/core/src/platform/ffi.ts

https://github.com/anomalyco/opentui/blob/main/packages/core/src/zig/lib.zig

Relevant lessons:

```text
broad direct C ABI is maintainable
native handles keep state native
FFI-dense layout/native operations are practical
centralized generated/runtime-adapted bindings
```
