# Arena allocator

Fixed-capacity bump-pointer arena with reset() for per-request reuse. Allocate is a single cursor bump; reset is constant-time. Auto-grow is opt-in via the growable feature.

Part of the [submillisecond.com cookbook](https://www.submillisecond.com/cookbook/recipes/subms-arena-allocator) - a `memory` recipe. Rust + Java, byte-equivalent JSON contract, zero runtime dependencies (`std` / JDK only; the perf harness is an opt-in Cargo feature).

## Install

```toml
# Cargo.toml
subms-arena-allocator = "0.10"
```

```xml
<!-- Maven -->
<dependency>
  <groupId>com.submillisecond.recipes</groupId>
  <artifactId>subms-arena-allocator</artifactId>
  <version>0.10.0</version>
</dependency>
```

Opt-in Cargo features: `typed`, `growable`, `stats`, `aligned` (`full` enables
all four). The Java port ships every feature in the jar under
`com.submillisecond.recipes.arena.features`.

## Docs, design, and measured benchmarks

The full writeup - implementation walkthrough, design tradeoffs, the quality-bar
contract (reference impl / claim conditions / non-claims), and the p99 numbers
captured on the stated hardware - lives at:

**https://www.submillisecond.com/cookbook/recipes/subms-arena-allocator**

## Layout

- [`rust/`](./rust/) - Rust edition 2024, `std`-only library (crate `subms_arena_allocator`).
- [`java/`](./java/) - JDK 21 (`com.submillisecond.recipes`).
- [`.subms/perf/`](./.subms/perf/) - fleet-captured latency + storage-growth JSON the site renders.
- [`.subms/features/`](./.subms/features/) - the per-language feature manifest (each opt-in feature classified from a measured size sweep).

## Thread safety

Every type in this recipe is single-threaded: the cursor is plain mutable state
with no synchronisation, and nothing here is safe to share across threads.

Rust enforces it at compile time and the crate declares no `unsafe impl Send` or
`unsafe impl Sync`. `Bump`, `GrowableBump` and `AlignedBump` hold a raw pointer,
so they are neither `Send` nor `Sync`; `TypedArena<T>` owns plain `Vec` storage
and every mutating method takes `&mut self`, so a shared reference cannot
allocate.

Java enforces nothing. A `BumpArena` reachable from two threads without external
synchronisation will hand the same offset to both callers, and the second write
silently clobbers the first. Give each thread its own arena.

## License

Dual-licensed under [MIT](../../LICENSE-MIT) OR [Apache-2.0](../../LICENSE-APACHE), at your option.
