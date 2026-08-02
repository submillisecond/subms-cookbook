//! Feature classification bench. Each feature's representative op is swept
//! across three RING CAPACITIES, `classify_feature` DECIDES the category from
//! the shape of that sweep, and the decision plus a measured `p99ByStage` is
//! merge-written into `.subms/features/rust.json`.
//!
//! Capacity is the sweep axis because it is the only thing that sets a ring's
//! size. 1024 / 16384 / 262144 slots of `u64` is 8 KiB / 128 KiB / 2 MiB, so the
//! span crosses out of L1 and out of L2. Push and pop are O(1) by construction,
//! so a flat curve is the EXPECTED answer here and a rising one would be the
//! finding; what the sweep guards against is a feature whose bookkeeping walks
//! the ring.
//!
//! Two measurement units, deliberately:
//!
//! - The SWEEP times a sample of `ITEMS_PER_SAMPLE` round trips, not one op. A
//!   push costs a few ns and the platform clock this bench is developed on ticks
//!   at 100 ns, so every single-op p50 reads exactly 100 ns and every curve is
//!   flat by quantisation rather than by physics. Folding a thousand round trips
//!   into one sample puts the sample microseconds above the tick and lets the
//!   sweep see a slope at all.
//! - `p99ByStage` times ONE op, the way every other recipe's manifest does, so
//!   the numbers stay comparable across the cookbook. Those figures are only
//!   published from a fleet capture, where the clock resolves single ns.
//!
//! A sample covers the same ITEM count in every feature, including `bulk`, whose
//! calls are `BULK_BATCH` items wide. Comparing a 32-item bulk call against a
//! 1-item push would compare batch sizes, not features.
//!
//! Every ring is pre-filled to half capacity and every measured op is a round
//! trip, so occupancy is a fixed fraction at every sweep point and neither side
//! ever takes its full / empty branch.
//!
//! Every op returns a `u64` that the timed loop accumulates and blackboxes once
//! per sample. Blackboxing each pop's `Option` instead forces a 16-byte value
//! through memory on every iteration, and it lands only on the features whose
//! pop returns an `Option` - which made the BusySpin wrapper measure ~20% FASTER
//! than the base ring it wraps.
//!
//! Each sweep point is measured `ROUNDS` times, size-interleaved, and the
//! MINIMUM is kept. A single pass put the LARGEST ring fastest on three of five
//! features, and the raw rounds show why: every measurement on this box lands on
//! one of exactly two levels a constant 1.31x apart, the same ratio for the base
//! ring and for every feature, which is a clock or core-class landing rather than
//! anything about the ring. A median mixes the two levels, so the sweep column
//! then moves by which level each point drew - `mpsc-fan-in` read as a 1.3x rise
//! with size that way, and reads flat once the levels are separated. The minimum
//! draws every point from the same level.
//!
//! `wait-strategies` is measured ONLY on the non-full, non-empty fast path. A
//! strategy's `wait()` is a scheduler measurement - `ParkStrategy` sleeps until
//! the other end unparks it, which is milliseconds - and publishing that as the
//! feature's per-op cost would be a category error. What IS measured is what the
//! wrapper costs when the ring is ready, which is where a caller spends its time.
//!
//! The multi-producer features (`mpsc-fan-in`, `mpmc-disruptor`) are measured
//! SINGLE-THREADED at a fixed producer / consumer count. That isolates the
//! indirection from the contention it exists to relieve; a contended number here
//! would say more about the thread count than about the feature.
//!
//! These p99 figures describe THIS machine. They are published only when the
//! manifest is stamped `p99_source: fleet`; a local run leaves the category,
//! which is machine independent, and no published number.
//!
//! Run:
//!   cargo run --release --example perf_features \
//!       --features "harness bulk wait-strategies mpsc-fan-in mpmc-disruptor metrics"

use std::collections::BTreeMap;
use std::hint::black_box;
use std::io::{self, Write};
use std::path::PathBuf;

use subms::{SubMsFeatureManifest, SubMsP99Source, SubMsPerfHarness, classify_feature, summarize};
use subms_spsc_ring_buffer::{Consumer, Producer, SpscRingBuffer};

/// Slot counts the sweep walks. A 256x span, already powers of two.
const SIZES: [usize; 3] = [1_024, 16_384, 262_144];
const CANON: usize = SIZES[SIZES.len() - 1];
/// Items moved inside ONE timed sample. Fixed across every feature so the
/// base-delta test compares cost per item moved.
const ITEMS_PER_SAMPLE: usize = 1_024;
/// Timed samples per sweep point.
const SAMPLES: usize = 256;
/// Size-interleaved repeats of the whole sweep; the per-size minimum is kept.
const ROUNDS: usize = 7;
/// Items per bulk call. FIXED across the sweep - varying it would sweep the
/// batch size rather than the ring.
const BULK_BATCH: usize = 32;
/// Single-op reps behind each `p99ByStage` figure.
const OPS: usize = 50_000;
/// Warmup is TIME-BOXED, not a fixed rep count. A cold first sweep point lands
/// its page-fault and branch-predictor ramp on whichever size runs first, which
/// reads as a curve that FALLS with size - as wrong as a fake rise, and harder
/// to spot.
const WARM_NANOS: u64 = 300_000_000;
const WARM_MAX_SAMPLES: usize = 1_000;

fn stat(h: &SubMsPerfHarness, median: bool) -> u64 {
    summarize(h)
        .stages
        .iter()
        .find(|s| s.name == "op")
        .map_or(0, |s| if median { s.p50_ns } else { s.p99_ns })
}

/// p50 ns of one timed sample covering `reps` calls of `op`.
fn batched(reps: usize, mut op: impl FnMut(usize) -> u64) -> u64 {
    let mut i = 0usize;
    let start = std::time::Instant::now();
    for _ in 0..WARM_MAX_SAMPLES {
        let mut acc = 0u64;
        for _ in 0..reps {
            acc = acc.wrapping_add(op(i));
            i += 1;
        }
        black_box(acc);
        if start.elapsed().as_nanos() as u64 >= WARM_NANOS {
            break;
        }
    }
    let mut h = SubMsPerfHarness::new("spsc-feature", "rust");
    let st = h.stage("op", SAMPLES);
    for _ in 0..SAMPLES {
        st.time(|| {
            let mut acc = 0u64;
            for _ in 0..reps {
                acc = acc.wrapping_add(op(i));
                i += 1;
            }
            black_box(acc);
        });
    }
    stat(&h, true)
}

/// p99 ns of a single `timed` call. `untimed` runs outside the timed region and
/// restores the ring's depth, so a 50k-op enqueue pass cannot fill the ring and
/// start measuring the full branch instead of the fast path.
fn single(mut timed: impl FnMut(usize) -> u64, mut untimed: impl FnMut(usize) -> u64) -> u64 {
    for i in 0..OPS {
        black_box(timed(i));
        black_box(untimed(i));
    }
    let mut h = SubMsPerfHarness::new("spsc-feature", "rust");
    let st = h.stage("op", OPS);
    for i in 0..OPS {
        st.time(|| black_box(timed(i)));
        black_box(untimed(i));
    }
    stat(&h, false)
}

/// Sweeps and PRINTS the curve, raw rounds included in ROUND ORDER. A
/// ratio-compressed or non-monotonic curve classifies flat, and the rows are the
/// only place that shows up - and the raw rounds are the only place a bimodal
/// clock shows up.
fn sweep(label: &str, mut at: impl FnMut(usize) -> u64) -> Vec<(usize, u64)> {
    let mut runs: Vec<Vec<u64>> = vec![Vec::new(); SIZES.len()];
    for _ in 0..ROUNDS {
        for (k, &n) in SIZES.iter().enumerate() {
            runs[k].push(at(n));
        }
    }
    let rows: Vec<(usize, u64)> = SIZES
        .iter()
        .enumerate()
        .map(|(k, &n)| (n, runs[k].iter().copied().min().unwrap_or(0)))
        .collect();
    eprintln!("sweep {label}: {rows:?} raw {runs:?}");
    rows
}

/// Lowest of `ROUNDS` repeats, for a figure that is printed rather than swept.
fn best(mut f: impl FnMut() -> u64) -> u64 {
    (0..ROUNDS).map(|_| f()).min().unwrap_or(0)
}

/// A ring pre-filled to half capacity. Occupancy is a constant fraction at every
/// sweep point, so a slope has one cause; and both sides stay off their full /
/// empty branch for the whole measurement.
fn pair(cap: usize) -> (Producer<u64>, Consumer<u64>) {
    let (mut tx, rx) = SpscRingBuffer::with_capacity::<u64>(cap);
    for i in 0..cap / 2 {
        let _ = tx.try_push(i as u64);
    }
    (tx, rx)
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

    // The baseline: the base wait-free push + pop round trip. Swept as well as
    // sampled, because whether ring capacity moves the BASE op is the context
    // every feature curve is read against.
    let base_sweep = sweep("base/push+pop", |cap| {
        let (mut tx, mut rx) = pair(cap);
        batched(ITEMS_PER_SAMPLE, |i| {
            let _ = tx.try_push(i as u64);
            rx.try_pop().unwrap_or(0)
        })
    });
    let base_p50 = base_sweep
        .iter()
        .find(|(n, _)| *n == CANON)
        .map_or(0, |(_, v)| *v);
    eprintln!("base push+pop p50 per {ITEMS_PER_SAMPLE}-item sample: {base_p50}ns");

    // ---------- bulk: one fence per BULK_BATCH items ----------
    #[cfg(feature = "bulk")]
    {
        // A sample moves ITEMS_PER_SAMPLE items either way; only the call width
        // differs. That is the comparison the feature exists to win, and it is
        // why the reps count is divided rather than the batch grown.
        let sw = sweep("bulk/enqueue+dequeue", |cap| {
            let (mut tx, mut rx) = pair(cap);
            let batch = [0u64; BULK_BATCH];
            let mut out = [0u64; BULK_BATCH];
            batched(ITEMS_PER_SAMPLE / BULK_BATCH, |_| {
                let n = tx.try_enqueue_bulk(&batch);
                (n + rx.try_dequeue_bulk(&mut out)) as u64
            })
        });
        let (cat, reason) = classify_feature(&sw, Some(base_p50), None);

        let batch = [0u64; BULK_BATCH];
        let mut out = [0u64; BULK_BATCH];
        let (mut tx, mut rx) = pair(CANON);
        let enq = single(
            |_| tx.try_enqueue_bulk(&batch) as u64,
            |_| rx.try_dequeue_bulk(&mut out) as u64,
        );
        let (mut tx, mut rx) = pair(CANON);
        let deq = single(
            |_| rx.try_dequeue_bulk(&mut out) as u64,
            |_| tx.try_enqueue_bulk(&batch) as u64,
        );
        let mut p99 = BTreeMap::new();
        p99.insert("enqueue_bulk".to_string(), enq);
        p99.insert("dequeue_bulk".to_string(), deq);
        manifest.set_feature("bulk", cat, &p99, &reason);
    }

    // ---------- wait-strategies: blocking wrappers, fast path only ----------
    #[cfg(feature = "wait-strategies")]
    {
        use subms_spsc_ring_buffer::{
            BlockingSpscConsumer, BlockingSpscProducer, BusySpin, ParkStrategy,
        };
        // Swept on BusySpin, whose `signal()` is a no-op, so the curve is the
        // wrapper's own overhead over the base ring and nothing else. The ring
        // is never full or empty, so `wait()` is never called; the parked-wakeup
        // path is a scheduler latency, not a per-op cost, and is not measured.
        let sw = sweep("wait-strategies/push+pop", |cap| {
            let (tx, rx) = pair(cap);
            let mut p = BlockingSpscProducer::new(tx, BusySpin);
            let mut c = BlockingSpscConsumer::new(rx, BusySpin);
            batched(ITEMS_PER_SAMPLE, |i| {
                p.push(i as u64);
                c.pop()
            })
        });
        let (cat, reason) = classify_feature(&sw, Some(base_p50), None);

        // ParkStrategy on the SAME ready ring, printed rather than classified.
        // Its `signal()` fires on every successful op even when nothing is
        // parked, and it takes a lock to do it, so the strategy choice shows up
        // here on the fast path - not in `wait()`, which this never enters.
        let park_batched = best(|| {
            let (ps, cs) = ParkStrategy::pair();
            let (tx, rx) = pair(CANON);
            let mut p = BlockingSpscProducer::new(tx, ps);
            let mut c = BlockingSpscConsumer::new(rx, cs);
            batched(ITEMS_PER_SAMPLE, |i| {
                p.push(i as u64);
                c.pop()
            })
        });
        eprintln!(
            "wait-strategies park fast path at {CANON}: {park_batched}ns per \
             {ITEMS_PER_SAMPLE}-item sample (spin {}ns) - signal() takes a lock per op",
            sw.iter().find(|(n, _)| *n == CANON).map_or(0, |(_, v)| *v)
        );

        let (tx, rx) = pair(CANON);
        let mut p = BlockingSpscProducer::new(tx, BusySpin);
        let mut c = BlockingSpscConsumer::new(rx, BusySpin);
        let push_spin = single(
            |i| {
                p.push(i as u64);
                0
            },
            |_| c.pop(),
        );
        let (tx, rx) = pair(CANON);
        let mut p = BlockingSpscProducer::new(tx, BusySpin);
        let mut c = BlockingSpscConsumer::new(rx, BusySpin);
        let pop_spin = single(
            |_| c.pop(),
            |i| {
                p.push(i as u64);
                0
            },
        );

        let (ps, cs) = ParkStrategy::pair();
        let (tx, rx) = pair(CANON);
        let mut p = BlockingSpscProducer::new(tx, ps);
        let mut c = BlockingSpscConsumer::new(rx, cs);
        let push_park = single(
            |i| {
                p.push(i as u64);
                0
            },
            |_| c.pop(),
        );
        let (ps, cs) = ParkStrategy::pair();
        let (tx, rx) = pair(CANON);
        let mut p = BlockingSpscProducer::new(tx, ps);
        let mut c = BlockingSpscConsumer::new(rx, cs);
        let pop_park = single(
            |_| c.pop(),
            |i| {
                p.push(i as u64);
                0
            },
        );

        let mut p99 = BTreeMap::new();
        p99.insert("push_spin".to_string(), push_spin);
        p99.insert("pop_spin".to_string(), pop_spin);
        p99.insert("push_park".to_string(), push_park);
        p99.insert("pop_park".to_string(), pop_park);
        manifest.set_feature("wait-strategies", cat, &p99, &reason);
    }

    // ---------- mpsc-fan-in: N rings, one round-robin consumer ----------
    #[cfg(feature = "mpsc-fan-in")]
    {
        use subms_spsc_ring_buffer::{MpscFanIn, MpscFanInConsumer, MpscFanInProducer};
        /// Producer count is held FIXED across the sweep. Moving it would sweep
        /// the fan-in width, which sets the consumer's probe loop, not the ring.
        const PRODUCERS: usize = 4;

        fn fanin(cap: usize) -> (Vec<MpscFanInProducer<u64>>, MpscFanInConsumer<u64>) {
            let (mut ps, c) = MpscFanIn::with_capacity::<u64>(PRODUCERS, cap);
            for p in ps.iter_mut() {
                for i in 0..cap / 2 {
                    let _ = p.try_push(i as u64);
                }
            }
            (ps, c)
        }

        // Pushes round-robin and the consumer cursor advances one ring per pop,
        // so the two stay in step and every ring holds a constant half load.
        // With every ring non-empty the consumer's probe hits on its first try,
        // which is the steady-state shape; a starved fan-in probes all N and that
        // is a different measurement.
        let sw = sweep("mpsc-fan-in/push+pop", |cap| {
            let (mut ps, mut c) = fanin(cap);
            batched(ITEMS_PER_SAMPLE, |i| {
                let _ = ps[i % PRODUCERS].try_push(i as u64);
                c.try_pop().unwrap_or(0)
            })
        });
        let (cat, reason) = classify_feature(&sw, Some(base_p50), None);

        let (mut ps, mut c) = fanin(CANON);
        let enq = single(
            |i| {
                let _ = ps[i % PRODUCERS].try_push(i as u64);
                0
            },
            |_| c.try_pop().unwrap_or(0),
        );
        let (mut ps, mut c) = fanin(CANON);
        let deq = single(
            |_| c.try_pop().unwrap_or(0),
            |i| {
                let _ = ps[i % PRODUCERS].try_push(i as u64);
                0
            },
        );
        let mut p99 = BTreeMap::new();
        p99.insert("fanin_enqueue".to_string(), enq);
        p99.insert("fanin_dequeue".to_string(), deq);
        manifest.set_feature("mpsc-fan-in", cat, &p99, &reason);
    }

    // ---------- mpmc-disruptor: CAS claim + sequence barrier ----------
    #[cfg(feature = "mpmc-disruptor")]
    {
        use subms_spsc_ring_buffer::{DisruptorConsumer, DisruptorProducer, MpmcDisruptor};
        /// One consumer, held fixed. `try_publish` scans every consumer cursor
        /// before claiming, so the consumer count - not the capacity - is what
        /// that loop is O(). Sweeping it would answer a different question.
        const CONSUMERS: usize = 1;

        fn disruptor(cap: usize) -> (DisruptorProducer<u64>, DisruptorConsumer<u64>) {
            let (p, mut cs) = MpmcDisruptor::with_consumers::<u64>(cap, CONSUMERS);
            let c = cs.remove(0);
            for i in 0..cap / 2 {
                let _ = p.try_publish(i as u64);
            }
            (p, c)
        }

        // Half a ring of published-but-unconsumed items keeps the producer clear
        // of the gating spin (it only fires within one capacity of the slowest
        // consumer) and the consumer clear of the unpublished early return.
        let sw = sweep("mpmc-disruptor/publish+consume", |cap| {
            let (p, mut c) = disruptor(cap);
            batched(ITEMS_PER_SAMPLE, |i| {
                let _ = p.try_publish(i as u64);
                c.try_consume().unwrap_or(0)
            })
        });
        let (cat, reason) = classify_feature(&sw, Some(base_p50), None);

        let (p, mut c) = disruptor(CANON);
        let published = single(
            |i| {
                let _ = p.try_publish(i as u64);
                0
            },
            |_| c.try_consume().unwrap_or(0),
        );
        let (p, mut c) = disruptor(CANON);
        let consumed = single(
            |_| c.try_consume().unwrap_or(0),
            |i| {
                let _ = p.try_publish(i as u64);
                0
            },
        );
        let mut p99 = BTreeMap::new();
        p99.insert("publish".to_string(), published);
        p99.insert("consume".to_string(), consumed);
        manifest.set_feature("mpmc-disruptor", cat, &p99, &reason);
    }

    // ---------- metrics: counters on the push / pop path ----------
    #[cfg(feature = "metrics")]
    {
        use subms_spsc_ring_buffer::InstrumentedSpsc;

        // The wrapper adds a `fetch_add` per op plus, on the producer side, a
        // high-water-mark update. `local_depth` only ever increments - a producer
        // cannot observe pops - so that update takes its CAS on every push rather
        // than settling once the mark stops moving. The counter cost on the push
        // path is two read-modify-writes, not one.
        let sw = sweep("metrics/push+pop", |cap| {
            let (tx, rx) = pair(cap);
            let (mut tx, mut rx, _m) = InstrumentedSpsc::wrap(tx, rx);
            batched(ITEMS_PER_SAMPLE, |i| {
                let _ = tx.try_push(i as u64);
                rx.try_pop().unwrap_or(0)
            })
        });
        let (cat, reason) = classify_feature(&sw, Some(base_p50), None);

        let (tx, rx) = pair(CANON);
        let (mut tx, mut rx, _m) = InstrumentedSpsc::wrap(tx, rx);
        let enq = single(
            |i| {
                let _ = tx.try_push(i as u64);
                0
            },
            |_| rx.try_pop().unwrap_or(0),
        );
        let (tx, rx) = pair(CANON);
        let (mut tx, mut rx, _m) = InstrumentedSpsc::wrap(tx, rx);
        let deq = single(
            |_| rx.try_pop().unwrap_or(0),
            |i| {
                let _ = tx.try_push(i as u64);
                0
            },
        );
        let mut p99 = BTreeMap::new();
        p99.insert("metrics_enqueue".to_string(), enq);
        p99.insert("metrics_dequeue".to_string(), deq);
        manifest.set_feature("metrics", cat, &p99, &reason);
    }

    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(&path, manifest.to_json())?;
    io::stdout().write_all(manifest.to_json().as_bytes())?;
    Ok(())
}
