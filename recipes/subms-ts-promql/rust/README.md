# subms-ts-promql

A zero-dependency, hand-rolled PromQL-subset query engine over a
`TsCollection<f64>`. Parses a useful slice of PromQL (instant + range
selectors with label matchers, the `sum`/`avg`/`min`/`max`/`count`
aggregations, `rate`/`irate`/`increase`, scalar/vector binary ops, and
`offset`) and evaluates it against the series in a collection, resolving a
selector to the set of series whose `__name__` + label tags match. Part of the
[submillisecond](https://www.submillisecond.com) cookbook `timeseries` arc.

- **Recipe page:** <https://www.submillisecond.com/cookbook/recipes/subms-ts-promql>
- **Crate:** `subms-ts-promql` on crates.io
- **Maven:** `com.submillisecond.recipes:subms-ts-promql`

## Install

```toml
[dependencies]
subms-ts-promql = "0.6"
subms-ts = "0.6"
```

## Quickstart

```rust
use subms_ts::{TsCollection, TsSeriesMetadata};
use subms_ts_promql::TsPromQl;

let mut coll = TsCollection::<f64>::new();
let id = coll
    .register(
        TsSeriesMetadata::new(1, "")
            .with_tag("__name__", "http_requests_total")
            .with_tag("job", "api"),
    )
    .unwrap();
coll.push(id, 0, 0.0).unwrap();
coll.push(id, 60_000_000_000, 60.0).unwrap();
coll.push(id, 120_000_000_000, 120.0).unwrap();

let engine = TsPromQl::new(&coll);
let res = engine
    .eval_instant("rate(http_requests_total{job=\"api\"}[5m])", 120_000_000_000)
    .unwrap();
assert!((res.samples()[0].value - 1.0).abs() < 1e-9); // 1 req/s
```

## What it ships

- `TsPromQl::new(&collection)` - a read-only engine over a `TsCollection<f64>`.
- `eval_instant(query, at_ts)` -> `TsPromQlResult` - an instant vector of
  `(label set, value)` samples.
- `eval_range(query, start, end, step)` -> `TsPromQlRangeResult` - the query
  evaluated at each step.
- A hand-rolled lexer + recursive-descent parser (`parser`) and a tree-walking
  evaluator (`eval`), both std-only.

## The PromQL subset

In: instant vector selectors with `=` / `!=` / `=~` / `!~` matchers; range
vector selectors `metric{...}[5m]`; `sum`/`avg`/`min`/`max`/`count` with
`by (...)` / `without (...)`; `rate` / `irate` / `increase`; `+ - * /` binary
ops (scalar broadcast + one-to-one vector matching on the full label set);
`offset`; `s`/`m`/`h`/`d` durations.

Out (non-claims): subqueries, the `@` modifier, `histogram_quantile` and the
rest of the function library, full PCRE `=~` (only literal + `.*`, anchored),
staleness handling, `bool` modifiers, `on`/`ignoring`/`group_left` matching,
and `topk`/`bottomk`/`quantile`.

## Status

`0.6.0`. Parse is sub-ms by three orders of magnitude; eval is sub-ms p99 over
a couple hundred tagged series. Reference: Prometheus PromQL.

## Licence

MIT OR Apache-2.0.
