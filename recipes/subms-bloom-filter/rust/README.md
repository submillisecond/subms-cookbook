# Bloom filter - Rust

Edition 2021, stable rustc. Zero external dependencies - `std` only.
Standalone reusable crate; intended to be picked up by other cookbook
samples via path dependency.

```sh
cargo test --release
```

## Public API

- `BloomFilter::new(expected_entries)` - sized at ~10 bits/key, k=7.
- `add(&mut self, key: &str)`
- `might_contain(&self, key: &str) -> bool`
- `write_to(&self, out: &mut impl Write)` / `parse(buf: &[u8]) -> io::Result<Self>`
  - for embedding in a larger file format.

## Consumed by

- [`cookbook/recipes/subms-lsm-tree/rust/`](../../subms-lsm-tree/rust/) -
  one bloom filter per SSTable, parsed out of the file's trailer.

## Files

- `src/lib.rs` - implementation. FNV-1a 64-bit produces two 32-bit
  subhashes for the double-hashing trick.
- `tests/bloom_filter_tests.rs` - present-key round-trip, serialisation round-trip,
  empty-filter sanity, and a statistical FPR check.
