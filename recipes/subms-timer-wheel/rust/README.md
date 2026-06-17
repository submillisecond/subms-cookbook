# Timer wheel - Rust

Single-level hashed timer wheel. O(1) schedule and cancel; tick fires every timer with rounds=0 in the current bucket.

Part of the [submillisecond.com cookbook](https://submillisecond.com/cookbook/recipes/subms-timer-wheel). Zero external dependencies; `std` only.

## Install

```toml
[dependencies]
subms-timer-wheel = "0.4"
```

## Quickstart

```sh
cargo test --release
cargo run --example demo
```

## Public API

- `pub struct TimerWheel<V>`
- `pub fn new(num_slots: usize) -> Self`
- `pub fn num_slots(&self) -> usize`
- `pub fn schedule(&mut self, delay_ticks: usize, value: V) -> u64`
- `pub fn cancel(&mut self, id: u64) -> bool`
- `pub fn tick(&mut self) -> Vec<V>`

## Files

- `src/lib.rs` - implementation.
- `tests/` - integration tests; correctness, edge cases, property/stress.
- `examples/demo.rs` - stdout walkthrough.
- `examples/perf_main.rs` - bench entry (behind the `harness` feature).

## License

Dual-licensed under MIT OR Apache-2.0.
