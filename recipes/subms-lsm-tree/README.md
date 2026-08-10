# LSM Trees

A minimal log-structured merge tree, implemented twice - once in Java,
once in Rust - with one production optimisation (a bloom filter per
SSTable) so the read path stays submillisecond on realistic workloads.

## Install

```toml
# Cargo.toml
subms-lsm-tree = "0.10"
```

```xml
<!-- Maven -->
<dependency>
  <groupId>com.submillisecond.recipes</groupId>
  <artifactId>subms-lsm-tree</artifactId>
  <version>0.10.0</version>
</dependency>
```

The shape:

```
        writes ──► Memtable (in-memory sorted map)
                       │
                       │ flush when full
                       ▼
                    SSTable_2   ◄─── bloom filter checked before scan
                    SSTable_1   ◄─── bloom filter checked before scan
                    SSTable_0   ◄─── bloom filter checked before scan

        reads ── memtable → SSTables newest → oldest, first hit wins
```

Deletes write a **tombstone** - a marker that overrides any older value
on read and is dropped during compaction.

## Depends on

[`cookbook/recipes/subms-bloom-filter`](../subms-bloom-filter/) -
one bloom filter per SSTable, parsed out of the file's trailer.

- Rust: path dependency in `Cargo.toml`, no extra build step.
- Java: ordinary Maven dependency, but you must run `mvn install` in the
  bloom-filter project once before building this one.

## On-disk format

Each SSTable is a single file with an inline trailer:

```text
records: (key_len:u32 key:utf-8 flag:u8 value_len:u32 value:bytes)*
bloom:   <bloom-filter on-disk layout>                  ◄── from recipes/subms-bloom-filter
footer:  records_end_offset:u64 magic:u32 ("LSMT")
flag := 0x00 (present) | 0x01 (tombstone, value_len == 0)
```

All integers are big-endian. Keys within the records section are
strictly increasing. Files are immutable once written. On open the
entire file is read into memory; subsequent reads operate against the
in-memory buffer and the parsed bloom filter, so a get never touches the
filesystem after startup.

Both implementations use the **same on-disk format** so an SSTable
written by one could in principle be read by the other (not exercised
here).

## What's still missing

- **Compaction.** SSTable count grows linearly; tombstones never get
  reclaimed. The standard fix is levelled or size-tiered compaction.
- **Write-ahead log.** A memtable lost on crash is silently corrupted
  data. Real engines append every write to a sequential log before ACK
  and replay it on startup.
- **Sparse index.** Inside a single SSTable a get still scans linearly.
  A sparse index (one offset per N records) turns the per-file scan into
  a seek.
- **Concurrency.** Single-threaded by construction. Real systems
  serialise writes through one queue while reads run lock-free against
  an immutable snapshot.

## Implementations

- [`java/`](./java/) - OpenJDK 21, Maven; package `com.submillisecond.recipes.lsm`.
- [`rust/`](./rust/) - edition 2021, `std` only; crate `lsm-tree-cookbook`.

## Performance test

Each implementation ships a **perf test** that asserts p99 of every
operation stays under 1ms. The numbers cited in the
[post](../../../apps/web/content/posts/lsm-trees-from-scratch.md#submillisecond)
come from running those tests.

```sh
# Rust
cargo test --release --test sub_millisecond_bench -- --nocapture

# Java  (after `mvn install` in cookbook/recipes/subms-bloom-filter/java)
mvn -q package
java -cp target/classes com.submillisecond.recipes.lsm.SubMillisecondBench
```

## Related post

[LSM trees from scratch](../../../apps/web/content/posts/lsm-trees-from-scratch.md)
