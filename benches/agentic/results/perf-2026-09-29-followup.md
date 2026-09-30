# Evaluator latency and throughput: follow-up, 2026-09-29

Baseline: `570bff8696431cf68fd97651c119ef4d3c7d3ae8`. Optimized: the
working-tree changes accompanying this report. Windows, Rust 1.98.1, default
native features, Cargo's release profile. These are **in-process** figures;
they exclude process startup and transport overhead, and are not directly
comparable to the earlier WSL measurements.

Both executables used the expanded `examples/perf_probe.rs`. Each collection
case warms up once, then reports the median of seven runs. Warm request cases
include parsing and evaluation in an existing environment, with seven batches
of 10,000 requests. Environment construction is outside the timer.

Four paired rounds ran after compilation and validation finished: baseline,
optimized; baseline, optimized; optimized, baseline; optimized, baseline.
Every collection result matched in all eight runs. The table gives the range
of the four reported medians, and the median of the four paired speedups
(`baseline / optimized`), rather than dividing unpaired timing aggregates.
The host's variation is visible; values below 1 mean slower in these runs.

| Case | Baseline range | Optimized range | Paired median speedup |
| --- | ---: | ---: | ---: |
| builtin call | 270–379 ns | 235–256 ns | 1.23x |
| scalar map, per element | 159–212 ns | 133–159 ns | 1.29x |
| where, per element | 169–208 ns | 147–183 ns | 1.20x |
| build record + implicit field filter, per row | 1,635–2,329 ns | 1,468–1,811 ns | 1.19x |
| module call (`math.abs`), per element | 799–879 ns | 223–238 ns | 3.65x |
| build nested record with 100-element payload + filter, per row | 17,966–23,167 ns | 15,594–17,568 ns | 1.25x |
| indexed map, per element | 272–302 ns | 225–257 ns | 1.19x |
| reduce, per element | 390–553 ns | 197–263 ns | 2.27x |
| indexed reduce, per element | 385–420 ns | 302–387 ns | 1.28x |
| string concat, per element | 948–1,011 ns | 982–1,114 ns | 0.96x |
| sort, per element | 269–344 ns | 261–324 ns | 1.05x |
| SQL over piped records, per row | 1,702–2,222 ns | 1,811–2,542 ns | 0.99x |
| warm parse + eval `1 + 1`, per request | 326–395 ns | 341–465 ns | 0.92x |
| warm parse + eval `math.abs(-42)`, per request | 1,388–1,648 ns | 777–960 ns | 1.79x |
| warm parse + eval `str.upper("hello")`, per request | 1,443–1,667 ns | 866–1,119 ns | 1.63x |

The stable gains are in module calls and collection lambda execution. No gain
is claimed for tiny arithmetic, strings, sorting or SQL. Nested-record timing
includes payload construction and allocation, not just the field lookup.
The original one-shot "parse+eval tiny" case parsed outside its timer and often
reported zero on Windows; it has been replaced with the batched warm cases.

## Changes

- Field chains rooted in an environment variable borrow the source record and
  clone only the selected value. Previously, `math.abs` cloned the whole math
  module, and `row.meta.n` cloned the row and then its nested record. Computed
  records keep ordinary evaluation order and move the selected field out of
  the owned result. Missing-field suggestions, coded type errors and deadline
  checks remain in place.
- `map`, `where` and `reduce` allocate their parameter slots once per nonempty
  collection. Each iteration swaps values into those slots and restores them
  without removing/reallocating their name keys. Nested calls and failing
  bodies restore the original environment; returned closures still capture
  the value from their own iteration. Empty collections retain their previous
  behavior.
- `reduce` consumes its piped array and moves the accumulator and each element
  into the lambda. It chooses two or three arguments from the lambda's declared
  arity instead of trying three and retrying with two. This also preserves the
  original body error from an indexed lambda instead of replacing it with an
  arity error.

## Validation and reproduction

- `cargo test --workspace -- --skip posix::tests::collation_matches_glibc_en_us`:
  **2,469 tests passed**, including seven new ownership/binding regression
  tests, existing pipeline scaling checks, streaming and deadline tests.
- The remaining collation test **passed separately** with `LC_ALL` unset:
  `cargo test --lib posix::tests::collation_matches_glibc_en_us`. The initial
  unfiltered run failed because this environment inherited `LC_ALL=C.UTF-8`,
  overriding the test's `LC_COLLATE=en_US.UTF-8`. The test itself was unchanged;
  the override was removed only for the subprocess. **2,470 tests passed in
  total across these two runs.**
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `cargo build --release --bins`: passed; optimized `ae` and `aimodel` built.
  Release `ae` smoke checks returned 45 for a module-call/map/reduce pipeline
  and 7 for a nested field read.

Run `cargo run --release --example perf_probe` to reproduce the workloads.
For a baseline comparison, use the same probe with the baseline source files,
preserve each executable, and alternate runs. After restoring copied source
files, update their timestamps or force a rebuild: Cargo otherwise can reuse
the baseline binary because copied files retain old timestamps. Executable
hashes were checked to ensure the measured builds differed.

No release profile, safety policy, persistent-file cache, or startup behavior
was changed in this follow-up.
