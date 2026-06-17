# Adaptive Radix Tree (ART) - Rust

Byte-trie with adaptive node size. Small nodes hold up to 4 children in an inline array; once a 5th child is needed, the node grows to a full 256-way dispatch.

Part of the [submillisecond.com cookbook](https://submillisecond.com/cookbook/recipes/subms-adaptive-radix-tree). Zero external dependencies; `std` only.

## Install

```toml
[dependencies]
subms-adaptive-radix-tree = "0.4"
```

## Quickstart

```sh
cargo test --release
cargo run --example demo
```

## Public API

- `pub struct Art<V>`
- `pub fn new() -> Self`
- `pub fn len(&self) -> usize`
- `pub fn is_empty(&self) -> bool`
- `pub fn insert(&mut self, key: &[u8], value: V) -> Option<V>`
- `pub fn get(&self, key: &[u8]) -> Option<&V>`

## Files

- `src/lib.rs` - implementation.
- `tests/` - integration tests; correctness, edge cases, property/stress.
- `examples/demo.rs` - stdout walkthrough.
- `examples/perf_main.rs` - bench entry (behind the `harness` feature).

## License

Dual-licensed under MIT OR Apache-2.0.
