# subms-ts-csv

Zero-dependency, hand-rolled CSV / NDJSON reader and writer to and from the
typed `TsDataFrame`, with per-column type inference. Part of the
[submillisecond](https://www.submillisecond.com) cookbook `timeseries` arc.

- **Recipe page:** <https://www.submillisecond.com/cookbook/recipes/subms-ts-csv>
- **Crate:** `subms-ts-csv` on crates.io
- **Maven:** `com.submillisecond.recipes:subms-ts-csv`

## Install

```toml
[dependencies]
subms-ts-csv = "0.6"
```

## Quickstart

```rust
use subms_ts_csv::{read_csv, write_csv, TsCsvOptions};
use subms_ts::TsDataType;

let text = "t,price,ok\n1,10.5,true\n2,11.0,false\n";
let df = read_csv(text, &TsCsvOptions::default().ts_column("t")).unwrap();
assert_eq!(df.column("price").unwrap().data_type(), TsDataType::F64);
assert_eq!(df.column("ok").unwrap().data_type(), TsDataType::Bool);

let back = write_csv(&df);
assert!(back.starts_with("price,ok\n"));
```

## What it ships

- `read_csv(text, opts)` - RFC-4180-ish parse (comma sep, `""`-escaped quoting,
  CRLF/LF) into a `TsDataFrame`. Per-column narrowest-fit inference: `I64`,
  else `F64`, else `Bool`, else `Str`. An empty cell is a gap (no point
  pushed), not a null.
- `read_ndjson(text, opts)` - one flat JSON object per line; union of keys =
  columns; an absent key is a gap; a quoted value stays `Str`.
- `write_csv(df)` - emit a frame as CSV (header + one row per aligned ts, empty
  cell for a gap, minimal quoting). Round-trips with `read_csv` for the
  inferred types.
- `TsCsvOptions { has_header, ts_column, delimiter }`, `TsCsvError`.

## Status

`0.6.0`. Zero third-party deps; `subms-ts` for the frame, `subms` (optional,
`harness` feature) for the bench. Throughput-contracted: a 4096-row, 5-column
parse runs in single-digit milliseconds (roughly a microsecond per row), so the
CI gate is a generous block-latency bound, not a per-call sub-ms claim.

## Licence

MIT OR Apache-2.0.
