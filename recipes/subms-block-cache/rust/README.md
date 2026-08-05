# Block cache - Rust

Clock-sweep block cache with constant-time eviction. Approximates LRU; pays one hash probe + one slot bump per get.

Part of the [submillisecond.com cookbook](https://www.submillisecond.com/cookbook/recipes/subms-block-cache). Zero external dependencies; `std` only. The perf harness is an opt-in `harness` feature.

## Install

```toml
[dependencies]
subms-block-cache = "0.9"
```

## Quickstart

```sh
cargo test --all-features
cargo run --example sample_app --features arc,tinylfu,weighted,concurrent-shards,metrics
```

## Public API

Base cache:

- `pub struct BlockCache<K, V>`
- `pub fn with_capacity(capacity: usize) -> Self`
- `pub fn capacity(&self) -> usize`
- `pub fn len(&self) -> usize`
- `pub fn is_empty(&self) -> bool`
- `pub fn get(&mut self, key: &K) -> Option<&V>`
- `pub fn put(&mut self, key: K, value: V) -> Option<(K, V)>`
- `pub fn remove(&mut self, key: &K) -> Option<V>`
- `pub fn clear(&mut self)`

Opt-in features, each behind its own Cargo feature: `arc` (`ArcCache`),
`tinylfu` (`TinyLfuCache`), `weighted` (`WeightedCache`), `concurrent-shards`
(`ShardedCache`), `metrics` (`MetricsCache`, `CacheMetrics`). `full` turns on
all five. `remove` / `clear` live on the base cache and are forwarded by
`MetricsCache` and `ShardedCache`; the three policy variants do not have them.

## Files

- `src/lib.rs` - the base cache.
- `src/features/` - one module per opt-in feature, each with colocated tests.
- `src/*_tests.rs` - colocated unit tests.
- `tests/sub_millisecond_bench.rs` - the asserted p99 gate (`harness` feature).
- `examples/sample_app.rs` - end-to-end tour of the base API and every feature.
- `examples/perf_main.rs` - bench entry (`harness`).
- `examples/perf_features.rs` - per-feature classification sweep (`harness`).
- `examples/growth_main.rs` - storage-growth curve (`harness`).

## License

Dual-licensed under MIT OR Apache-2.0.
