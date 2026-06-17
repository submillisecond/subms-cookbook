# subms-ts-plan

Compose the per-operation p99 contracts the cookbook recipes publish into one
system-level latency certificate an SRE can put in an SLA. Part of the
[submillisecond](https://www.submillisecond.com) cookbook `timeseries` arc.

- **Recipe page:** <https://www.submillisecond.com/cookbook/recipes/subms-ts-plan>
- **Crate:** `subms-ts-plan` on crates.io
- **Maven:** `com.submillisecond.recipes:subms-ts-plan`

## Install

```toml
[dependencies]
subms-ts-plan = "0.6"
```

## Quickstart

```rust
use subms_ts_plan::TsPlan;

let cert = TsPlan::new()
    .then("subms-zone-map", "candidates", 500_000)
    .then("subms-gorilla-block", "range_scan", 37_100)
    .then("subms-ts", "range_min", 900)
    .then("subms-tdigest", "quantile", 300)
    .with_overhead(50_000)
    .certify("ci-dedicated", 0);

assert!(cert.meets_budget(1_000_000)); // composes to < 1 ms p99
assert!(cert.verify());                // integrity hash checks out
```

## What it ships

- `TsPlan` - an ordered list of recipe+stage citations plus a planner
  overhead; `total_p99_ns` sums them. `certify(tier, valid_until)` freezes a
  `TsLatencyCertificate` carrying the breakdown, the composed p99, and a
  deterministic FNV-1a integrity hash over its canonical JSON (`to_json`).
  `verify()` recomputes the hash; `meets_budget(ns)` checks the SLA.

## Status

`0.6.0`. Pure compute, std-only; `certify` + `verify` are sub-ms p99. The
integrity hash is tamper-evidence, byte-equivalent across Rust + Java (a JSON
fixture pins it); it is NOT a cryptographic signature - to sign for real, run
your signer over `to_json()` (the key is yours, so it stays a hook).

## Licence

MIT OR Apache-2.0.
