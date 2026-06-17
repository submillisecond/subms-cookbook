# Cuckoo filter - Rust

Bloom-alternative that supports delete. Partial-key cuckoo hashing places each fingerprint in one of two candidate buckets; under load, kick a fingerprint out and re-place it.

Part of the [submillisecond.com cookbook](https://submillisecond.com/cookbook/recipes/subms-cuckoo-filter). Zero external dependencies; `std` only.

## Install

```toml
[dependencies]
subms-cuckoo-filter = "0.4"
```

## Quickstart

```sh
cargo test --release
cargo run --example demo
```

## Public API

- `pub struct CuckooFilter`
- `pub fn with_capacity(expected_entries: usize) -> Self`
- `pub fn len(&self) -> usize`
- `pub fn is_empty(&self) -> bool`
- `pub fn bucket_count(&self) -> usize`
- `pub fn insert(&mut self, key: &str) -> bool`
- `pub fn contains(&self, key: &str) -> bool`
- `pub fn delete(&mut self, key: &str) -> bool`

## Files

- `src/lib.rs` - implementation.
- `tests/` - integration tests; correctness, edge cases, property/stress.
- `examples/demo.rs` - stdout walkthrough.
- `examples/perf_main.rs` - bench entry (behind the `harness` feature).

## License

Dual-licensed under MIT OR Apache-2.0.
