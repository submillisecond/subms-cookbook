# Rate limiter - Rust

Lock-free GCRA rate limiter using a single-atomic CAS-loop. One `tryAcquire` is one load and one CAS, even at 8-way contention.

Part of the [submillisecond.com cookbook](https://submillisecond.com/cookbook/recipes/subms-rate-limiter). Zero external dependencies; `std` only.

## Install

```toml
[dependencies]
subms-rate-limiter = "0.4"
```

## Quickstart

```sh
cargo test --release
cargo run --example demo
```

## Public API

- `pub struct RateLimiter`
- `pub fn new(rate_per_sec: f64, burst_capacity: u64) -> Self`
- `pub fn try_acquire(&self) -> bool`
- `pub fn rate_per_sec(&self) -> f64`
- `pub fn burst_capacity(&self) -> u64`

## Files

- `src/lib.rs` - implementation.
- `tests/` - integration tests; correctness, edge cases, property/stress.
- `examples/demo.rs` - stdout walkthrough.
- `examples/perf_main.rs` - bench entry (behind the `harness` feature).

## License

Dual-licensed under MIT OR Apache-2.0.
