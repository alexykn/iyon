# PERF-7 benchmark result

## Method

This benchmark compares the existing direct retained structured decoder with a
packed `Uint32Array + string[]` transaction. Both candidates are measured
through the complete public operation:

```text
View construction / packed encoding
→ N-API call
→ Rust decode and cache work
→ final View rendered by the native headless host
```

The packed decoder is enabled only with the non-production Cargo feature
`perf-packed-benchmark`. The normal native build does not enable it; the public
runtime continues to use Candidate A.

Run it with:

```sh
ION_NATIVE_FEATURES=perf-packed-benchmark,perf-counters CARGO_BUILD_JOBS=1 \
  bun run native:stage
PERF_ITERATIONS=20 bun packages/iyon-runtime/bench/tui_performance.ts
```

The run used 20 post-warmup samples for each size and update mode, equal fresh
native hosts per candidate/mode, and a direct-vs-packed rendered-row
correctness check before measurement.

## Results

Results below are packed improvement over direct, calculated from the recorded
median and p95 total-operation latency. Positive values are faster.

| Size | Mode | Median | p95 |
| --- | --- | ---: | ---: |
| 2,000 | COLD | +48.4% | +49.8% |
| 2,000 | IDENTICAL_IDENTITY | +4.1% | +39.0% |
| 2,000 | SHARED_PATH | -0.5% | +38.5% |
| 2,000 | REBUILT_EQUIVALENT | +54.1% | +35.7% |
| 10,000 | COLD | +63.6% | -55.6% |
| 10,000 | IDENTICAL_IDENTITY | -0.4% | +55.0% |
| 10,000 | SHARED_PATH | -0.7% | +47.8% |
| 10,000 | REBUILT_EQUIVALENT | +72.3% | +42.2% |

Small/medium and warm large cases did not consistently clear the 5% median
threshold: the packed path regressed by approximately 6.6% at 200-node
IDENTICAL_IDENTITY, 0.5% at 2,000-node SHARED_PATH, and 0.7% at 10,000-node
SHARED_PATH. The recorded p95 and heap measurements were also unstable in
some 10,000-node cases, including a packed cold p95 regression and a packed
shared-path heap increase.

## Decision

Do **not** promote Candidate B to production.

The large cold/rebuilt cases exceed the 15% threshold, but that result is not
sufficient by itself. Candidate B has sub-5% median gains for several
shared-path and identical-identity workloads. More importantly, the packed
encoder/decoder only covers the text/column benchmark shape, not the complete
generic View schema. That correctness-scope gap rejects it under the PERF-7
rules despite the cold/rebuilt latency gains.

Candidate A remains the only production transport. The packed implementation is
retained solely behind `perf-packed-benchmark` for reproducible benchmark
experimentation; the default native build exposes no working packed transport.
