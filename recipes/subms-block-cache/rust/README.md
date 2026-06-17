# Block cache - Rust

Clock-sweep block cache with constant-time eviction. Approximates LRU; pays one hash probe + one slot bump per get.

Part of the [submillisecond.com cookbook](https://submillisecond.com/cookbook/recipes/subms-block-cache). Zero external dependencies; `std` only.

## Install

```toml
[dependencies]
subms-block-cache = "0.4"
```

## Quickstart

```sh
cargo test --release
cargo run --example demo
```

## Public API

- `pub struct BlockCache<K, V>`
- `pub fn with_capacity(capacity: usize) -> Self`
- `pub fn capacity(&self) -> usize`
- `pub fn len(&self) -> usize`
- `pub fn is_empty(&self) -> bool`
- `pub fn get(&mut self, key: &K) -> Option<&V>`
- `pub fn put(&mut self, key: K, value: V) -> Option<(K, V)>`

## Files

- `src/lib.rs` - implementation.
- `tests/` - integration tests; correctness, edge cases, property/stress.
- `examples/demo.rs` - stdout walkthrough.
- `examples/perf_main.rs` - bench entry (behind the `harness` feature).

## License

Dual-licensed under MIT OR Apache-2.0.
