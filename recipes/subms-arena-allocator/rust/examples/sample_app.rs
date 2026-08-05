//! Sample app: a tour of `subms-arena-allocator` for per-tick / per-request
//! scratch, base API first, then each optional feature. Run the base with
//! `cargo run --example sample_app`; add `--all-features` (or a subset like
//! `--features typed`) to light up the feature sections.
//!
//! * base       - per-tick order-book scratch, reset between ticks
//! * typed      - TypedArena<Level>: slot handles, freed slots reused
//! * growable   - a deep-book tick outgrows the chunk; the arena grows
//! * stats      - lifetime counters to size the arena from real load
//! * aligned    - cache-line-aligned scratch for a SIMD-style price scan

use subms_arena_allocator::Bump;

#[derive(Copy, Clone)]
struct Level {
    price_ticks: u64,
    qty: u32,
}

fn main() {
    base_per_tick_scratch();

    #[cfg(feature = "typed")]
    typed_snapshot();

    #[cfg(feature = "growable")]
    growable_deep_book();

    #[cfg(feature = "stats")]
    stats_sizing();

    #[cfg(feature = "aligned")]
    aligned_price_scan();
}

/// Base API: on every market-data tick the visible price levels land in one
/// fixed scratch chunk as throwaway `Copy` structs; we fold each into the mid
/// and the resting-quantity imbalance, then `reset()` for the next tick. The
/// chunk is sized once, so the steady state pays no allocator round-trip and
/// never touches the global malloc or the GC.
fn base_per_tick_scratch() {
    println!("== base: per-tick order-book scratch ==");
    let mut scratch = Bump::with_capacity(4096);
    let cap = scratch.capacity();

    let ticks: [&[(u64, u32, bool)]; 2] = [
        &[
            (9998, 5, true),
            (9997, 8, true),
            (10002, 4, false),
            (10003, 9, false),
        ],
        &[(9999, 3, true), (10001, 7, false)],
    ];

    for (t, updates) in ticks.iter().enumerate() {
        let (mut best_bid, mut best_ask) = (0u64, u64::MAX);
        let (mut bid_qty, mut ask_qty) = (0u64, 0u64);
        for &(price, qty, is_bid) in updates.iter() {
            let level = scratch.alloc_copy(Level {
                price_ticks: price,
                qty,
            });
            if is_bid {
                best_bid = best_bid.max(level.price_ticks);
                bid_qty += level.qty as u64;
            } else {
                best_ask = best_ask.min(level.price_ticks);
                ask_qty += level.qty as u64;
            }
        }
        let mid = (best_bid + best_ask) / 2;
        let imbalance = bid_qty as i64 - ask_qty as i64;
        println!(
            "  tick {t}: {} levels, mid={mid} imbalance={imbalance:+} used={}B",
            updates.len(),
            scratch.used(),
        );
        assert!(scratch.used() > 0, "levels consumed scratch");
        scratch.reset();
        assert_eq!(scratch.used(), 0, "reset rewinds the cursor");
        assert_eq!(scratch.capacity(), cap, "no reallocation between ticks");
    }
    println!("  -> steady-state chunk stays at {cap}B across all ticks");
}

/// `typed` feature: `TypedArena<Level>` when every scratch object is the same
/// compile-time type. `alloc` hands back an opaque `Slot` that `get` reads,
/// a cancelled level is `free`d back to the arena, and the next `alloc` takes
/// that slot instead of consuming a fresh one - so an order cache that churns
/// inside one tick stops advancing the high-water mark.
#[cfg(feature = "typed")]
fn typed_snapshot() {
    use subms_arena_allocator::TypedArena;
    println!("\n== typed: per-tick levels with slot reuse ==");
    let mut book = TypedArena::<Level>::with_capacity(64);
    let mut live = Vec::new();
    for &(price, qty) in &[(9998u64, 5u32), (9997, 8), (10002, 4), (10003, 9)] {
        live.push(book.alloc(Level {
            price_ticks: price,
            qty,
        }));
    }
    let resting: u64 = live.iter().map(|s| book.get(s).qty as u64).sum();
    let top = live.iter().map(|s| book.get(s).price_ticks).max().unwrap();
    println!(
        "  {} levels, {resting} resting, top price {top}",
        book.len()
    );
    assert_eq!(book.len(), 4);
    assert_eq!(resting, 26);

    let cancelled = live.pop().expect("a level to cancel");
    let freed_index = cancelled.index();
    book.free(cancelled);
    let replacement = book.alloc(Level {
        price_ticks: 10_004,
        qty: 2,
    });
    println!(
        "  cancelled level {freed_index} reused by the replacement: {} ({} reuse hits)",
        replacement.index() == freed_index,
        book.reuse_hits(),
    );
    assert_eq!(replacement.index(), freed_index, "freed slot came back");
    assert_eq!(book.reuse_hits(), 1);
    assert_eq!(book.len(), 4, "cancel + replace is footprint-neutral");

    book.reset();
    assert!(book.is_empty(), "reset recycles the snapshot storage");
}

/// `growable` feature: a tick with an unusually deep book overflows the initial
/// chunk. `GrowableBump` opens a fresh chunk instead of failing; `reset()` keeps
/// only the largest chunk, so subsequent ticks run grow-free.
#[cfg(feature = "growable")]
fn growable_deep_book() {
    use subms_arena_allocator::GrowableBump;
    println!("\n== growable: a deep-book tick that outgrows the chunk ==");
    let mut scratch = GrowableBump::with_capacity(256);
    for i in 0..200u64 {
        scratch.alloc_copy(Level {
            price_ticks: 10_000 + i,
            qty: 1,
        });
    }
    let grown = scratch.chunk_count();
    println!(
        "  200 levels -> {grown} chunks, {}B retained",
        scratch.total_capacity()
    );
    assert!(grown > 1, "deep book forced a grow");
    scratch.reset();
    assert_eq!(
        scratch.chunk_count(),
        1,
        "reset keeps only the largest chunk"
    );
    let cap_after = scratch.total_capacity();
    for i in 0..50u64 {
        scratch.alloc_copy(Level {
            price_ticks: 10_000 + i,
            qty: 1,
        });
    }
    assert_eq!(
        scratch.total_capacity(),
        cap_after,
        "steady-state tick is grow-free"
    );
}

/// `stats` feature: `StatsBump` keeps lifetime counters across resets, so a
/// long-running feed handler can size the arena from observed `peak_bytes` and
/// watch `bytes_wasted` for alignment-padding creep.
#[cfg(feature = "stats")]
fn stats_sizing() {
    use subms_arena_allocator::StatsBump;
    println!("\n== stats: size the arena from real load ==");
    let mut scratch = StatsBump::with_capacity(4096);
    for _tick in 0..1_000 {
        for i in 0..8u64 {
            scratch.alloc_copy(Level {
                price_ticks: 10_000 + i,
                qty: 1,
            });
        }
        scratch.reset();
    }
    let s = scratch.stats();
    println!(
        "  {} allocs over 1000 ticks, peak {}B, wasted {}B",
        s.allocations, s.peak_bytes, s.bytes_wasted,
    );
    assert_eq!(s.allocations, 8_000, "counters survive reset");
    assert!(s.peak_bytes > 0, "peak recorded");
}

/// `aligned` feature: `AlignedBump` hands out cache-line-aligned scratch for a
/// SIMD-style scan over a tick's prices. The backing buffer is 64-byte aligned,
/// so the first cache-line request pays zero padding.
#[cfg(feature = "aligned")]
fn aligned_price_scan() {
    use subms_arena_allocator::AlignedBump;
    println!("\n== aligned: cache-line scratch for a price scan ==");
    let mut scratch = AlignedBump::with_capacity(1024);
    let region = scratch.alloc_aligned(64, 64);
    assert_eq!(region.as_ptr() as usize % 64, 0, "cache-line aligned");
    for (i, b) in region.iter_mut().enumerate() {
        *b = i as u8;
    }
    let checksum: u32 = region.iter().map(|&b| b as u32).sum();
    println!(
        "  64B aligned scratch, checksum {checksum}, used {}B",
        scratch.used()
    );
    assert_eq!(checksum, (0..64u32).sum::<u32>());
    scratch.reset();
    assert_eq!(scratch.used(), 0);
}
