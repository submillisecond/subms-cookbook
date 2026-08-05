//! Per-feature bench: sweeps each opt-in feature (`typed`, `growable`,
//! `stats`, `aligned`) across three allocation counts, lets
//! `classify_feature` DECIDE the category from the shape of that sweep, and
//! merge-writes the decision into `../.subms/features/rust.json`.
//!
//! An arena's "size" is how many allocations it is carrying, so the sweep
//! fills to N and times the allocate path there. A per-op cost that holds
//! steady as N grows is `hot-path`; one that climbs with N is `structural`.
//!
//! The sweep classifies on p50. p99 over a few dozen samples is just the worst
//! one, and a single scheduler slice is large enough to swamp the size signal
//! the sweep is reading. The p99 still goes into the manifest for the stage
//! table.
//!
//! These p99 figures describe THIS machine. They are published only when the
//! manifest is stamped `p99_source: fleet`; a local run leaves the category,
//! which is machine independent, and no published number.
//!
//! Run:
//!   cargo run --release --example perf_features \
//!       --features "harness typed growable stats aligned"

use std::collections::BTreeMap;
use std::io::{self, Write};
use std::path::PathBuf;

use subms::{SubMsFeatureManifest, SubMsP99Source, SubMsPerfHarness, classify_feature, summarize};

/// Allocation counts the sweep walks.
const SIZES: [usize; 3] = [4_096, 32_768, 262_144];

fn stage_stats(h: &SubMsPerfHarness, name: &str) -> (u64, u64) {
    summarize(h)
        .stages
        .iter()
        .find(|s| s.name == name)
        .map_or((0, 0), |s| (s.p50_ns, s.p99_ns))
}

/// (p50, p99) in ns of `op` run `n` times.
fn run_p50_p99(n: usize, mut op: impl FnMut(usize)) -> (u64, u64) {
    let mut h = SubMsPerfHarness::new("arena-feature", "rust");
    {
        let st = h.stage("op", n);
        for i in 0..n {
            st.time(|| op(i));
        }
    }
    stage_stats(&h, "op")
}

fn main() -> io::Result<()> {
    let canon = SIZES[SIZES.len() - 1];

    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(".subms")
        .join("features")
        .join("rust.json");
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    let mut manifest = SubMsFeatureManifest::load_str("rust", &existing);
    // Stamp the box these numbers came from. The bench runs wherever it is
    // invoked, so an unstamped manifest is indistinguishable from a fleet
    // capture; the renderer will not publish one it cannot attribute.
    let (source, instance) = SubMsP99Source::from_env();
    manifest.set_p99_source(source, instance.as_deref());

    // ---------- typed: one Copy type, slot handles, reuse on free ----------
    #[cfg(feature = "typed")]
    {
        use subms_arena_allocator::TypedArena;
        let sweep: Vec<(usize, u64)> = SIZES
            .iter()
            .map(|&n| {
                let mut arena: TypedArena<u64> = TypedArena::with_capacity(n);
                let (p50, _) = run_p50_p99(n, |i| {
                    std::hint::black_box(arena.alloc(i as u64));
                });
                (n, p50)
            })
            .collect();
        let (cat, reason) = classify_feature(&sweep, None, None);

        let mut arena: TypedArena<u64> = TypedArena::with_capacity(canon);
        let (_, alloc99) = run_p50_p99(canon, |i| {
            std::hint::black_box(arena.alloc(i as u64));
        });
        // Every timed op here takes the slot the previous one freed, so the
        // reuse path is what is measured rather than the append path.
        let mut churn: TypedArena<u64> = TypedArena::with_capacity(2);
        let (_, free99) = run_p50_p99(canon, |i| {
            let slot = churn.alloc(i as u64);
            churn.free(slot);
        });
        let mut p99 = BTreeMap::new();
        p99.insert("alloc".to_string(), alloc99);
        p99.insert("free".to_string(), free99);
        manifest.set_feature("typed", cat, &p99, &reason);
    }

    // ---------- growable: a new chunk when the active one runs out ----------
    #[cfg(feature = "growable")]
    {
        use subms_arena_allocator::GrowableBump;
        let sweep: Vec<(usize, u64)> = SIZES
            .iter()
            .map(|&n| {
                let mut a = GrowableBump::new();
                let (p50, _) = run_p50_p99(n, |i| {
                    std::hint::black_box(a.alloc_copy(i as u64));
                });
                (n, p50)
            })
            .collect();
        let (cat, reason) = classify_feature(&sweep, None, None);

        let mut a = GrowableBump::new();
        let (_, alloc99) = run_p50_p99(canon, |i| {
            std::hint::black_box(a.alloc_copy(i as u64));
        });
        let mut filled = GrowableBump::new();
        for i in 0..canon {
            filled.alloc_copy(i as u64);
        }
        let (_, reset99) = run_p50_p99(1, |_| filled.reset());
        let mut p99 = BTreeMap::new();
        p99.insert("alloc".to_string(), alloc99);
        p99.insert("reset".to_string(), reset99);
        manifest.set_feature("growable", cat, &p99, &reason);
    }

    // ---------- stats: live counters on the alloc path ----------
    #[cfg(feature = "stats")]
    {
        use subms_arena_allocator::StatsBump;
        let sweep: Vec<(usize, u64)> = SIZES
            .iter()
            .map(|&n| {
                let mut a = StatsBump::new();
                let (p50, _) = run_p50_p99(n, |i| {
                    std::hint::black_box(a.alloc_copy(i as u64));
                });
                (n, p50)
            })
            .collect();
        let (cat, reason) = classify_feature(&sweep, None, None);

        let mut a = StatsBump::new();
        let (_, alloc99) = run_p50_p99(canon, |i| {
            std::hint::black_box(a.alloc_copy(i as u64));
        });
        let (_, snap99) = run_p50_p99(canon, |_| {
            std::hint::black_box(a.stats());
        });
        let mut p99 = BTreeMap::new();
        p99.insert("alloc".to_string(), alloc99);
        p99.insert("stats".to_string(), snap99);
        manifest.set_feature("stats", cat, &p99, &reason);
    }

    // ---------- aligned: explicit per-allocation alignment ----------
    #[cfg(feature = "aligned")]
    {
        use subms_arena_allocator::AlignedBump;
        let sweep: Vec<(usize, u64)> = SIZES
            .iter()
            .map(|&n| {
                let mut a = AlignedBump::with_capacity(n * 16 + 4096);
                let (p50, _) = run_p50_p99(n, |_| {
                    std::hint::black_box(a.alloc_aligned(8, 8).len());
                });
                (n, p50)
            })
            .collect();
        let (cat, reason) = classify_feature(&sweep, None, None);

        let mut a = AlignedBump::with_capacity(canon * 16 + 4096);
        let (_, alloc99) = run_p50_p99(canon, |_| {
            std::hint::black_box(a.alloc_aligned(8, 8).len());
        });
        let mut p99 = BTreeMap::new();
        p99.insert("alloc_aligned".to_string(), alloc99);
        manifest.set_feature("aligned", cat, &p99, &reason);
    }

    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(&path, manifest.to_json())?;
    io::stdout().write_all(manifest.to_json().as_bytes())?;
    Ok(())
}
