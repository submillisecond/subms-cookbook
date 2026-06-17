# MPSC queue - Rust

Vyukov's multi-producer single-consumer linked queue. Producers swap the head; consumer walks `next` pointers. The dangling-tail window is the whole story.

Part of the [submillisecond.com cookbook](https://submillisecond.com/cookbook/recipes/subms-mpsc-queue). Zero external dependencies; `std` only.

## Install

```toml
[dependencies]
subms-mpsc-queue = "0.4"
```

## Quickstart

```sh
cargo test --release
cargo run --example demo
```

## Public API

- `pub enum PopResult<T>`
- `pub struct MpscQueue<T>`
- `pub fn new() -> Self`
- `pub fn push(&self, value: T)`
- `pub fn try_pop(&mut self) -> PopResult<T>`

## Files

- `src/lib.rs` - implementation.
- `tests/` - integration tests; correctness, edge cases, property/stress.
- `examples/demo.rs` - stdout walkthrough.
- `examples/perf_main.rs` - bench entry (behind the `harness` feature).

## License

Dual-licensed under MIT OR Apache-2.0.
