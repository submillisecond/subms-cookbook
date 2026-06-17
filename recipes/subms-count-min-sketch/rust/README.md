# Count-Min Sketch - Rust

Frequency-estimation sketch with conservative update and Kirsch-Mitzenmacher hashing. Always over-estimates, never under.

Part of the [submillisecond.com cookbook](https://submillisecond.com/cookbook/recipes/subms-count-min-sketch). Zero external dependencies; `std` only.

## Install

```toml
[dependencies]
subms-count-min-sketch = "0.4"
```

## Quickstart

```sh
cargo test --release
cargo run --example demo
```

## Public API

- `pub struct CountMinSketch`
- `pub fn new(d: usize, w: usize) -> Self`
- `pub fn depth(&self) -> usize`
- `pub fn width(&self) -> usize`
- `pub fn add(&mut self, key: &str)`
- `pub fn estimate(&self, key: &str) -> u32`

## Files

- `src/lib.rs` - implementation.
- `tests/` - integration tests; correctness, edge cases, property/stress.
- `examples/demo.rs` - stdout walkthrough.
- `examples/perf_main.rs` - bench entry (behind the `harness` feature).

## License

Dual-licensed under MIT OR Apache-2.0.
