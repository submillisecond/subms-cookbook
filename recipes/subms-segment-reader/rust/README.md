# Segment reader - Rust

Read length-prefix framed records from a segment file. Truncated tails surface as a typed error, not a crash.

Part of the [submillisecond.com cookbook](https://submillisecond.com/cookbook/recipes/subms-segment-reader). Zero external dependencies; `std` only.

## Install

```toml
[dependencies]
subms-segment-reader = "0.4"
```

## Quickstart

```sh
cargo test --release
cargo run --example demo
```

## Public API

- `pub enum Error`
- `pub struct SegmentReader<R: Read>`
- `pub fn new(reader: R) -> Self`
- `pub fn next_record(&mut self) -> Result<Option<&[u8]>, Error>`
- `pub struct SegmentWriter<W: Write>`
- `pub fn new(writer: W) -> Self`
- `pub fn write(&mut self, record: &[u8]) -> io::Result<()>`
- `pub fn flush(&mut self) -> io::Result<()>`

## Files

- `src/lib.rs` - implementation.
- `tests/` - integration tests; correctness, edge cases, property/stress.
- `examples/demo.rs` - stdout walkthrough.
- `examples/perf_main.rs` - bench entry (behind the `harness` feature).

## License

Dual-licensed under MIT OR Apache-2.0.
