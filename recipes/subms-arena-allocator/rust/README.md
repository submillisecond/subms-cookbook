# Arena allocator - Rust

Fixed-capacity bump-pointer arena with reset() for per-request reuse. Allocate is a single cursor bump; reset is constant-time. Auto-grow is opt-in via the `growable` feature.

Part of the [submillisecond.com cookbook](https://submillisecond.com/cookbook/recipes/subms-arena-allocator). Zero external dependencies; `std` only.

## Install

```toml
[dependencies]
subms-arena-allocator = "0.5"
```

Opt-in features layer focused capability over the fixed-capacity base:
`typed`, `growable`, `stats`, `aligned`.

## Quickstart

```sh
cargo test --release
cargo run --example demo
```

## Public API

- `pub struct Bump`
- `pub fn new() -> Self`
- `pub fn with_capacity(capacity_bytes: usize) -> Self`
- `pub fn alloc_copy<T: Copy>(&mut self, value: T) -> &mut T`
- `pub fn try_alloc_copy<T: Copy>(&mut self, value: T) -> Option<&mut T>`
- `pub fn alloc_raw(&mut self, layout: Layout) -> *mut u8`
- `pub fn try_alloc_raw(&mut self, layout: Layout) -> Option<*mut u8>`
- `pub fn reset(&mut self)`
- `pub fn used(&self) -> usize`
- `pub fn capacity(&self) -> usize`
- `pub fn total_capacity(&self) -> usize` (alias for `capacity`)

## Files

- `src/lib.rs` - implementation.
- `tests/` - integration tests; correctness, edge cases, property/stress.
- `examples/demo.rs` - stdout walkthrough.
- `examples/perf_main.rs` - bench entry (behind the `harness` feature).

## License

Dual-licensed under MIT OR Apache-2.0.
