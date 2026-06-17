# subms-ts-wal

submillisecond.com cookbook recipe - `timeseries`: a zero-dependency,
append-only write-ahead log for f64-valued time-series records with
truncation-safe crash recovery.

The log is a directory of fixed-width 28-byte segment records (little-endian,
CRC-32 trailer). Replay validates every record and stops cleanly at the first
torn or corrupt one, returning the valid prefix - a crash mid-append loses only
the uncommitted tail, never the whole log.

```rust
use subms_ts_wal::{TsFsyncPolicy, TsWal};

let mut wal = TsWal::open("/var/lib/app/wal", TsFsyncPolicy::EveryNAppends(64))?;
wal.append(7, 100, 1.5)?;
wal.flush()?;
let records = wal.replay()?; // Vec<TsWalRecord>
```

Independent recipe: no `subms-ts` dependency. Replayed `TsWalRecord`s are meant
to be fed into a `TsSeries` by the consumer.

## On-disk format

Segment files `wal-<10-digit-seq>.log`, each a sequence of 28-byte records:

```
[series_id u64 LE][ts i64 LE][value_bits u64 LE][crc32 u32 LE]
```

`crc32` (IEEE 0xEDB88320) covers the first 24 payload bytes. `value_bits` is
`f64::to_bits`. Byte-equivalent across the Rust + Java ports (a hex fixture pins
it). Segments roll every 4096 records.

## Bench

- `append_buffered` (policy `Never`) and `append_synced_n` (policy
  `EveryNAppends(64)`): both p99 < 1 ms.
- `Always` (fsync-per-append) is fsync-floor-limited and NOT a sub-ms claim. See
  the recipe page for the measured figure and its hardware caveats.

Server-side primitive: uses `std::fs`, does not compile to `wasm32`.

## Licence

MIT OR Apache-2.0.
