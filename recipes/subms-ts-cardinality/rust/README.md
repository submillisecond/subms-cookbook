# subms-ts-cardinality

Series-count caps, per-tenant cardinality limits, and idempotent-ingest dedup
for a multi-tenant `TsCollection`. Part of the
[submillisecond](https://www.submillisecond.com) cookbook `timeseries` arc.

- **Recipe page:** <https://www.submillisecond.com/cookbook/recipes/subms-ts-cardinality>
- **Crate:** `subms-ts-cardinality` on crates.io
- **Maven:** `com.submillisecond.recipes:subms-ts-cardinality`

## Install

```toml
[dependencies]
subms-ts-cardinality = "0.6"
```

## Quickstart

```rust
use subms_ts_cardinality::{TsCardinalityGuard, TsOverflowPolicy};

let mut guard = TsCardinalityGuard::new(2, TsOverflowPolicy::Reject);
assert!(guard.admit().is_ok());
assert!(guard.admit().is_ok());
assert!(guard.admit().is_err()); // at the cap
guard.release();
assert!(guard.admit().is_ok());  // a slot opened up
```

## What it ships

- `TsCardinalityGuard` - a global series-count cap. `admit` / `release`,
  `count` / `remaining` / `over_count`, under a `Reject` or `Allow`
  `TsOverflowPolicy`.
- `TsTenantedGuard` - an independent per-tenant cap keyed on `TsTenantId`, so
  one tenant at its limit never blocks another.
- `TsDedupFilter` - exact idempotent ingest over `TsIngestKey { series_id,
  sequence }`. `is_new` is true once per key, false on replay.
- `TsGuardedCollection` - a thin decorator that owns a `TsCollection` plus a
  guard and enforces the cap on `register`; reads delegate straight through.

All three primitives are standalone and std-only over `subms-ts`; none of them
mutate the subms-ts core.

## Status

`0.6.0`. Pure counter / hash-set arithmetic; sub-ms p99 on the `admit` and
`dedup` decisions. The dedup filter is an exact `HashSet`, so its memory grows
with the number of distinct keys seen - bound the dedup window upstream or
`reset` it at a flush boundary. A bounded / rolling-bloom variant is future
work.

## Licence

MIT OR Apache-2.0.
