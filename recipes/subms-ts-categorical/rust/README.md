# subms-ts-categorical

A string-interning table and dictionary-encoded string columns - the optimizer
that turns expensive string group/join keys into cheap `u32` integer compares.
Part of the [submillisecond](https://www.submillisecond.com) cookbook
`timeseries` arc.

- **Recipe page:** <https://www.submillisecond.com/cookbook/recipes/subms-ts-categorical>
- **Crate:** `subms-ts-categorical` on crates.io
- **Maven:** `com.submillisecond.recipes:subms-ts-categorical`

## Install

```toml
[dependencies]
subms-ts-categorical = "0.6"
```

## Quickstart

```rust
use subms_ts_categorical::TsDictColumn;

let symbols = ["AAPL", "MSFT", "AAPL", "AAPL", "MSFT"];
let dict = TsDictColumn::from_strs(symbols);
assert_eq!(dict.len(), 5);
assert_eq!(dict.cardinality(), 2);   // two distinct tickers
assert_eq!(dict.codes()[0], dict.codes()[2]); // equal strings share a code
assert_eq!(dict.decode_at(1), Some("MSFT"));
```

## What it ships

- `TsStringInterner` - a stable `&str -> u32` table. `intern` dedups (same
  string returns the same id; ids dense from 0 in first-seen order); `resolve`
  reverses; `contains` / `get` / `len` round it out.
- `TsDictColumn` - a dictionary-encoded string column: `{ codes: Vec<u32>,
  dict: Vec<String> }`. `encode` a `TsSeries<String>` (or `from_strs` any
  string iterator), `codes()` hands a downstream group-by / join an int key,
  `decode_at` / `to_series` / `to_vec` round-trip the values.
- `encode_str_column` - the bridge: dict-encode a `TsColumn::Str` lifted off a
  `TsDataFrame`.

## Status

`0.6.0`. The interner is an exact, unbounded table - memory grows with the
distinct-string count. That is the right call for a bounded-alphabet column (a
`symbol` / `region` / `status` field); an unbounded distinct stream wants a
bounded / evicting variant (future work). Cross-language byte-equivalence is
not a goal: this is an in-memory optimizer, not a wire format.

## Licence

MIT OR Apache-2.0.
