# Treap - Rust

Probabilistic balanced BST. Random priorities + heap-on-priority + BST-on-key give expected O(log n) without colour bits or rebalancing factors.

Part of the [submillisecond.com cookbook](https://submillisecond.com/cookbook/recipes/subms-treap). Zero external dependencies; `std` only.

## Install

```toml
[dependencies]
subms-treap = "0.4"
```

## Quickstart

```sh
cargo test --release
cargo run --example demo
```

## Public API

- `pub struct Treap<K, V>`
- `pub fn new(seed: u64) -> Self`
- `pub fn with_capacity(seed: u64, capacity: usize) -> Self`
- `pub fn len(&self) -> usize`
- `pub fn is_empty(&self) -> bool`
- `pub fn insert(&mut self, key: K, value: V) -> Option<V>`
- `pub fn get(&self, key: &K) -> Option<&V>`
- `pub fn remove(&mut self, key: &K) -> Option<V>`
- `pub fn collect_in_order(&self) -> Vec<(&K, &V)>`

## Files

- `src/lib.rs` - implementation.
- `tests/` - integration tests; correctness, edge cases, property/stress.
- `examples/demo.rs` - stdout walkthrough.
- `examples/perf_main.rs` - bench entry (behind the `harness` feature).

## License

Dual-licensed under MIT OR Apache-2.0.
