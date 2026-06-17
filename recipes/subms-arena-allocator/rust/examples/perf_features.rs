//! Per-feature bench: runs a 50k-iteration arena workload against the
//! fixed-capacity base `Bump`, plus each opt-in feature (`typed`,
//! `growable`, `stats`, `aligned`, `freelist`) when its Cargo feature is
//! enabled at compile time.
//!
//! The output JSON has one stage block per variant - e.g. `base_allocate`,
//! `base_reset`, `typed_allocate`, `growable_allocate`, etc. - so the
//! cookbook page can fill the per-feature p99 table from a single file.
//!
//! The workload mirrors the per-request reuse pattern the arena is built
//! for: allocate a batch of fixed-layout values, then `reset()` to rewind
//! the cursor, repeating until ITERATIONS allocations have been timed. The
//! batch size stays well inside each arena's capacity so the base (which is
//! now fixed-capacity, not auto-growing) never overflows. The `growable`
//! stage deliberately sizes the initial chunk so the batch crosses a grow
//! boundary, exercising the chunk-allocation path the other variants skip.
//!
//! Run:
//!   cargo run --release --example perf_features \
//!       --features "harness typed growable stats aligned freelist"

use std::io::{self, Write};

use subms::{SubMsPerfHarness, SubMsStageKind, summarize, summary_to_json};

const ITERATIONS: usize = 50_000;
const SEED: u64 = 0;

// Allocate this many values between resets. 256 u64s = 2 KiB, comfortably
// inside the base's 4 KiB chunk so the fixed-capacity arena never refuses.
const BATCH: usize = 256;

fn main() -> io::Result<()> {
    let mut h = SubMsPerfHarness::new("arena-allocator-features", "rust");
    h.input("iterations", &ITERATIONS.to_string());
    h.input("seed", &SEED.to_string());
    h.input("batch", &BATCH.to_string());
    h.add_meta("subms.recipe.slug", "subms-arena-allocator");
    h.add_meta("subms.recipe.category", "memory");

    base(&mut h);

    #[cfg(feature = "typed")]
    typed(&mut h);

    #[cfg(feature = "growable")]
    growable(&mut h);

    #[cfg(feature = "stats")]
    stats(&mut h);

    #[cfg(feature = "aligned")]
    aligned(&mut h);

    #[cfg(feature = "freelist")]
    freelist(&mut h);

    let summary = summarize(&h);
    let mut stdout = io::stdout();
    summary_to_json(&summary, &mut stdout)?;
    writeln!(stdout)?;
    Ok(())
}

// ---------- base ----------
// Fixed-capacity single-chunk arena: allocate a batch, reset, repeat. The
// reset stage is sampled once per batch (a constant-time cursor rewind).
fn base(h: &mut SubMsPerfHarness) {
    use subms_arena_allocator::Bump;
    h.add_meta("subms.workload.feature", "base");

    let mut a = Bump::with_capacity(4096);
    let mut counter = 0u64;
    {
        let alloc_stage = h
            .stage("base_allocate", ITERATIONS)
            .with_kind(SubMsStageKind::HotPath);
        for i in 0..ITERATIONS {
            counter = counter.wrapping_add(1);
            alloc_stage.time(|| {
                let _ = a.alloc_copy(counter);
            });
            if (i + 1) % BATCH == 0 {
                a.reset();
            }
        }
    }
    let reset_stage = h
        .stage("base_reset", ITERATIONS / BATCH + 1)
        .with_kind(SubMsStageKind::HotPath);
    a.reset();
    for _ in 0..(ITERATIONS / BATCH) {
        let _ = a.alloc_copy(counter);
        reset_stage.time(|| a.reset());
    }
}

// ---------- typed ----------
// TypedArena<u64> over a preallocated Vec: allocate to capacity, reset,
// repeat. alloc takes &self (interior mutability), reset takes &mut self.
#[cfg(feature = "typed")]
fn typed(h: &mut SubMsPerfHarness) {
    use subms_arena_allocator::TypedArena;
    h.add_meta("subms.workload.feature", "typed");

    let mut a = TypedArena::<u64>::with_capacity(BATCH);
    let mut counter = 0u64;
    {
        let alloc_stage = h
            .stage("typed_allocate", ITERATIONS)
            .with_kind(SubMsStageKind::HotPath);
        for i in 0..ITERATIONS {
            counter = counter.wrapping_add(1);
            alloc_stage.time(|| {
                let _ = a.alloc(counter);
            });
            if (i + 1) % BATCH == 0 {
                a.reset();
            }
        }
    }
    let reset_stage = h
        .stage("typed_reset", ITERATIONS / BATCH + 1)
        .with_kind(SubMsStageKind::HotPath);
    a.reset();
    for _ in 0..(ITERATIONS / BATCH) {
        let _ = a.alloc(counter);
        reset_stage.time(|| a.reset());
    }
}

// ---------- growable ----------
// Auto-grow arena: size the initial chunk so a BATCH crosses a grow
// boundary, exercising the chunk-allocation path. reset() keeps the largest
// chunk so steady-state batches settle on a single chunk.
#[cfg(feature = "growable")]
fn growable(h: &mut SubMsPerfHarness) {
    use subms_arena_allocator::GrowableBump;
    h.add_meta("subms.workload.feature", "growable");

    // 512-byte initial chunk = 64 u64s; a 256-batch forces grows on the
    // first batch, then reset() retains the grown chunk for steady state.
    let mut a = GrowableBump::with_capacity(512);
    let mut counter = 0u64;
    {
        let alloc_stage = h
            .stage("growable_allocate", ITERATIONS)
            .with_kind(SubMsStageKind::HotPath);
        for i in 0..ITERATIONS {
            counter = counter.wrapping_add(1);
            alloc_stage.time(|| {
                let _ = a.alloc_copy(counter);
            });
            if (i + 1) % BATCH == 0 {
                a.reset();
            }
        }
    }
    let reset_stage = h
        .stage("growable_reset", ITERATIONS / BATCH + 1)
        .with_kind(SubMsStageKind::HotPath);
    a.reset();
    for _ in 0..(ITERATIONS / BATCH) {
        let _ = a.alloc_copy(counter);
        reset_stage.time(|| a.reset());
    }
}

// ---------- stats ----------
// Instrumented arena: allocate (counter writes per call) + snapshot the
// live BumpStats. snapshot() is a struct copy, sampled per allocation.
#[cfg(feature = "stats")]
fn stats(h: &mut SubMsPerfHarness) {
    use subms_arena_allocator::StatsBump;
    h.add_meta("subms.workload.feature", "stats");

    let mut a = StatsBump::with_capacity(4096);
    let mut counter = 0u64;
    {
        let alloc_stage = h
            .stage("stats_allocate", ITERATIONS)
            .with_kind(SubMsStageKind::HotPath);
        for i in 0..ITERATIONS {
            counter = counter.wrapping_add(1);
            alloc_stage.time(|| {
                let _ = a.alloc_copy(counter);
            });
            if (i + 1) % BATCH == 0 {
                a.reset();
            }
        }
    }
    let snap_stage = h
        .stage("stats_snapshot", ITERATIONS)
        .with_kind(SubMsStageKind::HotPath);
    for _ in 0..ITERATIONS {
        snap_stage.time(|| {
            let _ = std::hint::black_box(a.stats());
        });
    }
}

// ---------- aligned ----------
// Cache-line allocations: alloc_aligned(64, 64) repeatedly, reset per batch.
#[cfg(feature = "aligned")]
fn aligned(h: &mut SubMsPerfHarness) {
    use subms_arena_allocator::AlignedBump;
    h.add_meta("subms.workload.feature", "aligned");

    // 64-byte slots: a 256-batch is 16 KiB, so size the chunk for the batch.
    let chunk = BATCH * 64 + 64;
    let mut a = AlignedBump::with_capacity(chunk);
    {
        let alloc_stage = h
            .stage("aligned_allocate_aligned", ITERATIONS)
            .with_kind(SubMsStageKind::HotPath);
        for i in 0..ITERATIONS {
            alloc_stage.time(|| {
                let s = a.alloc_aligned(64, 64);
                std::hint::black_box(s.as_ptr());
            });
            if (i + 1) % BATCH == 0 {
                a.reset();
            }
        }
    }
}

// ---------- freelist ----------
// Per-(size, align) reuse: alloc a slot, free it, alloc again (reuse hit).
// The free + reuse pair is the steady-state object-pool shape this variant
// targets, so both the allocate (reuse path) and free stages get sampled.
#[cfg(feature = "freelist")]
fn freelist(h: &mut SubMsPerfHarness) {
    use std::alloc::Layout;
    use subms_arena_allocator::FreelistBump;
    h.add_meta("subms.workload.feature", "freelist");

    let layout = Layout::new::<u64>();
    let mut a = FreelistBump::with_capacity(4096);

    // Prime one slot so the very first timed alloc hits the freelist.
    let primed = a.alloc_raw(layout);
    unsafe { a.free(primed, layout) };

    let mut held: *mut u8 = std::ptr::null_mut();
    {
        let alloc_stage = h
            .stage("freelist_allocate", ITERATIONS)
            .with_kind(SubMsStageKind::HotPath);
        for _ in 0..ITERATIONS {
            alloc_stage.time(|| {
                held = a.alloc_raw(layout);
            });
            // Return it so the next iteration reuses the same slot. Timed
            // separately below; here we just keep the freelist warm.
            unsafe { a.free(held, layout) };
        }
    }

    // Re-prime, then time the free path on its own.
    let p = a.alloc_raw(layout);
    let free_stage = h
        .stage("freelist_free", ITERATIONS)
        .with_kind(SubMsStageKind::HotPath);
    let mut cur = p;
    for _ in 0..ITERATIONS {
        free_stage.time(|| unsafe { a.free(cur, layout) });
        // Pull it back out (reuse hit, untimed) so the next free has a slot.
        cur = a.alloc_raw(layout);
    }
}
