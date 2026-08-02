//! Feature classification bench. Each feature's representative op is swept
//! across three queue SIZES, `classify_feature` DECIDES the category from the
//! shape of that sweep, and the decision plus a measured `p99ByStage` is
//! merge-written into `.subms/features/rust.json`.
//!
//! Resident elements is the sweep axis: for the linked variants it is the number
//! of nodes the queue holds, for the ring variants the ring's capacity with half
//! of it occupied. 4096 / 32768 / 262144 elements is a 64x span that walks a
//! linked queue's live set from 160 KiB to 10 MiB and a ring from 64 KiB to
//! 4 MiB, so it crosses out of L1 and out of L2. Enqueue and dequeue are O(1) by
//! construction, so a FLAT curve is the expected answer and a rising one would
//! be the finding; what the sweep guards against is a feature whose bookkeeping
//! walks the queue.
//!
//! Two measurement units, deliberately:
//!
//! - The SWEEP times a sample of `ITEMS_PER_SAMPLE` round trips, not one op. An
//!   enqueue costs a few ns and the platform clock this bench was developed on
//!   ticks at 100 ns, so a single-op p50 reads exactly 100 or 200 ns and the
//!   category is decided by which side of a tick boundary the op lands on. That
//!   is not a hypothetical: measured one op at a time, `metrics` flipped between
//!   hot-path and auxiliary on consecutive runs of unchanged code. Folding a
//!   thousand round trips into one sample puts the sample tens of microseconds
//!   above the tick, and the same run then separates the ring variants from the
//!   allocating base by a clear margin.
//! - `p99ByStage` times ONE op, the way every other recipe's manifest does, so
//!   the numbers stay comparable across the cookbook. Those figures are only
//!   published from a fleet capture, where the clock has ~1 ns resolution.
//!
//! A sample covers the same ITEM count in every feature, `batch` included, whose
//! calls are `BATCH` items wide. Comparing a 256-item drain against a 1-item pop
//! would compare batch sizes, not features - and cost per item moved is exactly
//! the comparison the batch feature exists to win.
//!
//! Every queue is pre-filled and every measured unit is a round trip, so
//! occupancy is a fixed fraction at every sweep point and the ring variants
//! never take their full or empty branch (`bounded`'s reject path is measured
//! separately, as its own stage).
//!
//! The multi-producer variants are measured SINGLE-THREADED. That isolates the
//! CAS and the counter indirection from the contention they exist to survive; a
//! contended number here would say more about the thread count and the box than
//! about the feature, and what contention buys is a throughput property that no
//! per-op p99 states.
//!
//! This replaces the previous shape, which ran every variant at ONE size and
//! ASSERTED hot-path via `SubMsStageKind::HotPath`. An asserted category is an
//! opinion the bench cannot contradict; a sweep measures it, and can disagree.
//!
//! These p99 figures describe THIS machine. They are published only when the
//! manifest is stamped `p99_source: fleet`; a local run leaves the category,
//! which is machine independent, and no published number.
//!
//! Run:
//!   cargo run --release --example perf_features \
//!       --features "harness mpmc bounded batch metrics affinity"

use std::collections::BTreeMap;
use std::hint::black_box;
use std::io::{self, Write};
use std::path::PathBuf;

use subms::{
    SubMsFeatureManifest, SubMsP99Source, SubMsPerfHarness, SubMsStage, classify_feature, summarize,
};
use subms_mpsc_queue::MpscQueue;

/// Resident elements (ring capacity for the bounded variants). A 64x span.
const SIZES: [usize; 3] = [4_096, 32_768, 262_144];
const CANON: usize = SIZES[SIZES.len() - 1];
/// Items moved inside ONE timed sample. Fixed across every feature so the
/// base-delta test compares cost per item moved.
const ITEMS_PER_SAMPLE: usize = 1_024;
/// Timed samples per sweep point.
const SAMPLES: usize = 256;
/// Interleaved measurements per sweep point; the lowest is what gets classified.
const SWEEP_REPEATS: usize = 5;
/// Items per batch call. FIXED across the sweep - varying it would sweep the
/// batch size rather than the queue.
const BATCH: usize = 256;
/// Single-op reps behind each `p99ByStage` figure.
const OPS: usize = 50_000;
/// Warmup is TIME-BOXED, not a fixed rep count. Rust has no JIT, but every one
/// of these queues either allocates per push or streams a ring array, and both
/// have a first-touch ramp: measured cold, the first sweep point carries the
/// page faults for all three and the curve FALLS with size - as wrong as a fake
/// rise, and harder to spot.
const WARM_NANOS: u64 = 300_000_000;
const WARM_MAX_SAMPLES: usize = 5_000;
/// One-off burn before the first measurement, so the process-level ramp lands
/// somewhere other than the first sweep point.
const BURN_NANOS: u64 = 1_000_000_000;

fn stat(h: &SubMsPerfHarness, median: bool) -> u64 {
    summarize(h)
        .stages
        .iter()
        .find(|s| s.name == "op")
        .map_or(0, |s| if median { s.p50_ns } else { s.p99_ns })
}

/// p50 ns of one timed sample covering `reps` calls of `op`.
fn batched(reps: usize, mut op: impl FnMut(usize)) -> u64 {
    let mut i = 0usize;
    let start = std::time::Instant::now();
    for _ in 0..WARM_MAX_SAMPLES {
        for _ in 0..reps {
            op(i);
            i += 1;
        }
        if start.elapsed().as_nanos() as u64 >= WARM_NANOS {
            break;
        }
    }
    let mut h = SubMsPerfHarness::new("mpsc-queue-feature", "rust");
    let st = h.stage("op", SAMPLES);
    for _ in 0..SAMPLES {
        st.time(|| {
            for _ in 0..reps {
                op(i);
                i += 1;
            }
        });
    }
    stat(&h, true)
}

/// p99 ns of a single timed op. `body` is handed the stage so it can put the
/// restoring half of the round trip OUTSIDE the timed region: without that, a
/// 50k-op enqueue pass drifts the queue off its steady-state depth and a bounded
/// ring ends up measuring its full branch instead of the fast path.
fn single(mut body: impl FnMut(&mut SubMsStage, usize)) -> u64 {
    let mut warm = SubMsPerfHarness::new("mpsc-queue-feature", "rust");
    {
        let st = warm.stage("op", OPS);
        for i in 0..OPS {
            body(&mut *st, i);
        }
    }
    let mut h = SubMsPerfHarness::new("mpsc-queue-feature", "rust");
    {
        let st = h.stage("op", OPS);
        for i in 0..OPS {
            body(&mut *st, i);
        }
    }
    stat(&h, false)
}

/// Sweeps and PRINTS the curve, both the figures it returns and the raw repeats
/// behind them. A ratio-compressed or non-monotonic curve classifies flat, and
/// the rows are the only place that shows up.
///
/// Every size is measured `SWEEP_REPEATS` times, INTERLEAVED (all three sizes,
/// then all three again), and the LOWEST is taken. Two reasons, both visible in
/// the repeats this prints:
///
/// - Measuring one size to completion before moving on aliases slow drift onto
///   the size axis. Unpinned, this box alternates between two clock states 1.31x
///   apart and holds each for longer than a measurement takes, which is three
///   times the delta being classified and enough on its own to flip a feature.
///   Interleaving spreads that across all three points instead of concentrating
///   it in one.
/// - The lowest run is the one least disturbed by the box, and disturbance here
///   is one-sided: a collection, a migration or a frequency drop only ever adds
///   time. An average carries the interference into the comparison; the minimum
///   compares the queues. The p99 figures in `p99ByStage` still come from an
///   ordinary timed pass, interference included - it is the CATEGORY decision,
///   not the published latency, that wants the undisturbed number.
fn sweep(label: &str, mut at: impl FnMut(usize) -> u64) -> Vec<(usize, u64)> {
    let mut runs = vec![Vec::with_capacity(SWEEP_REPEATS); SIZES.len()];
    for _ in 0..SWEEP_REPEATS {
        for (k, &n) in SIZES.iter().enumerate() {
            runs[k].push(at(n));
        }
    }
    let rows: Vec<(usize, u64)> = SIZES
        .iter()
        .enumerate()
        .map(|(k, &n)| (n, runs[k].iter().copied().min().unwrap_or(0)))
        .collect();
    eprintln!("sweep {label}: {rows:?} repeats {runs:?}");
    rows
}

/// The base queue holding `n` nodes.
fn filled(n: usize) -> MpscQueue<u64> {
    let q = MpscQueue::new();
    for i in 0..n {
        q.push(i as u64);
    }
    q
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

    // Optional, off by default: pin this thread to one core for the whole run.
    // On a heterogeneous laptop the scheduler moves the bench between core
    // clusters and every measurement lands in one of two clock states 1.31x
    // apart - a spread three times wider than the deltas being classified, and
    // large enough on its own to flip a feature between auxiliary and hot-path.
    // Pinned, the same sweep repeats to within 1%. Left OFF by default because a
    // fleet box isolates cores outside the process, and pinning from in here
    // would override that placement with a core the orchestrator did not choose.
    #[cfg(feature = "affinity")]
    if let Some(core) = std::env::var("SUBMS_PIN").ok().and_then(|v| v.parse().ok()) {
        let _ = subms_mpsc_queue::set_affinity(&[core]);
    }

    // Burn before the first measurement, not just before each one. Every
    // `batched` call warms itself, but the FIRST measurement in the process pays
    // a ramp the per-measurement warm sits inside rather than absorbs, and the
    // sweep runs smallest-first: without this the base curve read 71800 / 45300 /
    // 46500 ns, a 1.6x fall with size that is the process settling, not the
    // queue.
    {
        let mut q = filled(CANON);
        let start = std::time::Instant::now();
        while (start.elapsed().as_nanos() as u64) < BURN_NANOS {
            for i in 0..ITEMS_PER_SAMPLE {
                q.push(i as u64);
                black_box(q.try_pop());
            }
        }
    }

    // The baseline: the base queue's push + try_pop round trip. Swept as well as
    // sampled, because whether queue depth moves the BASE op is the context
    // every feature curve is read against.
    let base_sweep = sweep("base/push+pop", |n| {
        let mut q = filled(n);
        batched(ITEMS_PER_SAMPLE, |i| {
            q.push(i as u64);
            black_box(q.try_pop());
        })
    });
    let base_p50 = base_sweep
        .iter()
        .find(|(n, _)| *n == CANON)
        .map_or(0, |(_, v)| *v);
    eprintln!("base push+pop p50 per {ITEMS_PER_SAMPLE}-item sample: {base_p50}ns");

    // ---------- bounded: fixed-capacity ring, backpressure on enqueue ----------
    #[cfg(feature = "bounded")]
    {
        use subms_mpsc_queue::BoundedMpscQueue;
        // Half full at every sweep point. Filled to a FIXED element count
        // instead, the big rings would sit 98% empty and the enqueue would be
        // measuring the fill fraction rather than the footprint.
        fn ring(n: usize) -> BoundedMpscQueue<u64> {
            let q = BoundedMpscQueue::new(n);
            for i in 0..n / 2 {
                let _ = q.try_enqueue(i as u64);
            }
            q
        }
        let sw = sweep("bounded/enqueue+dequeue", |n| {
            let mut q = ring(n);
            batched(ITEMS_PER_SAMPLE, |i| {
                let _ = q.try_enqueue(i as u64);
                black_box(q.try_dequeue());
            })
        });
        let (cat, reason) = classify_feature(&sw, Some(base_p50), None);

        let mut q = ring(CANON);
        let mut p99 = BTreeMap::new();
        p99.insert(
            "enqueue".to_string(),
            single(|st, i| {
                st.time(|| {
                    let _ = q.try_enqueue(i as u64);
                });
                black_box(q.try_dequeue());
            }),
        );
        p99.insert(
            "dequeue".to_string(),
            single(|st, i| {
                let _ = q.try_enqueue(i as u64);
                st.time(|| black_box(q.try_dequeue()));
            }),
        );
        // The reject path, which is the reason the feature exists. The ring is
        // filled to capacity once, OUTSIDE the timed region; every timed call
        // then takes the full branch and hands the value back to the caller.
        let full: BoundedMpscQueue<u64> = BoundedMpscQueue::new(CANON);
        while full.try_enqueue(0).is_ok() {}
        p99.insert(
            "enqueue_full".to_string(),
            single(|st, i| {
                st.time(|| {
                    let _ = full.try_enqueue(i as u64);
                });
            }),
        );
        manifest.set_feature("bounded", cat, &p99, &reason);
    }

    // ---------- mpmc: bounded ring, sequence CAS on both ends ----------
    #[cfg(feature = "mpmc")]
    {
        use subms_mpsc_queue::MpmcQueue;
        fn ring(n: usize) -> MpmcQueue<u64> {
            let q = MpmcQueue::new(n);
            for i in 0..n / 2 {
                let _ = q.try_enqueue(i as u64);
            }
            q
        }
        // Uncontended, so every CAS succeeds first try. That is the figure the
        // category is about: what the multi-consumer claim costs a queue that is
        // NOT contended, which is the state a well-sized pipeline runs in.
        let sw = sweep("mpmc/enqueue+dequeue", |n| {
            let q = ring(n);
            batched(ITEMS_PER_SAMPLE, |i| {
                let _ = q.try_enqueue(i as u64);
                black_box(q.try_dequeue());
            })
        });
        let (cat, reason) = classify_feature(&sw, Some(base_p50), None);

        let q = ring(CANON);
        let mut p99 = BTreeMap::new();
        p99.insert(
            "enqueue".to_string(),
            single(|st, i| {
                st.time(|| {
                    let _ = q.try_enqueue(i as u64);
                });
                black_box(q.try_dequeue());
            }),
        );
        p99.insert(
            "dequeue".to_string(),
            single(|st, i| {
                let _ = q.try_enqueue(i as u64);
                st.time(|| black_box(q.try_dequeue()));
            }),
        );
        manifest.set_feature("mpmc", cat, &p99, &reason);
    }

    // ---------- batch: drain up to BATCH items behind one acquire fence ----------
    #[cfg(feature = "batch")]
    {
        use subms_mpsc_queue::BatchMpscQueue;
        fn filled_batch(n: usize) -> BatchMpscQueue<u64> {
            let q = BatchMpscQueue::new();
            for i in 0..n {
                q.push(i as u64);
            }
            q
        }
        // A sample moves ITEMS_PER_SAMPLE items either way; only the call width
        // differs. That is why the reps count is divided rather than the batch
        // grown - growing it would sweep the batch size, and the number would
        // stop being comparable to the base round trip.
        let sw = sweep("batch/push+dequeue_batch", |n| {
            let mut q = filled_batch(n);
            let mut buf: Vec<Option<u64>> = (0..BATCH).map(|_| None).collect();
            batched(ITEMS_PER_SAMPLE / BATCH, |i| {
                for j in 0..BATCH {
                    q.push((i + j) as u64);
                }
                black_box(q.try_dequeue_batch(&mut buf));
            })
        });
        let (cat, reason) = classify_feature(&sw, Some(base_p50), None);

        let mut q = filled_batch(CANON);
        let mut buf: Vec<Option<u64>> = (0..BATCH).map(|_| None).collect();
        let mut p99 = BTreeMap::new();
        // The refill is outside the timed region: timing it would put a BATCH of
        // pushes inside the drain's number and the stage would stop being a
        // drain figure at all.
        p99.insert(
            "dequeue_batch".to_string(),
            single(|st, i| {
                st.time(|| black_box(q.try_dequeue_batch(&mut buf)));
                for j in 0..BATCH {
                    q.push((i + j) as u64);
                }
            }),
        );
        p99.insert(
            "enqueue".to_string(),
            single(|st, i| {
                st.time(|| q.push(i as u64));
                let _ = q.try_dequeue_batch(&mut buf[..1]);
            }),
        );
        manifest.set_feature("batch", cat, &p99, &reason);
    }

    // ---------- metrics: relaxed atomic counters around each op ----------
    #[cfg(feature = "metrics")]
    {
        use subms_mpsc_queue::MetricsMpscQueue;
        fn filled_metrics(n: usize) -> MetricsMpscQueue<u64> {
            let q = MetricsMpscQueue::new();
            for i in 0..n {
                q.push(i as u64);
            }
            q
        }
        let sw = sweep("metrics/push+pop", |n| {
            let mut q = filled_metrics(n);
            batched(ITEMS_PER_SAMPLE, |i| {
                q.push(i as u64);
                black_box(q.try_pop());
            })
        });
        let (cat, reason) = classify_feature(&sw, Some(base_p50), None);

        let mut q = filled_metrics(CANON);
        let mut p99 = BTreeMap::new();
        p99.insert(
            "enqueue".to_string(),
            single(|st, i| {
                st.time(|| q.push(i as u64));
                black_box(q.try_pop());
            }),
        );
        p99.insert(
            "dequeue".to_string(),
            single(|st, i| {
                q.push(i as u64);
                st.time(|| black_box(q.try_pop()));
            }),
        );
        p99.insert(
            "snapshot".to_string(),
            single(|st, _| {
                st.time(|| black_box(q.snapshot()));
            }),
        );
        manifest.set_feature("metrics", cat, &p99, &reason);
    }

    // ---------- affinity: pin the calling thread, once, at startup ----------
    // Runs LAST because measuring it pins THIS process to core 0, and every
    // number taken afterwards would be a number taken on one core.
    #[cfg(feature = "affinity")]
    {
        use subms_mpsc_queue::set_affinity;
        // Swept over the same axis to show what it is: a call that touches no
        // queue state and cannot move with queue size. PINNED auxiliary rather
        // than left to the base-delta test, which would see a syscall costing
        // more than an enqueue and call it hot-path. It is not on the hot path at
        // any price - `set_affinity` is called once per thread at startup and
        // appears in neither `push` nor `try_pop`. The two ports are not even
        // measuring the same thing: Rust issues a real `SetThreadAffinityMask` /
        // `sched_setaffinity`, while the Java sibling validates its argument and
        // returns UNSUPPORTED because the stock JDK has no pinning API. The
        // per-call figure, not the sample, is the interpretable one and it is in
        // `p99ByStage`.
        let sw = sweep("affinity/set_affinity", |_| {
            batched(ITEMS_PER_SAMPLE, |_| {
                let _ = set_affinity(&[0]);
            })
        });
        let (cat, reason) = classify_feature(
            &sw,
            Some(base_p50),
            Some(subms::SubMsFeatureCategory::Auxiliary),
        );

        let mut p99 = BTreeMap::new();
        p99.insert(
            "set_affinity".to_string(),
            single(|st, _| {
                st.time(|| {
                    let _ = set_affinity(&[0]);
                });
            }),
        );
        manifest.set_feature("affinity", cat, &p99, &reason);

        let cores: Vec<usize> = (0..std::thread::available_parallelism()
            .map_or(1, std::num::NonZeroUsize::get))
            .collect();
        let _ = set_affinity(&cores);
    }

    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(&path, manifest.to_json())?;
    io::stdout().write_all(manifest.to_json().as_bytes())?;
    Ok(())
}
