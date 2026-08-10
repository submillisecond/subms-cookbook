# Merge iterator - Rust

K-way merge of sorted streams via a min-heap of stream heads. next() is one heap-pop and one heap-push.

Part of the [submillisecond.com cookbook](https://www.submillisecond.com/cookbook/recipes/subms-merge-iterator). Zero external dependencies; `std` only.

## Install

```toml
[dependencies]
subms-merge-iterator = "0.10"

# or with the opt-in cursor and key-resolution variants
subms-merge-iterator = { version = "0.10", features = ["seek-to", "tombstones"] }
```

## Quickstart

```sh
cargo test --all-features
cargo run --example sample_app --features full
```

## Public API

Base, always available:

- `MergeIterator::new(streams)` - ascending k-way merge; implements `Iterator`.
- `peek()`, `live_streams()`, `num_streams()` - cursor introspection.

Behind Cargo features (`full` turns on all five):

- `seek-to` - `SeekableMergeIterator` with `seek(target)`, `set_upper_bound(hi)` (exclusive), `clear_upper_bound()`.
- `reverse` - `ReverseMergeIterator` over descending sources, with `seek_for_prev(target)`, `set_lower_bound(lo)` (inclusive), `clear_lower_bound()`.
- `tombstones` - `TombstoneMergeIterator` / `TombstoneEntry`; latest-source-wins with delete markers.
- `dedup` - `DedupMergeIterator` / `DedupEntry`; one entry per distinct key, latest-source-wins.
- `priority` - `PriorityMergeIterator` / `PriorityEntry` / `PrioritySource`; explicit per-source precedence.

The `harness` feature pulls in `subms` for the `SubMsRecipe` impl and the bench targets.

## Files

- `src/lib.rs` - the base merge; `src/features/` - one module per opt-in feature.
- `src/*_tests.rs` - colocated unit tests; `tests/sub_millisecond_bench.rs` - the asserted p99 gate.
- `examples/sample_app.rs` - a miniature market-data store read through every variant.
- `examples/perf_main.rs`, `examples/perf_features.rs` - bench entries (behind `harness`).

## License

Dual-licensed under MIT OR Apache-2.0.
