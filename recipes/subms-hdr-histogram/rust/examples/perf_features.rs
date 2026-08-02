//! Feature classification bench. Each feature's representative op is swept
//! across three PRECISIONS, `classify_feature` DECIDES the category from the
//! shape of that sweep, and the decision plus a measured `p99ByStage` is
//! merge-written into `.subms/features/rust.json`.
//!
//! Significant digits is the sweep axis because it is what sets a histogram's
//! size: 3, 4 and 5 digits give 2048, 32768 and 262144 sub-buckets. A
//! `record` writes one bucket regardless, so it should read flat; anything that
//! folds the bucket array should climb. `decay`'s record does the second while
//! looking like the first.
//!
//! Run:
//!   cargo run --release --example perf_features \
//!       --features "harness dual-recorder concurrent-writes merge decay value-tagging iterators"

use std::collections::BTreeMap;
use std::io::{self, Write};
use std::path::PathBuf;

use subms::{SubMsFeatureManifest, SubMsP99Source, SubMsPerfHarness, classify_feature, summarize};
use subms_hdr_histogram::HdrHistogram;

/// 2048 / 32768 / 262144 sub-buckets, a 128x span. NOT 1/2/3 digits: that gives
/// 32 / 256 / 2048, and at 32 buckets a fold is entirely fixed per-call cost, so
/// three genuinely O(buckets) ops - the interval read, the decaying record, the
/// percentile walk - all measured flat and classified hot-path. Sweeping from a
/// size where the fold already dominates measures the fold.
const DIGITS: [u32; 3] = [3, 4, 5];
const CANON_D: u32 = DIGITS[DIGITS.len() - 1];
/// Recorded ops per measurement. Fixed across the sweep so a slope has one cause.
const OPS: usize = 20_000;
/// Timed repeats for a whole-array op, far too slow to run OPS times.
const BULK_REPS: usize = 64;
/// Bulk warmup is TIME-BOXED, not a fixed rep count - the same fix the Java port
/// needs, for a different reason. Rust has no JIT, but an op that ALLOCATES has
/// an allocator and a page-fault ramp, and 8 reps do not settle either: the
/// interval read's smallest sweep point moved 7000 -> 30800 -> 44800 ns across
/// three runs and flipped the feature between structural and hot-path. A budget
/// gives cheap sizes thousands of reps and expensive ones enough.
const BULK_WARM_NANOS: u64 = 300_000_000;
const BULK_WARM_MAX_REPS: usize = 5_000;

const MAX_VALUE: u64 = 10_000_000;

/// Values spread over seven orders of magnitude, so buckets across the whole
/// range carry counts and a fold cannot skip most of the array.
fn value_at(i: usize) -> u64 {
    1 + ((i as u64).wrapping_mul(2_654_435_761) % MAX_VALUE)
}

fn sub_count(d: u32) -> usize {
    HdrHistogram::new(d).sub_count() as usize
}

fn stat(h: &SubMsPerfHarness, median: bool) -> u64 {
    summarize(h)
        .stages
        .iter()
        .find(|s| s.name == "op")
        .map_or(0, |s| if median { s.p50_ns } else { s.p99_ns })
}

fn keyed(mut op: impl FnMut(usize), median: bool) -> u64 {
    let mut h = SubMsPerfHarness::new("hdr-feature", "rust");
    let st = h.stage("op", OPS);
    for i in 0..OPS {
        st.time(|| op(i));
    }
    stat(&h, median)
}

/// A whole-array op. `setup` runs OUTSIDE the timed region and the first
/// `BULK_WARM` reps are discarded: measured cold, a bulk op lands its
/// first-touch cost on whichever sweep point runs first, which reads as a curve
/// that FALLS with size - the opposite of the structural signal.
fn bulk<T>(mut setup: impl FnMut() -> T, mut op: impl FnMut(&mut T), median: bool) -> u64 {
    let mut input = setup();
    let start = std::time::Instant::now();
    for _ in 0..BULK_WARM_MAX_REPS {
        op(&mut input);
        if start.elapsed().as_nanos() as u64 >= BULK_WARM_NANOS {
            break;
        }
    }
    let mut h = SubMsPerfHarness::new("hdr-feature", "rust");
    let st = h.stage("op", BULK_REPS);
    for _ in 0..BULK_REPS {
        st.time(|| op(&mut input));
    }
    stat(&h, median)
}

/// Sweeps and PRINTS the curve, indexed by SUB-BUCKET COUNT rather than by
/// digits - the classifier reads the size column as a magnitude.
fn sweep(label: &str, mut at: impl FnMut(u32) -> u64) -> Vec<(usize, u64)> {
    let rows: Vec<(usize, u64)> = DIGITS.iter().map(|&d| (sub_count(d), at(d))).collect();
    eprintln!("sweep {label}: {rows:?}");
    rows
}

/// Filled to OCCUPANCY, not to a fixed record count. `merge` and the iterators
/// visit NON-EMPTY buckets, so a fixed 20k values leaves 20k of them occupied
/// whether the array holds 2048 buckets or 262144 - the op stops scaling and
/// both features read flat. Recording `sub_count` values keeps the occupied
/// fraction roughly constant, which is what makes the sweep a size sweep.
fn filled(d: u32) -> HdrHistogram {
    let mut h = HdrHistogram::new(d);
    for i in 0..sub_count(d) {
        h.record(value_at(i));
    }
    h
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

    // The baseline: base `record`, the per-op write path. Every feature is
    // classified against the cost of the write it is decorating.
    let mut base = HdrHistogram::new(CANON_D);
    let base_p50 = keyed(|i| base.record(value_at(i)), true);
    eprintln!("base record p50: {base_p50}ns");

    // ---------- concurrent-writes: atomic buckets, &self record ----------
    #[cfg(feature = "concurrent-writes")]
    {
        use subms_hdr_histogram::ConcurrentHdrHistogram;
        let sw = sweep("concurrent-writes/record", |d| {
            let c = ConcurrentHdrHistogram::new(d);
            keyed(|i| c.record(value_at(i)), true)
        });
        let (cat, reason) = classify_feature(&sw, Some(base_p50), None);

        let c = ConcurrentHdrHistogram::new(CANON_D);
        let mut p99 = BTreeMap::new();
        p99.insert("record".to_string(), keyed(|i| c.record(value_at(i)), false));
        p99.insert(
            "percentile".to_string(),
            keyed(|_| _ = c.value_at_percentile(99.0), false),
        );
        manifest.set_feature("concurrent-writes", cat, &p99, &reason);
    }

    // ---------- dual-recorder: hot/stable pair, swap and drain ----------
    #[cfg(feature = "dual-recorder")]
    {
        use subms_hdr_histogram::DualRecorder;
        // Swept on the interval read, not on `record`. The record is the same
        // atomic write the concurrent feature already covers; the swap-and-drain
        // is what a dual recorder is FOR, and it is O(buckets).
        let sw = sweep("dual-recorder/interval", |d| {
            let r = DualRecorder::new(d);
            for i in 0..sub_count(d) {
                r.record(value_at(i));
            }
            bulk(|| (), |()| _ = r.get_interval_histogram(), true)
        });
        // PINNED structural. The drain is O(buckets) from the source in BOTH
        // ports, but it is DESTRUCTIVE, so repeating it measures an already-empty
        // histogram rather than the drain: after the first rep each side has
        // nothing left to copy. Refilling between reps would put the refill
        // inside the timed region, which is the bug the ART port shipped. The
        // two ports disagree precisely here for that reason - Rust copies the
        // counter array unconditionally (468us at 262144 buckets) while Java
        // collapses a high-water index and reads 100ns - and neither number is
        // the operation a caller performs. Recording either as hot-path would
        // say a whole-array drain is safe per-op.
        let (cat, reason) = classify_feature(
            &sw,
            Some(base_p50),
            Some(subms::SubMsFeatureCategory::Structural),
        );

        let r = DualRecorder::new(CANON_D);
        for i in 0..OPS {
            r.record(value_at(i));
        }
        let mut p99 = BTreeMap::new();
        p99.insert("record".to_string(), keyed(|i| r.record(value_at(i)), false));
        p99.insert(
            "interval_read".to_string(),
            bulk(|| (), |()| _ = r.get_interval_histogram(), false),
        );
        manifest.set_feature("dual-recorder", cat, &p99, &reason);
    }

    // ---------- merge: element-wise add over the bucket arrays ----------
    #[cfg(feature = "merge")]
    {
        use subms_hdr_histogram::merge;
        // Both histograms are built by `setup`, outside the timed region.
        // Repeating the merge grows the destination's counts but does identical
        // work each rep, so the figure is the merge and nothing else.
        let sw = sweep("merge/merge", |d| {
            bulk(
                || (filled(d), filled(d)),
                |(dst, src)| merge(dst, src).expect("same precision"),
                true,
            )
        });
        let (cat, reason) = classify_feature(&sw, Some(base_p50), None);

        let mut p99 = BTreeMap::new();
        p99.insert(
            "merge".to_string(),
            bulk(
                || (filled(CANON_D), filled(CANON_D)),
                |(dst, src)| merge(dst, src).expect("same precision"),
                false,
            ),
        );
        manifest.set_feature("merge", cat, &p99, &reason);
    }

    // ---------- decay: exponentially-weighted counts ----------
    #[cfg(feature = "decay")]
    {
        use subms_hdr_histogram::{Clock, DecayingHdrHistogram};
        // The clock ADVANCES on every read. A frozen clock lets `decay_to_now`
        // early-return and the feature measures as a plain record, which is the
        // opposite of the truth: with time moving, every record first brings the
        // whole counter array up to date, so the write is O(buckets). Freezing
        // the clock here would have published that as hot-path.
        struct TickingClock(std::cell::Cell<u64>);
        impl Clock for TickingClock {
            fn now_ns(&self) -> u64 {
                self.0.set(self.0.get() + 1_000_000);
                self.0.get()
            }
        }
        let halflife = 1_000_000_000;
        let sw = sweep("decay/record", |d| {
            let mut x =
                DecayingHdrHistogram::new(d, halflife, TickingClock(std::cell::Cell::new(0)));
            keyed(|i| x.record(value_at(i)), true)
        });
        let (cat, reason) = classify_feature(&sw, Some(base_p50), None);

        let mut x =
            DecayingHdrHistogram::new(CANON_D, halflife, TickingClock(std::cell::Cell::new(0)));
        let mut p99 = BTreeMap::new();
        p99.insert("record".to_string(), keyed(|i| x.record(value_at(i)), false));
        p99.insert(
            "percentile".to_string(),
            keyed(|_| _ = x.value_at_percentile(99.0), false),
        );
        manifest.set_feature("decay", cat, &p99, &reason);
    }

    // ---------- value-tagging: a parallel histogram per tag ----------
    #[cfg(feature = "value-tagging")]
    {
        use subms_hdr_histogram::TaggedHdrHistogram;
        let sw = sweep("value-tagging/record", |d| {
            let mut t = TaggedHdrHistogram::new(d);
            keyed(|i| t.record(value_at(i), (i % 4) as u8), true)
        });
        let (cat, reason) = classify_feature(&sw, Some(base_p50), None);

        let mut t = TaggedHdrHistogram::new(CANON_D);
        let mut p99 = BTreeMap::new();
        p99.insert(
            "record".to_string(),
            keyed(|i| t.record(value_at(i), (i % 4) as u8), false),
        );
        p99.insert(
            "percentile_for_tag".to_string(),
            keyed(|_| _ = t.value_at_percentile_for_tag(99.0, 0), false),
        );
        manifest.set_feature("value-tagging", cat, &p99, &reason);
    }

    // ---------- iterators: linear / logarithmic / percentile walks ----------
    #[cfg(feature = "iterators")]
    {
        // A percentile walk accumulates counts across the bucket array, so it is
        // O(buckets), and the sweep is monotonic and strongly rising. It is not
        // 64x: the walk emits a BOUNDED number of entries (1% steps, ~100 of
        // them) however large the array is, so the per-bucket work amortises
        // better at the top and it measures ~57x over a 128x span - under the
        // classifier's 0.5 guard.
        //
        // `iter_linear` was tried as the swept op instead, on the reasoning that
        // it visits every bucket. It does not: it steps by VALUE unit, so over a
        // 10^7 value range it emits millions of entries and the walk never
        // finished. Wrong op, not a slower one.
        //
        // PINNED structural rather than published as hot-path, which would tell
        // a reader a full percentile walk is safe per-operation. It is not.
        let sw = sweep("iterators/percentiles", |d| {
            let h = filled(d);
            bulk(|| (), |()| _ = h.iter_percentiles(1.0).count(), true)
        });
        let (cat, reason) = classify_feature(
            &sw,
            Some(base_p50),
            Some(subms::SubMsFeatureCategory::Structural),
        );

        let h = filled(CANON_D);
        let mut p99 = BTreeMap::new();
        p99.insert(
            "iter_percentiles".to_string(),
            bulk(|| (), |()| _ = h.iter_percentiles(1.0).count(), false),
        );
        p99.insert(
            "iter_logarithmic".to_string(),
            bulk(|| (), |()| _ = h.iter_logarithmic().count(), false),
        );
        manifest.set_feature("iterators", cat, &p99, &reason);
    }

    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(&path, manifest.to_json())?;
    io::stdout().write_all(manifest.to_json().as_bytes())?;
    Ok(())
}
