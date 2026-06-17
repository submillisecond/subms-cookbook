# subms-zone-map

Per-block min/max/count index for time + value predicate pushdown - skip
blocks a query cannot touch without reading their bodies. Part of the
[submillisecond](https://www.submillisecond.com) cookbook `timeseries` arc;
pairs with [`subms-gorilla-block`](https://www.submillisecond.com/cookbook/recipes/subms-gorilla-block).

- **Recipe page:** <https://www.submillisecond.com/cookbook/recipes/subms-zone-map>
- **Crate:** `subms-zone-map` on crates.io
- **Maven:** `com.submillisecond.recipes:subms-zone-map`

## Install

```toml
[dependencies]
subms-zone-map = "0.6"
```

## Quickstart

```rust
use subms_zone_map::{TsZoneMap, TsValuePredicate, TsValueOp};
use subms_gorilla_block::TsGorillaBlock;

let mut z = TsZoneMap::new();
let mut b = TsGorillaBlock::new();
for i in 0..1_000 { b.append(1_000 + i, i as f64); }
z.observe(7, &b);
assert!(z.candidates(50_000, 60_000, None).is_empty()); // window miss -> pruned
```

## What it ships

- `TsZoneMap` - observe (from a Gorilla block's stats, or a zone directly) +
  candidates (time-window + value-predicate pruning), conservative.
- `TsZone`, `TsValuePredicate`, `TsValueOp`.

## Status

`0.6.0`. Sub-ms p99 to prune a 100,000-zone index (~181 us) and ~500 ns to
observe a zone.

## Licence

MIT OR Apache-2.0.
