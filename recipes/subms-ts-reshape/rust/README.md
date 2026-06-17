# subms-ts-reshape

Frame reshaping over a heterogeneous `TsDataFrame`: the long-to-wide `pivot`, the
wide-to-long `melt` (unpivot), the list-cell `explode`, the `vstack` / `hstack`
concatenators, and the row set-ops `union` / `intersect` / `except`. Part of the
[submillisecond](https://www.submillisecond.com) cookbook `timeseries` arc.

- **Recipe page:** <https://www.submillisecond.com/cookbook/recipes/subms-ts-reshape>
- **Crate:** `subms-ts-reshape` on crates.io
- **Maven:** `com.submillisecond.recipes:subms-ts-reshape`

## Install

```toml
[dependencies]
subms-ts-reshape = "0.6"
```

## Quickstart

```rust
use subms_ts_reshape::{pivot, melt, PivotAgg};

// long form (day, STRING sensor, reading) -> one column per sensor, summed.
let wide = pivot(&frame, "day", "sensor", "reading", PivotAgg::Sum)?;
// wide.column("b").get(i) -> None on an (index, column) pair with no rows.

// wide form -> long, with a real Str `variable` column naming the source slot.
let long = melt(&frame, &["day"], &["open", "close"])?;
```

## What it ships

- `pivot(frame, index_col, columns_col, values_col, agg)` - long-to-wide; one
  output column per distinct category (a `Str` symbol names its slot), each cell
  the agg of the value column over that (index, column) pair.
- `melt(frame, id_cols, value_cols)` - wide-to-long; emits a real `Str`
  `variable` column naming the source slot plus a `value` column.
- `explode(frame, list_col)` - one output row per element of a `Value` column's
  array cells; an empty list drops the row.
- `vstack(a, b)` - row concatenation (same column schema).
- `hstack(a, b)` - column concatenation (shared row axis; `_a` / `_b` on
  collisions).
- `union` / `intersect` / `except` - distinct-row set-ops on same-schema frames;
  row equality on the typed cell tuple (`Str` "3" never equals numeric 3).
- `PivotAgg { Sum, Mean, Min, Max, Last }`.
- Output `TsReshapeResult` of named `TsArray`s; absent cells carry an unset
  validity bit (the `subms-ts-expr` Arrow-style model), not a sentinel.

## Status

`0.6.0`. Throughput-contracted, NOT per-op sub-ms - reshaping is a whole-frame
operation; read rows/sec. No parallel execution, no spill-to-disk.

## Licence

MIT OR Apache-2.0.
