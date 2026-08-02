//! Feature classification bench. Each feature's representative op is swept
//! across three input sizes, `classify_feature` DECIDES the category from the
//! shape of that sweep, and the decision plus a measured `p99ByStage` is
//! merge-written into `.subms/features/rust.json`.
//!
//! The sweep axis is the TOTAL number of elements across the 16 merged streams.
//! A k-way merge step is O(log k) in the number of streams and independent of
//! how many elements sit behind them, so a per-element feature should read flat
//! and the sweep is here to prove that rather than assume it.
//!
//! Two measurement decisions carry the whole file:
//!
//! - A measurement TIMES A FIXED NUMBER OF ELEMENTS however large the input is.
//!   Timing a whole drain would report the size of the ANSWER: 64x the elements
//!   take 64x as long at an unchanged per-element cost, and every feature would
//!   classify structural. The drain still visits every element, so the working
//!   set grows with the sweep, but only `SAMPLES` batches of it are timed.
//! - Elements are timed in BATCHES of `BATCH` and the recorded figure is the
//!   batch mean. A merge step costs ~15 ns here and this platform's clock ticks
//!   at 100 ns, so an unbatched sample reads 0 or 100 ns: 41% of single-step
//!   timings came back as zero and the p50 of every variant, base included, was
//!   exactly one tick. Unbatched, this bench measures the clock.
//!
//! Run:
//!   cargo run --release --example perf_features \
//!       --features "harness seek-to tombstones dedup priority"

use std::collections::BTreeMap;
use std::hint::black_box;
use std::io::{self, Write};
use std::path::PathBuf;
use std::time::Instant;

use subms::{
    SubMsFeatureManifest, SubMsP99Source, SubMsPerfHarness, SubMsTimer, classify_feature, summarize,
};

use subms_merge_iterator::MergeIterator;
#[cfg(feature = "seek-to")]
use subms_merge_iterator::SeekableMergeIterator;
#[cfg(feature = "dedup")]
use subms_merge_iterator::{DedupEntry, DedupMergeIterator};
#[cfg(feature = "priority")]
use subms_merge_iterator::{PriorityEntry, PriorityMergeIterator, PrioritySource};
#[cfg(feature = "tombstones")]
use subms_merge_iterator::{TombstoneEntry, TombstoneMergeIterator};

/// Total elements across all streams: a 64x span. The bottom of the range is
/// already past the point where the 16-entry heap fits in L1 with room to
/// spare, so a per-element cost that still climbed would be a real cache
/// effect rather than a fixed per-call cost being amortised away.
const SIZES: [usize; 3] = [32_768, 262_144, 2_097_152];
const CANON: usize = SIZES[SIZES.len() - 1];
const STREAMS: usize = 16;

/// Elements per timed sample. The recorded value is the batch mean, which is
/// what lifts a ~15 ns step clear of the 100 ns clock tick.
const BATCH: usize = 64;
/// Timed batches per measurement. Fixed across the sweep, so the statistic is
/// the cost of ONE element and not the length of the drain.
const SAMPLES: usize = 512;
/// A short input runs out of batches before it runs out of samples, so the
/// drain repeats over a freshly built iterator until the sample count is met.
/// Without it the smallest sweep point was decided by a quarter of the samples
/// of the largest, and a single scheduling blip moved it by 30%.
const MAX_PASSES: usize = 16;

const WARM_NANOS: u64 = 300_000_000;
const WARM_MAX_REPS: usize = 64;

/// Keys skipped per `seek`. Held CONSTANT across the sweep for the same reason
/// the timed element count is: `seek` walks each stream forward one entry at a
/// time until it reaches the target, so its cost is set by the skip distance,
/// not by how many elements sit beyond it. Spreading a fixed number of seeks
/// over a growing key range would sweep the skip distance and call it size.
#[cfg(feature = "seek-to")]
const SEEK_SKIP: u64 = 64;
/// Seeks per pass, capped so a pass consumes at most half the smallest input.
#[cfg(feature = "seek-to")]
const SEEK_ROUNDS: usize = 256;
/// Seeks per timed sample. A seek over 64 keys costs a few hundred ns, which is
/// three or four clock ticks, and the unbatched curve jittered by a full tick
/// between sweep points. Batching buys the resolution back.
#[cfg(feature = "seek-to")]
const SEEK_BATCH: usize = 2;
#[cfg(feature = "seek-to")]
const SEEK_NEXT_ROUNDS: usize = 128;
/// Passes over a freshly built iterator. One pass cannot yield `SAMPLES`
/// without running the skip distance up with it, and the skip distance is the
/// one thing this measurement holds fixed. Kept as low as the sample count
/// allows: a pass builds the whole n-element input to seek over the first
/// 16k of it, and in the Java port that garbage is what a later measurement
/// ends up collecting.
#[cfg(feature = "seek-to")]
const SEEK_PASSES: usize = 4;

/// Stream `s` carries values `s, s+STREAMS, s+2*STREAMS, ...` so the 16 streams
/// interleave into the dense range `0..n` with no gaps.
fn plain_streams(n: usize) -> Vec<std::vec::IntoIter<u64>> {
    let per = n / STREAMS;
    (0..STREAMS)
        .map(|s| {
            (0..per)
                .map(move |i| (s + i * STREAMS) as u64)
                .collect::<Vec<u64>>()
                .into_iter()
        })
        .collect()
}

fn stat(h: &SubMsPerfHarness, median: bool) -> u64 {
    summarize(h)
        .stages
        .iter()
        .find(|s| s.name == "op")
        .map_or(0, |s| if median { s.p50_ns } else { s.p99_ns })
}

fn harness() -> SubMsPerfHarness {
    SubMsPerfHarness::new("merge-iterator-feature", "rust")
}

/// Runs `measure` into a throwaway harness until the budget expires, then once
/// more into the harness whose numbers are kept.
///
/// Warming the WORK is not enough: warm has to run the identical TIMED path.
/// With warm draining the iterator untimed, the first measurement a process
/// made still read about 20% high, and it stayed 20% high with the sweep
/// reversed - so it was the first entry into the timed region, not the size.
/// Time-boxed rather than a fixed rep count, or a cheap size gets the same
/// handful of passes as an expensive one.
fn warmed(mut measure: impl FnMut(&mut SubMsPerfHarness), median: bool) -> u64 {
    let start = Instant::now();
    for _ in 0..WARM_MAX_REPS {
        let mut scratch = harness();
        measure(&mut scratch);
        black_box(stat(&scratch, median));
        if start.elapsed().as_nanos() as u64 >= WARM_NANOS {
            break;
        }
    }
    let mut h = harness();
    measure(&mut h);
    stat(&h, median)
}

/// Per-element cost of a merge step, in ns. `make` builds a fresh iterator
/// OUTSIDE the timed region (a merge iterator is single-use, so the input has
/// to be rebuilt per pass and building it is not the thing being measured).
/// The drain consumes the whole input; every `stride`th batch is timed, which
/// keeps the sample count fixed while the working set grows with `n`.
fn per_element<It: Iterator>(
    mut make: impl FnMut() -> It,
    expected_out: usize,
    median: bool,
) -> u64 {
    let stride = (expected_out / (BATCH * SAMPLES)).max(1);
    warmed(
        |h| {
            let st = h.stage("op", SAMPLES + 1);
            let mut recorded = 0usize;
            for _ in 0..MAX_PASSES {
                if recorded >= SAMPLES {
                    break;
                }
                let mut it = make();
                let mut batch = 0usize;
                loop {
                    let timed = batch % stride == 0;
                    let mut taken = 0usize;
                    if timed {
                        let t0 = SubMsTimer::tick();
                        while taken < BATCH && it.next().is_some() {
                            taken += 1;
                        }
                        let ns = t0.elapsed_ns();
                        if taken == BATCH {
                            st.record(ns / BATCH as u64);
                            recorded += 1;
                        }
                    } else {
                        while taken < BATCH && it.next().is_some() {
                            taken += 1;
                        }
                    }
                    if taken < BATCH {
                        break;
                    }
                    batch += 1;
                }
            }
        },
        median,
    )
}

/// Sweeps and PRINTS the curve. A ratio-compressed or non-monotonic sweep
/// classifies flat, and the only way to catch one is to read the rows.
fn sweep(label: &str, mut at: impl FnMut(usize) -> u64) -> Vec<(usize, u64)> {
    let rows: Vec<(usize, u64)> = SIZES.iter().map(|&n| (n, at(n))).collect();
    eprintln!("sweep {label}: {rows:?}");
    rows
}

#[cfg(feature = "seek-to")]
fn seek_only(n: usize, median: bool) -> u64 {
    warmed(
        |h| {
            let st = h.stage("op", SEEK_PASSES * SEEK_ROUNDS / SEEK_BATCH + 1);
            for _ in 0..SEEK_PASSES {
                let mut it = SeekableMergeIterator::new(plain_streams(n));
                let mut r = 0usize;
                while r < SEEK_ROUNDS {
                    let t0 = SubMsTimer::tick();
                    for _ in 0..SEEK_BATCH {
                        r += 1;
                        it.seek(&(r as u64 * SEEK_SKIP));
                    }
                    st.record(t0.elapsed_ns() / SEEK_BATCH as u64);
                }
            }
        },
        median,
    )
}

/// Streaming cost of the `next` calls that follow a seek. Batched for the same
/// clock-tick reason as every other per-element figure, so the first element of
/// each batch is the one that lands right after the seek.
#[cfg(feature = "seek-to")]
fn seek_then_next(n: usize, median: bool) -> u64 {
    let stride = (SEEK_SKIP as usize + BATCH) as u64;
    warmed(
        |h| {
            let st = h.stage("op", SEEK_PASSES * SEEK_NEXT_ROUNDS + 1);
            for _ in 0..SEEK_PASSES {
                let mut it = SeekableMergeIterator::new(plain_streams(n));
                for r in 0..SEEK_NEXT_ROUNDS {
                    it.seek(&(r as u64 * stride));
                    let t0 = SubMsTimer::tick();
                    let mut taken = 0usize;
                    while taken < BATCH && it.next().is_some() {
                        taken += 1;
                    }
                    let ns = t0.elapsed_ns();
                    if taken == BATCH {
                        st.record(ns / BATCH as u64);
                    }
                }
            }
        },
        median,
    )
}

fn main() -> io::Result<()> {
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

    // The baseline: a plain merge step with no feature enabled. Every feature
    // decorates this step, so it is what they are classified against. Swept as
    // well as measured, because a base that itself drifted with size would make
    // every feature's flat reading meaningless.
    let base_sw = sweep("base/next", |n| {
        per_element(|| MergeIterator::new(plain_streams(n)), n, true)
    });
    let base_p50 = base_sw[base_sw.len() - 1].1;
    eprintln!("base next p50: {base_p50}ns/element");

    // ---------- seek-to: skip forward past a key ----------
    #[cfg(feature = "seek-to")]
    {
        let sw = sweep("seek-to/seek", |n| seek_only(n, true));
        let (cat, reason) = classify_feature(&sw, Some(base_p50), None);

        let mut p99 = BTreeMap::new();
        p99.insert("seek".to_string(), seek_only(CANON, false));
        p99.insert("next_after_seek".to_string(), seek_then_next(CANON, false));
        manifest.set_feature("seek-to", cat, &p99, &reason);
    }

    // ---------- tombstones: delete markers mask same-key entries ----------
    #[cfg(feature = "tombstones")]
    {
        // Every 8th key is a tombstone, so one next in eight pops twice and
        // loops to find the next live key. The decoration is per element.
        let sw = sweep("tombstones/next", |n| {
            per_element(
                || TombstoneMergeIterator::new(tombstone_streams(n)),
                n / 8 * 7,
                true,
            )
        });
        let (cat, reason) = classify_feature(&sw, Some(base_p50), None);

        let mut p99 = BTreeMap::new();
        p99.insert(
            "tombstones_next".to_string(),
            per_element(
                || TombstoneMergeIterator::new(tombstone_streams(CANON)),
                CANON / 8 * 7,
                false,
            ),
        );
        manifest.set_feature("tombstones", cat, &p99, &reason);
    }

    // ---------- dedup: collapse equal keys, latest source wins ----------
    #[cfg(feature = "dedup")]
    {
        // Halved key space, so every key is carried by two sources and every
        // next pops twice: the collapse path runs on every element yielded.
        let sw = sweep("dedup/next", |n| {
            per_element(|| DedupMergeIterator::new(dedup_streams(n)), n / 2, true)
        });
        let (cat, reason) = classify_feature(&sw, Some(base_p50), None);

        let mut p99 = BTreeMap::new();
        p99.insert(
            "dedup_next".to_string(),
            per_element(
                || DedupMergeIterator::new(dedup_streams(CANON)),
                CANON / 2,
                false,
            ),
        );
        manifest.set_feature("dedup", cat, &p99, &reason);
    }

    // ---------- priority: explicit per-source precedence on key tie ----------
    #[cfg(feature = "priority")]
    {
        // Same collide-on-halved-keys shape as dedup, plus a priority field in
        // the heap comparison, so the two figures are directly comparable.
        let sw = sweep("priority/next", |n| {
            per_element(
                || PriorityMergeIterator::new(priority_sources(n)),
                n / 2,
                true,
            )
        });
        let (cat, reason) = classify_feature(&sw, Some(base_p50), None);

        let mut p99 = BTreeMap::new();
        p99.insert(
            "priority_next".to_string(),
            per_element(
                || PriorityMergeIterator::new(priority_sources(CANON)),
                CANON / 2,
                false,
            ),
        );
        manifest.set_feature("priority", cat, &p99, &reason);
    }

    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(&path, manifest.to_json())?;
    io::stdout().write_all(manifest.to_json().as_bytes())?;
    Ok(())
}

#[cfg(feature = "tombstones")]
fn tombstone_streams(n: usize) -> Vec<std::vec::IntoIter<TombstoneEntry<u64, u64>>> {
    let per = n / STREAMS;
    (0..STREAMS)
        .map(|s| {
            (0..per)
                .map(move |i| {
                    let key = (s + i * STREAMS) as u64;
                    if key % 8 == 0 {
                        TombstoneEntry::tombstone(key)
                    } else {
                        TombstoneEntry::live(key, key)
                    }
                })
                .collect::<Vec<_>>()
                .into_iter()
        })
        .collect()
}

#[cfg(feature = "dedup")]
fn dedup_streams(n: usize) -> Vec<std::vec::IntoIter<DedupEntry<u64, u64>>> {
    let per = n / STREAMS;
    (0..STREAMS)
        .map(|s| {
            (0..per)
                .map(move |i| {
                    let key = ((s + i * STREAMS) as u64) / 2;
                    DedupEntry::new(key, key)
                })
                .collect::<Vec<_>>()
                .into_iter()
        })
        .collect()
}

#[cfg(feature = "priority")]
fn priority_sources(n: usize) -> Vec<PrioritySource<std::vec::IntoIter<PriorityEntry<u64, u64>>>> {
    let per = n / STREAMS;
    (0..STREAMS)
        .map(|s| {
            let stream = (0..per)
                .map(move |i| {
                    let key = ((s + i * STREAMS) as u64) / 2;
                    PriorityEntry::new(key, key)
                })
                .collect::<Vec<_>>()
                .into_iter();
            PrioritySource::new((STREAMS - s) as i32, stream)
        })
        .collect()
}
