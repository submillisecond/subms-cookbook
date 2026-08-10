# Bloom filter

A space-efficient probabilistic set: "is this key definitely not in
the set?" answered in a handful of hash probes against a fixed bit
array. False positives are possible (configurable rate); false
negatives are not. Useful any time a real lookup is expensive and a
fast negative answer can short-circuit it - SSTable scans, cache miss
filters, log dedup checks.

## Install

```toml
# Cargo.toml
subms-bloom-filter = "0.10"
```

```xml
<!-- Maven -->
<dependency>
  <groupId>com.submillisecond.recipes</groupId>
  <artifactId>subms-bloom-filter</artifactId>
  <version>0.10.0</version>
</dependency>
```

## Sizing

The defaults are sized for ~1% false-positive rate using the standard
optimum:

```
m = -n * ln(p) / (ln 2)^2     bits
k = (m / n) * ln 2             hashes
```

At p = 0.01 this collapses to **~10 bits per key, k = 7**. Both
implementations hard-code these constants - the cookbook value is in
seeing how few moving parts the structure actually has, not in tuning.

## Implementation notes

Both implementations use **FNV-1a 64-bit** for the underlying hash and
the classic **double-hashing** trick: rather than evaluate seven
separate hash functions, evaluate one 64-bit hash and split it into two
32-bit halves `h1`, `h2`; the `i`th probe is `(h1 + i*h2) mod m`. Two
hash evaluations per `add`/`mightContain`, not seven.

The wire format is fixed (`bit_count:u32` + `k:u32` + `word_count:u32`
+ `(u64)*` words, all big-endian) and identical across languages.

## Implementations

- [`java/`](./java/) - OpenJDK 21, Maven; package `com.submillisecond.recipes.bloom`.
- [`rust/`](./rust/) - edition 2021, `std` only; crate `bloom-filter-cookbook`.

## Consumed by

- [`cookbook/recipes/subms-lsm-tree`](../subms-lsm-tree/) - one bloom
  filter per SSTable in the trailer; lets a read short-circuit before
  scanning the records.
