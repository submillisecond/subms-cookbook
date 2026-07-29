//! Pins the behaviour each section of `sample_app` demonstrates: per-tick
//! scratch computes its snapshot and resets to a stable chunk, and each
//! optional feature keeps its contract.

use subms_arena_allocator::Bump;

#[derive(Copy, Clone)]
struct Level {
    price_ticks: u64,
    qty: u32,
}

#[test]
fn per_tick_scratch_resets_without_realloc() {
    let mut scratch = Bump::with_capacity(4096);
    let cap = scratch.capacity();
    let updates = [
        (9998u64, 5u32, true),
        (9997, 8, true),
        (10002, 4, false),
        (10003, 9, false),
    ];

    let (mut best_bid, mut best_ask) = (0u64, u64::MAX);
    let (mut bid_qty, mut ask_qty) = (0u64, 0u64);
    for &(price, qty, is_bid) in &updates {
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
    assert_eq!((best_bid + best_ask) / 2, 10_000, "mid of the tick");
    assert_eq!(bid_qty as i64 - ask_qty as i64, 0, "balanced book");
    assert!(scratch.used() > 0);

    scratch.reset();
    assert_eq!(scratch.used(), 0, "reset rewinds the cursor");
    assert_eq!(scratch.capacity(), cap, "no reallocation across ticks");
}

#[cfg(feature = "typed")]
#[test]
fn typed_snapshot_reads_back_and_recycles() {
    use subms_arena_allocator::TypedArena;
    let mut book = TypedArena::<Level>::with_capacity(64);
    for &(price, qty) in &[(9998u64, 5u32), (9997, 8), (10002, 4), (10003, 9)] {
        book.alloc(Level {
            price_ticks: price,
            qty,
        });
    }
    assert_eq!(book.len(), 4);
    assert_eq!(book.iter().map(|l| l.qty as u64).sum::<u64>(), 26);
    assert_eq!(book.iter().map(|l| l.price_ticks).max(), Some(10_003));
    book.reset();
    assert!(book.is_empty());
}

#[cfg(feature = "growable")]
#[test]
fn growable_deep_tick_then_grow_free_steady_state() {
    use subms_arena_allocator::GrowableBump;
    let mut scratch = GrowableBump::with_capacity(256);
    for i in 0..200u64 {
        scratch.alloc_copy(Level {
            price_ticks: 10_000 + i,
            qty: 1,
        });
    }
    assert!(scratch.chunk_count() > 1, "deep book forced a grow");
    scratch.reset();
    assert_eq!(scratch.chunk_count(), 1, "reset keeps the largest chunk");
    let cap = scratch.total_capacity();
    for i in 0..50u64 {
        scratch.alloc_copy(Level {
            price_ticks: 10_000 + i,
            qty: 1,
        });
    }
    assert_eq!(
        scratch.total_capacity(),
        cap,
        "steady-state tick is grow-free"
    );
}

#[cfg(feature = "stats")]
#[test]
fn stats_counters_survive_reset() {
    use subms_arena_allocator::StatsBump;
    let mut scratch = StatsBump::with_capacity(4096);
    for _ in 0..1_000 {
        for i in 0..8u64 {
            scratch.alloc_copy(Level {
                price_ticks: 10_000 + i,
                qty: 1,
            });
        }
        scratch.reset();
    }
    let s = scratch.stats();
    assert_eq!(s.allocations, 8_000);
    assert!(s.peak_bytes > 0);
}

#[cfg(feature = "aligned")]
#[test]
fn aligned_scratch_is_cache_line_aligned() {
    use subms_arena_allocator::AlignedBump;
    let mut scratch = AlignedBump::with_capacity(1024);
    let region = scratch.alloc_aligned(64, 64);
    assert_eq!(region.as_ptr() as usize % 64, 0);
    assert_eq!(region.len(), 64);
    scratch.reset();
    assert_eq!(scratch.used(), 0);
}

#[cfg(feature = "freelist")]
#[test]
fn freelist_reuses_same_size_slot() {
    use std::alloc::Layout;
    use subms_arena_allocator::FreelistBump;
    let mut cache = FreelistBump::with_capacity(1024);
    let layout = Layout::new::<Level>();
    let first = cache.alloc_raw(layout);
    let used = cache.used();
    unsafe { cache.free(first, layout) };
    let second = cache.alloc_raw(layout);
    assert_eq!(first, second, "freed slot reused");
    assert_eq!(cache.used(), used, "reuse does not advance the cursor");
    assert_eq!(cache.reuse_hits(), 1);
}
