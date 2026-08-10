//! Feature classification bench. Each feature's representative op is swept
//! across three FLEET SIZES, `classify_feature` DECIDES the category from the
//! shape of that sweep, and the decision plus a measured `p99ByStage` is
//! merge-written into `.subms/features/rust.json`.
//!
//! The sweep axis is the number of independent limited entities the workload
//! cycles over: buckets for `token-bucket` and `metrics`, children for
//! `hierarchical`, live keys for `distributed-backend`. A limiter has no
//! internal array to grow, so the only thing a deployment scales is the fleet
//! of tenants, and that is the axis every feature here shares. Four of the five
//! are O(1) per acquire and should read flat; `distributed-backend` sweeps the
//! whole counter map on every `incr`, so it should climb.
//!
//! Run:
//!   cargo run --release --example perf_features \
//!       --features "harness token-bucket hierarchical distributed-backend metrics keyed"

use std::collections::BTreeMap;
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use subms::{SubMsFeatureManifest, SubMsP99Source, SubMsPerfHarness, classify_feature, summarize};
#[cfg(feature = "keyed")]
use subms_rate_limiter::Acquire;
use subms_rate_limiter::{Clock, RateLimiter};

/// 1024 / 8192 / 65536 limiters, a 64x span.
const SIZES: [usize; 3] = [1024, 8192, 65536];
const CANON_N: usize = SIZES[SIZES.len() - 1];
/// Timed ops per measurement, fixed across the sweep so a slope has one cause.
/// Equal to the largest fleet, so every limiter is touched and the FILL scales
/// with the size instead of a fixed 20k of them carrying the whole workload.
const OPS: usize = CANON_N;

/// The distributed backend's per-call cost is the counter-map GC, so its span
/// is in live keys and it has to start high enough that the fixed per-call
/// work (a `String` alloc, a hash, a lock) does not compress the ratio.
const DIST_SIZES: [usize; 3] = [512, 2048, 8192];
const DIST_CANON: usize = DIST_SIZES[DIST_SIZES.len() - 1];
const DIST_OPS: usize = 4_096;
/// Timed calls hit a FIXED-size hot subset of the prefilled keys, so each hot
/// key takes the same number of bumps at every sweep point and the accept
/// ratio does not move with N. The cold keys still sit in the map and are
/// still swept by the GC - which is the cost being measured.
const DIST_HOT: usize = 256;
/// DIST_OPS / DIST_HOT = 16 bumps per hot key on top of the prefill's one, so
/// counts run 2..=17 and a limit of 9 splits them exactly in half.
const DIST_LIMIT: u64 = 9;
/// An hour. The window must not roll during a run: a roll expires every
/// prefilled key at once and the map collapses to the hot subset, which would
/// silently delete the size axis.
const DIST_WINDOW_NS: u64 = 3_600_000_000_000;

/// Synthetic ns added per clock read. See `SteppingClock`.
const STRIDE_NS: u64 = 1_000;

/// Bucket capacity, and the rates that put the accept ratio near half.
/// `TokenBucket::try_acquire` reads the clock once, so it accrues
/// `TB_RATE * STRIDE_NS / 1e9` = 0.5 tokens per call. `MeteredTokenBucket`
/// reads it three times (available, try_acquire, available), so its rate is a
/// third of that to land on the same ratio.
const TB_CAP: u64 = 4;
const TB_RATE: f64 = 500_000.0;
const MET_RATE: f64 = 166_667.0;
/// Untimed acquires each bucket takes during setup, enough to spend the
/// capacity it is built full with and settle into the alternating
/// accept/reject steady state. Without it the accept ratio is a function of
/// how many timed calls each bucket receives, which is `OPS / n` - so the
/// mix, not the size, is what the sweep varies: measured 55% grants at 1024
/// buckets, 88% at 8192 and 100% at 65536, where each bucket is touched once
/// and a full bucket cannot do anything but grant.
///
/// Odd-indexed buckets take one extra, which is what makes the ratio hold at
/// the top of the sweep. A settled bucket alternates, so a fleet settled in
/// LOCKSTEP still grants on every first touch - 100%, not 50%, at the size
/// where each bucket is touched exactly once. Half the fleet has to be
/// settled on the other phase for the mix to come from across the fleet.
const PRE_DRAIN: usize = 16;
/// The parent is deliberately not the throttle: it accrues 2 tokens per call
/// against the 1 it can spend, so it always grants and the child governs the
/// accept ratio. A parent that also rejected would change the mix of paths
/// taken as the fleet grew.
const HIER_PARENT_CAP: u64 = 64;
const HIER_PARENT_RATE: f64 = 2_000_000.0;

const BASE_RATE: f64 = 1_000_000.0;
const BASE_BURST: u64 = 4;

/// Warm is TIME-BOXED rather than a fixed rep count. A fixed count leaves the
/// cheap sweep points under-warmed and the expensive ones over-warmed, and an
/// allocating op (every `incr` here builds a `String` key) has an allocator
/// ramp that a handful of reps does not settle.
const WARM_NANOS: u64 = 300_000_000;
const WARM_MAX_REPS: usize = 200_000;

/// Ops per timed sample on the classification pass. The platform timer ticks
/// at 100 ns on this box, and a bucket acquire is a few tens of ns, so a
/// per-op sample can only ever read 0 or 100 - base and `token-bucket` both
/// landed on exactly 100 and the classifier called a mutex-guarded refill a
/// non-effect. A batch of 16 puts the sample an order of magnitude above the
/// tick. The published `p99ByStage` is still measured one op at a time: a
/// batch mean would hide the tail, which is the number the site prints.
const BATCH: usize = 16;

/// A clock that reads the platform clock and then throws the reading away,
/// returning a synthetic value that steps by a fixed `STRIDE_NS` per read.
///
/// Both halves are load-bearing. The read is kept because the production
/// `SystemClock` makes exactly that call, and a fixture that skipped it would
/// hand every feature a free saving the base limiter still pays, which reads
/// as "cheaper than base" - auxiliary - for a feature that is not. The value
/// is synthetic because real elapsed time between two touches of the SAME
/// bucket scales with how many buckets the loop cycles: at 65536 buckets a
/// bucket sees milliseconds between its own calls and refills to full every
/// time, so the sweep would be varying token occupancy rather than size.
///
/// A frozen clock is the other half of the same trap: `refill_locked`
/// early-returns on `elapsed == 0`, so a bucket driven by a stopped clock
/// never runs the refill arithmetic at all and the feature measures as a
/// compare-and-subtract.
struct SteppingClock {
    origin: Instant,
    steps: AtomicU64,
    sink: AtomicU64,
}

impl SteppingClock {
    fn new() -> Self {
        Self {
            origin: Instant::now(),
            steps: AtomicU64::new(0),
            sink: AtomicU64::new(0),
        }
    }
}

impl Clock for SteppingClock {
    fn now_ns(&self) -> u64 {
        self.sink
            .store(self.origin.elapsed().as_nanos() as u64, Ordering::Relaxed);
        self.steps.fetch_add(STRIDE_NS, Ordering::Relaxed) + STRIDE_NS
    }
}

struct Measured {
    p50: u64,
    p99: u64,
    accept: f64,
}

fn stat(h: &SubMsPerfHarness, name: &str, median: bool) -> u64 {
    summarize(h)
        .stages
        .iter()
        .find(|s| s.name == name)
        .map_or(0, |s| if median { s.p50_ns } else { s.p99_ns })
}

/// Builds the structure in `setup` OUTSIDE the timed region, warms on a
/// throwaway copy, then measures a FRESH one so the state at sample 0 is
/// identical at every sweep point - a warm pass on the measured instance would
/// leave its buckets drained and its counters bumped by however many reps the
/// time box happened to allow.
///
/// Two timed passes over that instance: one op per sample for the published
/// p99, then `batch` ops per sample for the p50 the classifier reads. Pass
/// `batch = 1` for an op already well clear of the timer tick; the second pass
/// is then skipped rather than run for nothing.
fn measure<T>(
    mut setup: impl FnMut() -> T,
    mut op: impl FnMut(&T, usize) -> bool,
    ops: usize,
    batch: usize,
) -> Measured {
    let warm = setup();
    let start = Instant::now();
    for rep in 0..WARM_MAX_REPS {
        if start.elapsed().as_nanos() as u64 >= WARM_NANOS {
            break;
        }
        op(&warm, rep % ops);
    }
    drop(warm);

    let target = setup();
    let mut h = SubMsPerfHarness::new("rate-limiter-feature", "rust");
    let mut granted = 0usize;
    {
        let st = h.stage("op", ops);
        for i in 0..ops {
            if st.time(|| op(&target, i)) {
                granted += 1;
            }
        }
    }
    let p99 = stat(&h, "op", false);
    let p50 = if batch > 1 {
        let samples = ops / batch;
        let st = h.stage("batched", samples);
        for s in 0..samples {
            st.time(|| {
                for k in 0..batch {
                    op(&target, s * batch + k);
                }
            });
        }
        stat(&h, "batched", true) / batch as u64
    } else {
        stat(&h, "op", true)
    };
    Measured {
        p50,
        p99,
        accept: granted as f64 / ops as f64,
    }
}

/// Sweeps, PRINTS the curve, and hands back both the `(size, p50)` rows the
/// classifier reads and the canonical (largest) point, whose p99 goes in the
/// manifest. Printing the accept ratio alongside is what makes it checkable
/// that a sweep point did not quietly slide onto one branch.
fn sweep(
    label: &str,
    sizes: &[usize],
    mut at: impl FnMut(usize) -> Measured,
) -> (Vec<(usize, u64)>, Measured) {
    let mut rows = Vec::with_capacity(sizes.len());
    let mut line = format!("sweep {label}:");
    let mut last = None;
    for &n in sizes {
        let m = at(n);
        line.push_str(&format!(
            " (n={n} p50={}ns p99={}ns accept={:.0}%)",
            m.p50,
            m.p99,
            m.accept * 100.0
        ));
        rows.push((n, m.p50));
        last = Some(m);
    }
    eprintln!("{line}");
    (rows, last.expect("non-empty sizes"))
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

    // The baseline: a plain `try_acquire` on the base GCRA limiter, cycled
    // over a fleet the same size as the canonical sweep point so it pays the
    // same cache footprint the features do. Every call is granted; GCRA's
    // reject path returns before the CAS, so the grant is the dearer branch
    // and the conservative baseline.
    let base = measure(
        || {
            (0..CANON_N)
                .map(|_| RateLimiter::new(BASE_RATE, BASE_BURST))
                .collect::<Vec<_>>()
        },
        |v, i| v[i % v.len()].try_acquire(),
        OPS,
        BATCH,
    );
    let base_p50 = base.p50;
    eprintln!(
        "base try_acquire over {CANON_N} limiters: p50={base_p50}ns p99={}ns accept={:.0}%",
        base.p99,
        base.accept * 100.0
    );
    // One hammered limiter, for context only. It is not the classifier's base:
    // comparing a fleet-cycling feature against a single hot limiter would
    // charge the feature for the cache misses the workload shape causes.
    let hot = measure(
        || RateLimiter::new(BASE_RATE, (2 * OPS) as u64),
        |r, _| r.try_acquire(),
        OPS,
        BATCH,
    );
    eprintln!(
        "base try_acquire on 1 hot limiter: p50={}ns p99={}ns (context only)",
        hot.p50, hot.p99
    );

    // ---------- token-bucket: mutex-guarded refill + batch drain ----------
    #[cfg(feature = "token-bucket")]
    {
        use subms_rate_limiter::TokenBucket;

        fn fleet(n: usize) -> Vec<TokenBucket> {
            let v: Vec<TokenBucket> = (0..n)
                .map(|_| TokenBucket::with_clock(TB_CAP, TB_RATE, Box::new(SteppingClock::new())))
                .collect();
            for (j, b) in v.iter().enumerate() {
                for _ in 0..PRE_DRAIN + (j & 1) {
                    b.try_acquire(1);
                }
            }
            v
        }

        let (rows, canon) = sweep("token-bucket/try_acquire", &SIZES, |n| {
            measure(
                || fleet(n),
                |v, i| v[i % v.len()].try_acquire(1),
                OPS,
                BATCH,
            )
        });
        let (cat, reason) = classify_feature(&rows, Some(base_p50), None);

        let avail = measure(
            || fleet(CANON_N),
            |v, i| {
                let _ = v[i % v.len()].available();
                true
            },
            OPS,
            BATCH,
        );
        let mut p99 = BTreeMap::new();
        p99.insert("try_acquire".to_string(), canon.p99);
        p99.insert("available".to_string(), avail.p99);
        manifest.set_feature("token-bucket", cat, &p99, &reason);
    }

    // ---------- hierarchical: child AND parent must both grant ----------
    #[cfg(feature = "hierarchical")]
    {
        use subms_rate_limiter::HierarchicalLimiter;

        // Swept on the CHILD COUNT, which is the only thing this feature can
        // scale. It is not a parent CHAIN - the source holds one parent and a
        // flat `Vec` of children, and a call is a `Vec` index plus a fixed
        // three bucket operations - so the cost is expected to be flat and the
        // sweep is what says so rather than a reading of the code.
        fn hier(n: usize) -> HierarchicalLimiter {
            let h = HierarchicalLimiter::with_clock_fn(
                HIER_PARENT_CAP,
                HIER_PARENT_RATE,
                n,
                TB_CAP,
                TB_RATE,
                || Box::new(SteppingClock::new()),
            );
            for c in 0..h.num_children() {
                for _ in 0..PRE_DRAIN + (c & 1) {
                    h.try_acquire(c, 1);
                }
            }
            h
        }

        let (rows, canon) = sweep("hierarchical/try_acquire", &SIZES, |n| {
            measure(
                || hier(n),
                |h, i| h.try_acquire(i % h.num_children(), 1),
                OPS,
                BATCH,
            )
        });
        let (cat, reason) = classify_feature(&rows, Some(base_p50), None);

        let mut p99 = BTreeMap::new();
        p99.insert("try_acquire".to_string(), canon.p99);
        manifest.set_feature("hierarchical", cat, &p99, &reason);
    }

    // ---------- distributed-backend: fixed-window counters ----------
    #[cfg(feature = "distributed-backend")]
    {
        use subms_rate_limiter::{DistributedLimiter, InMemoryBackend};

        // Keys are built once, outside every timed region: formatting one
        // inside the loop would put string construction in the measurement.
        // Fixed width so key length is not a second variable.
        let keys: Vec<String> = (0..DIST_CANON).map(|i| format!("key-{i:06}")).collect();

        let prefilled = |n: usize| {
            let d = DistributedLimiter::with_clock(
                Box::new(InMemoryBackend::new()),
                DIST_LIMIT,
                DIST_WINDOW_NS,
                Box::new(SteppingClock::new()),
            );
            for k in &keys[..n] {
                d.try_acquire(k);
            }
            d
        };

        let (rows, canon) = sweep("distributed-backend/try_acquire", &DIST_SIZES, |n| {
            measure(
                || prefilled(n),
                |d, i| d.try_acquire(&keys[i % DIST_HOT]),
                DIST_OPS,
                1,
            )
        });
        let (cat, reason) = classify_feature(&rows, Some(base_p50), None);

        let mut p99 = BTreeMap::new();
        p99.insert("try_acquire".to_string(), canon.p99);
        manifest.set_feature("distributed-backend", cat, &p99, &reason);
    }

    // ---------- metrics: counters around the same bucket ----------
    #[cfg(feature = "metrics")]
    {
        use subms_rate_limiter::MeteredTokenBucket;

        fn fleet(n: usize) -> Vec<MeteredTokenBucket> {
            let v: Vec<MeteredTokenBucket> = (0..n)
                .map(|_| {
                    MeteredTokenBucket::with_clock(TB_CAP, MET_RATE, Box::new(SteppingClock::new()))
                })
                .collect();
            for (j, b) in v.iter().enumerate() {
                for _ in 0..PRE_DRAIN + (j & 1) {
                    b.try_acquire(1);
                }
            }
            v
        }

        let (rows, canon) = sweep("metrics/try_acquire", &SIZES, |n| {
            measure(
                || fleet(n),
                |v, i| v[i % v.len()].try_acquire(1),
                OPS,
                BATCH,
            )
        });
        let (cat, reason) = classify_feature(&rows, Some(base_p50), None);

        let snap = measure(
            || fleet(CANON_N),
            |v, i| {
                let _ = v[i % v.len()].snapshot();
                true
            },
            OPS,
            BATCH,
        );
        let mut p99 = BTreeMap::new();
        p99.insert("try_acquire".to_string(), canon.p99);
        p99.insert("snapshot".to_string(), snap.p99);
        manifest.set_feature("metrics", cat, &p99, &reason);
    }

    // ---------- keyed: one GCRA limiter per key, sharded ----------
    #[cfg(feature = "keyed")]
    {
        use subms_rate_limiter::KeyedRateLimiter;

        // Swept on the LIVE KEY COUNT, the only axis this feature scales on.
        // A hash map lookup is expected to read flat; what the sweep is really
        // testing is that the sharded lock does not turn into the bottleneck as
        // the key set outgrows cache.
        //
        // Keys are formatted once, outside every timed region, and the clock is
        // driven rather than read: a fixed `now` of 0 with a burst wide enough
        // to cover every timed call keeps each op on the grant branch, which is
        // the dearer one (a reject returns before the map write).
        let keys: Vec<String> = (0..CANON_N).map(|i| format!("key-{i:06}")).collect();
        let burst = (OPS / SIZES[0] + 2) as u64;

        let (rows, canon) = sweep("keyed/try_acquire", &SIZES, |n| {
            measure(
                || KeyedRateLimiter::new(BASE_RATE, burst),
                |k, i| matches!(k.try_acquire_at(0, &keys[i % n], 1), Acquire::Ok),
                OPS,
                BATCH,
            )
        });
        let (cat, reason) = classify_feature(&rows, Some(base_p50), None);

        let mut p99 = BTreeMap::new();
        p99.insert("try_acquire".to_string(), canon.p99);
        manifest.set_feature("keyed", cat, &p99, &reason);
    }

    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(&path, manifest.to_json())?;
    io::stdout().write_all(manifest.to_json().as_bytes())?;
    Ok(())
}
