# Treap - Rust

Probabilistic balanced BST. Random priorities + heap-on-priority + BST-on-key give expected O(log n) without colour bits or rebalancing factors, and a split/join a red-black tree cannot match.

Part of the [submillisecond.com cookbook](https://www.submillisecond.com/cookbook/recipes/subms-treap). Zero external dependencies; `std` only.

## Install

```toml
[dependencies]
subms-treap = "0.10"
```

## Quickstart

```sh
cargo test --features full
cargo run --features full --example sample_app
```

## Public API

Default path, no features:

- `Treap::new(seed)` / `with_capacity(seed, capacity)` / `from_entropy()`
- `Treap::from_sorted(seed, items) -> Result<Self, TreapError>` - O(n) bulk build
- `insert` / `get` / `get_mut` / `remove` / `contains_key` / `clear`
- `len` / `is_empty` / `height`
- `first` / `last` / `pop_first` / `pop_last`
- `floor` / `ceiling` / `predecessor` / `successor`
- `iter` / `iter_rev` / `collect_in_order`
- `range(from, to)` with `RangeBound::{Unbounded, Inclusive, Exclusive}`
- `split_off(pivot) -> Treap` / `join(other) -> Result<(), TreapError>`

Opt-in features: `persistent` (`PersistentTreap`), `merge-split`
(`SplittableTreap`), `concurrent-reads` (`TreapSnapshot`). `full` enables all
three; `harness` wires the `subms` perf harness.

## Files

- `src/lib.rs` - the treap; `src/range.rs` - bounded ordered iteration.
- `src/features/` - the opt-in capability modules, one Cargo feature each.
- `src/*_tests.rs` - colocated unit tests.
- `tests/sub_millisecond_bench.rs` - the asserted p99 gate (`harness` feature).
- `examples/sample_app.rs` - a bid-side depth book touring the whole surface.
- `examples/perf_main.rs`, `examples/perf_features.rs` - bench entry points.

## License

Dual-licensed under [MIT](../../../LICENSE-MIT) OR [Apache-2.0](../../../LICENSE-APACHE), at your option.
