# subms-primer-perf-harness (Rust)

The runnable companion to the `subms-perf-harness` primer. A tiny realistic
workload (open-addressed `u32 -> u32` map, measured at insert / hit-lookup /
miss-lookup) wired through the `subms` perf harness end-to-end.

This crate is not published. It exists to be read, run, and copied from when
you start your own recipe.

## What it shows

- `SubMsPerfHarness` with three named stages following the cookbook k/v
  convention (`put`, `get_hit`, `get_miss`).
- A `SubMsRecipe` impl (`HarnessRecipe`) that's the standard recipe shape
  trimmed to its smallest defensible form.
- `run_bench` -> `summarize` -> `print_summary` -> `assert_p99_under` ->
  `summary_to_json` - the full presenter / asserter pipeline.
- An `examples/perf_main.rs` that emits a canonical `SubMsBenchSummary` JSON
  on stdout, suitable for capture into `perf/rust.json`.

## Run the example

```sh
cargo run --release --example perf_main > perf/rust.json
```

With explicit params:

```sh
cat <<EOF | cargo run --release --example perf_main > perf/rust.json
entries=50000
warmup=5000
seed=0
EOF
```

Stdout is the JSON capture. Stderr is the human-readable percentile table
plus the run header.

## Run the tests

```sh
cargo test --release
```

Includes a `sub_millisecond_gate_passes_on_realistic_workload` test that
runs the harness through `assert_p99_under` and asserts each stage's p99
stays under one millisecond.

## Layout

- `src/lib.rs` - `TinyMap` (the thing under test) + `HarnessRecipe` (the
  wiring) + `default_assertions()` + `SUB_MS_NS` constant.
- `examples/perf_main.rs` - the end-to-end driver.
- `tests/lib_tests.rs` - TinyMap correctness, recipe wiring, harness
  round-trip, p99 gate.

## License

MIT OR Apache-2.0.
