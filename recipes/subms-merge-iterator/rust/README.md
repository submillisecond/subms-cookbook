# Merge iterator - Rust

K-way merge of sorted streams via a min-heap of stream heads. next() is one heap-pop and one heap-push.

Part of the [submillisecond.com cookbook](https://submillisecond.com/cookbook/recipes/subms-merge-iterator). Zero external dependencies; `std` only.

## Install

```toml
[dependencies]
subms-merge-iterator = "0.4"
```

## Quickstart

```sh
cargo test --release
cargo run --example demo
```

## Public API

- `pub struct MergeIterator<T: Ord, I: Iterator<Item = T>>`
- `pub fn new<S: IntoIterator<Item = I>>(streams: S) -> Self`

## Files

- `src/lib.rs` - implementation.
- `tests/` - integration tests; correctness, edge cases, property/stress.
- `examples/demo.rs` - stdout walkthrough.
- `examples/perf_main.rs` - bench entry (behind the `harness` feature).

## License

Dual-licensed under MIT OR Apache-2.0.
