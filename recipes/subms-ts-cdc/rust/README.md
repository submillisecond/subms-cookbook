# subms-ts-cdc

Change-data-capture / subscribe layer over a `subms-ts` `TsCollection`. Wraps
the collection in an observable decorator that mirrors every mutation onto one
wait-free SPSC ring per subscriber. Part of the
[submillisecond](https://www.submillisecond.com) cookbook `timeseries` arc.

- **Recipe page:** <https://www.submillisecond.com/cookbook/recipes/subms-ts-cdc>
- **Crate:** `subms-ts-cdc` on crates.io
- **Maven:** `com.submillisecond.recipes:subms-ts-cdc`

## Install

```toml
[dependencies]
subms-ts-cdc = "0.6"
```

## Quickstart

```rust
use subms_ts::TsSeriesMetadata;
use subms_ts_cdc::{TsObservableCollection, TsChangeEvent};

let mut obs = TsObservableCollection::<f64>::new();
let mut sub = obs.subscribe(4096);
let id = obs.register(TsSeriesMetadata::new(1, "px")).unwrap();
obs.push(id, 1_000, 42.5).unwrap();

assert_eq!(
    sub.try_recv(),
    Some(TsChangeEvent::Push { series_id: id, ts: 1_000, value: 42.5 }),
);
```

## What it ships

- `TsObservableCollection<T>` - a `TsCollection` decorator that publishes
  `register`-less mutations (`push` / `delete_at` / `delete_range` /
  `deregister`) as `TsChangeEvent`s.
- `TsSubscription<T>` - the consumer end of one ring; `try_recv` / `drain`.
- `TsChangeEvent<T>` - Push / DeleteAt / DeleteRange / Deregister.

## Back-pressure

One SPSC ring per subscriber. A full ring drops the event and bumps
`dropped_events()`; the mutation always succeeds, so a slow consumer never
blocks the writer. With zero subscribers the publish path is a no-op.

## Status

`0.6.0`. Sub-ms p99 for `push_notify` (push with one 4096-cap subscriber) and
`recv` (single `try_recv`). In-process, ephemeral pub/sub - not durable. Pair
with `subms-ts-wal` for replay.

## Licence

MIT OR Apache-2.0.
