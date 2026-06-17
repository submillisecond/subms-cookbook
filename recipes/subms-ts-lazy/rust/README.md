# subms-ts-lazy

submillisecond.com cookbook recipe - `timeseries`: a deferred `LazyTsFrame`
query planner over the typed `TsDataFrame`, with result-preserving optimiser
passes, that lowers to a `subms-ts-plan` latency certificate.

This is the analytical engine's capstone: it weaponises the per-op p99
contracts the rest of the arc publishes into a per-QUERY guarantee. A lazy
query emits a signed `TsLatencyCertificate` for the whole pipeline - the only
analytical engine you can put in a latency SLA.

## Install

```toml
[dependencies]
subms-ts-lazy = "0.6"
```

## Quickstart

```rust
use subms_ts::{TsColumn, TsDataFrame, TsSeries};
use subms_ts_expr::TsExpr;
use subms_ts_lazy::LazyTsFrame;

let cert = LazyTsFrame::new(frame)
    .filter(TsExpr::col("px").gt(TsExpr::lit_f64(2.0)))
    .with_column("notional", TsExpr::col("px").mul(TsExpr::col("qty")))
    .select(&["notional"])
    .certify("ci-dedicated", 0);

assert!(cert.meets_budget(1_000_000)); // composes to a sub-ms certify budget
assert!(cert.verify());
```

`collect()` runs the optimised plan and returns a `ResultFrame`;
`into_data_frame()` rounds it back to a `TsDataFrame`. `explain()` renders the
optimised plan; `optimise()` returns the rewritten lazy frame.

## Scope

A LINEAR pipeline: `select`, `filter`, `with_column`, `sort_by`, `limit`, and a
terminal `agg`. Each is expressible via `subms-ts-expr` eval, so this recipe
depends on `subms-ts` + `subms-ts-expr` + `subms-ts-plan` and nothing else in
the operator arc. Group-by and join are the eager standalone operators you
compose AROUND a lazy pipeline.

## Optimiser passes

- PREDICATE PUSHDOWN - slide each filter as early as legal (above any
  `with_column` it does not read, above a `select` that still carries its
  columns).
- PROJECTION PUSHDOWN - drop source columns no downstream node references,
  before the scan does per-row work.
- REDUNDANT-PROJECTION ELIMINATION - collapse adjacent / no-op selects.

All passes are result-preserving; a test pins `optimised collect == unoptimised
collect`.

## Status

- Throughput-contracted `collect`; sub-ms-asserted `certify`.
- Rust + Java parity; certificate JSON byte-equivalent across both.

## Licence

MIT OR Apache-2.0.
