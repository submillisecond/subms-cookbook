# subms-ts-join

The full equi-join matrix between two heterogeneous `TsDataFrame`s on typed key
column(s): inner / left / right / outer / semi / anti, via hash join, sort-merge
join, and cross join. Keys can be `Str` (symbol), `I64` (date), `Bool`, `F64`,
or any tuple mixing them. Part of the
[submillisecond](https://www.submillisecond.com) cookbook `timeseries` arc.

- **Recipe page:** <https://www.submillisecond.com/cookbook/recipes/subms-ts-join>
- **Crate:** `subms-ts-join` on crates.io
- **Maven:** `com.submillisecond.recipes:subms-ts-join`

## Install

```toml
[dependencies]
subms-ts-join = "0.6"
```

## Quickstart

```rust
use subms_ts::{TsColumn, TsDataFrame, TsSeries};
use subms_ts_join::{hash_join, TsJoinKind};

// join quotes and trades on the STRING symbol column.
let out = hash_join(&quotes, &trades, &["sym"], &["sym"], TsJoinKind::Inner)?;
// out.column("qty").unwrap().get(i) -> None on an outer-join missing cell.
```

## What it ships

- `hash_join(left, right, &left_keys, &right_keys, TsJoinKind)` - build a hash
  table on one side keyed by the tuple of typed key cells, probe the other.
- `sort_merge_join(...)` - both sides sorted on keys, linear merge.
- `cross_join(left, right)` - the keyless cartesian product.
- `TsJoinKind { Inner, Left, Right, Outer, Semi, Anti }`.
- Output `TsJoinResult` of named `TsArray`s; outer-side missing cells carry an
  unset validity bit (the `subms-ts-expr` Arrow-style model), not a sentinel.
- Column collisions renamed `_left` / `_right`; join keys emitted once.

## Status

`0.6.0`. Throughput-contracted, NOT per-op sub-ms - a join is a whole-frame
operation; read rows/sec. asof joins live in `subms-ts-asof-join`.

## Licence

MIT OR Apache-2.0.
