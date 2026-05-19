# LSM tree - Rust

Edition 2021, stable rustc. No external (crates.io) dependencies - only
the cookbook's own bloom-filter recipe
([`cookbook/recipes/subms-bloom-filter/rust`](../../subms-bloom-filter/rust/)),
referenced as a path dependency. Not part of a Cargo workspace.

```sh
cargo test --release                                                       # correctness + perf tests
cargo test --release --test sub_millisecond_bench -- --nocapture           # perf test with output
cargo run --release --example demo                                         # tiny illustrative scenario
```

The `sub_millisecond_bench` test asserts p99 of every operation stays
under 1ms and exits non-zero if it does not. Cargo resolves the path
dependency automatically - no separate install step.

## Files

- `src/lib.rs` - `LsmTree` coordinator: routes writes, flushes, walks
  runs newest-first on read.
- `src/memtable.rs` - `BTreeMap`-backed memtable; `None` value = tombstone.
- `src/sstable.rs` - writes a sorted run with bloom-filter trailer; on
  open, slurps the whole file into a `Vec<u8>` and parses the bloom out
  of the trailer; get short-circuits on a bloom miss. Imports
  `BloomFilter` from the `subms_bloom_filter` crate.
- `examples/demo.rs` - put / delete / flush / get walkthrough on stock
  symbols.
- `tests/lsm_tree_tests.rs` - correctness: round-trip, tombstone shadowing,
  newer-SSTable wins, reopen-from-disk, threshold-driven flush, bloom
  doesn't lose present keys.
- `tests/sub_millisecond_bench.rs` - perf test: 50,000 puts + 50,000
  hit-reads + 50,000 miss-reads, prints p50/p99/p999/max in microseconds,
  asserts p99 < 1ms.
