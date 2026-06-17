//! Per-feature bench: runs the same 50k-entry workload against the base
//! `HdrHistogram`, plus each opt-in feature (`dual-recorder`,
//! `concurrent-writes`, `merge`, `decay`, `value-tagging`, `iterators`)
//! when its Cargo feature is enabled at compile time.
//!
//! The output JSON has one stage block per feature variant - e.g.
//! `base_record`, `dual_recorder_record`, `merge_merge`, etc. - so the
//! cookbook page can fill in the per-feature p99 table without juggling
//! multiple JSON files.
//!
//! Every recorded value is drawn from an exponential-ish latency
//! distribution (most values small, a long tail) so the bucket spread
//! resembles real latency capture rather than a flat sweep.
//!
//! Run:
//!   cargo run --release --example perf_features \
//!       --features "harness dual-recorder concurrent-writes merge decay value-tagging iterators"

use std::io::{self, Write};

use subms::{SubMsLcg, SubMsPerfHarness, SubMsStageKind, SubMsTimer, summarize, summary_to_json};
use subms_hdr_histogram::HdrHistogram;

const ENTRIES: usize = 50_000;
const SEED: u64 = 0;

/// Exponential-ish latency in nanoseconds: a base floor plus a value
/// whose magnitude follows -ln(u) so most samples are small and a thin
/// tail stretches into the microseconds. Deterministic under `SEED`.
fn next_latency_ns(rng: &mut SubMsLcg) -> u64 {
    let u = (rng.next_u32() as f64 + 1.0) / (u32::MAX as f64 + 2.0);
    let v = -(u.ln()) * 2_000.0;
    (v as u64) + 50
}

fn main() -> io::Result<()> {
    let mut h = SubMsPerfHarness::new("hdr-histogram-features", "rust");
    h.input("entries", &ENTRIES.to_string());
    h.input("seed", &SEED.to_string());
    h.add_meta("subms.recipe.slug", "subms-hdr-histogram");
    h.add_meta("subms.recipe.category", "observability");

    // ---------- base ----------
    {
        h.add_meta("subms.workload.feature", "base");
        let mut hist = HdrHistogram::new(3);
        let mut rng = SubMsLcg::new(SEED);
        let s = h
            .stage("base_record", ENTRIES)
            .with_kind(SubMsStageKind::HotPath);
        for _ in 0..ENTRIES {
            let v = next_latency_ns(&mut rng);
            let t0 = SubMsTimer::tick();
            hist.record(v);
            s.record(t0.elapsed_ns());
        }
        let s = h
            .stage("base_percentile", ENTRIES)
            .with_kind(SubMsStageKind::HotPath);
        for _ in 0..ENTRIES {
            let t0 = SubMsTimer::tick();
            let _ = hist.value_at_percentile(0.99);
            s.record(t0.elapsed_ns());
        }
    }

    // ---------- dual-recorder ----------
    #[cfg(feature = "dual-recorder")]
    {
        use subms_hdr_histogram::DualRecorder;
        h.add_meta("subms.workload.feature", "dual-recorder");
        let rec = DualRecorder::new(3);
        let mut rng = SubMsLcg::new(SEED);
        let s = h
            .stage("dual_recorder_record", ENTRIES)
            .with_kind(SubMsStageKind::HotPath);
        for _ in 0..ENTRIES {
            let v = next_latency_ns(&mut rng);
            let t0 = SubMsTimer::tick();
            rec.record(v);
            s.record(t0.elapsed_ns());
        }
        // get_interval is an occasional consumer call (rotate + drain).
        // It scans the inactive side, so time it on its own cadence.
        const INTERVALS: usize = 200;
        let s = h
            .stage("dual_recorder_get_interval", INTERVALS)
            .with_kind(SubMsStageKind::HotPath);
        for _ in 0..INTERVALS {
            for _ in 0..100 {
                rec.record(next_latency_ns(&mut rng));
            }
            let t0 = SubMsTimer::tick();
            let _ = rec.get_interval_histogram();
            s.record(t0.elapsed_ns());
        }
    }

    // ---------- concurrent-writes ----------
    #[cfg(feature = "concurrent-writes")]
    {
        use subms_hdr_histogram::ConcurrentHdrHistogram;
        h.add_meta("subms.workload.feature", "concurrent-writes");
        // Single-threaded per-op latency: the atomic fetch_add cost
        // without cross-thread contention noise.
        let hist = ConcurrentHdrHistogram::new(3);
        let mut rng = SubMsLcg::new(SEED);
        let s = h
            .stage("concurrent_writes_record", ENTRIES)
            .with_kind(SubMsStageKind::HotPath);
        for _ in 0..ENTRIES {
            let v = next_latency_ns(&mut rng);
            let t0 = SubMsTimer::tick();
            hist.record(v);
            s.record(t0.elapsed_ns());
        }
    }

    // ---------- merge ----------
    #[cfg(feature = "merge")]
    {
        use subms_hdr_histogram::merge;
        h.add_meta("subms.workload.feature", "merge");
        // Build two populated histograms outside the timed loop, then
        // time the merge alone. Each merge folds `src` into a fresh
        // copy-shaped `dst` so the scan cost is what we measure.
        let mut rng = SubMsLcg::new(SEED);
        let mut src = HdrHistogram::new(3);
        for _ in 0..ENTRIES {
            src.record(next_latency_ns(&mut rng));
        }
        const MERGES: usize = 2_000;
        let s = h
            .stage("merge_merge", MERGES)
            .with_kind(SubMsStageKind::BatchOp);
        for _ in 0..MERGES {
            let mut dst = HdrHistogram::new(3);
            dst.record(1);
            let t0 = SubMsTimer::tick();
            merge(&mut dst, &src).expect("same shape");
            s.record(t0.elapsed_ns());
        }
    }

    // ---------- decay ----------
    #[cfg(feature = "decay")]
    {
        use subms_hdr_histogram::{DecayingHdrHistogram, ManualClock};
        h.add_meta("subms.workload.feature", "decay");
        // ManualClock advanced a fixed step per record so the decay
        // multiply actually fires (a system clock would mostly read
        // zero elapsed between adjacent records and skip the work).
        let clock = ManualClock::new();
        let halflife_ns = 1_000_000_000u64;
        let mut hist = DecayingHdrHistogram::new(3, halflife_ns, &clock);
        let mut rng = SubMsLcg::new(SEED);
        let s = h
            .stage("decay_record", ENTRIES)
            .with_kind(SubMsStageKind::HotPath);
        for _ in 0..ENTRIES {
            let v = next_latency_ns(&mut rng);
            clock.advance_ns(10_000);
            let t0 = SubMsTimer::tick();
            hist.record(v);
            s.record(t0.elapsed_ns());
        }
        let s = h
            .stage("decay_percentile", ENTRIES)
            .with_kind(SubMsStageKind::HotPath);
        for _ in 0..ENTRIES {
            let t0 = SubMsTimer::tick();
            let _ = hist.value_at_percentile(0.99);
            s.record(t0.elapsed_ns());
        }
    }

    // ---------- value-tagging ----------
    #[cfg(feature = "value-tagging")]
    {
        use subms_hdr_histogram::TaggedHdrHistogram;
        h.add_meta("subms.workload.feature", "value-tagging");
        let mut hist = TaggedHdrHistogram::new(3);
        let mut rng = SubMsLcg::new(SEED);
        let s = h
            .stage("value_tagging_record", ENTRIES)
            .with_kind(SubMsStageKind::HotPath);
        for _ in 0..ENTRIES {
            let v = next_latency_ns(&mut rng);
            let tag = (rng.next_u32() % 8) as u8;
            let t0 = SubMsTimer::tick();
            hist.record(v, tag);
            s.record(t0.elapsed_ns());
        }
        let s = h
            .stage("value_tagging_per_tag_percentile", ENTRIES)
            .with_kind(SubMsStageKind::HotPath);
        for i in 0..ENTRIES {
            let tag = (i % 8) as u8;
            let t0 = SubMsTimer::tick();
            let _ = hist.value_at_percentile_for_tag(0.99, tag);
            s.record(t0.elapsed_ns());
        }
    }

    // ---------- iterators ----------
    #[cfg(feature = "iterators")]
    {
        h.add_meta("subms.workload.feature", "iterators");
        // Populate one histogram, then step the linear iterator over it.
        // Each `next()` advances to the next populated bucket; we time
        // individual steps across many full passes to fill ENTRIES.
        let mut hist = HdrHistogram::new(3);
        let mut rng = SubMsLcg::new(SEED);
        for _ in 0..ENTRIES {
            hist.record(next_latency_ns(&mut rng));
        }
        let s = h
            .stage("iterators_next", ENTRIES)
            .with_kind(SubMsStageKind::HotPath);
        let mut emitted = 0usize;
        while emitted < ENTRIES {
            let mut it = hist.iter_linear();
            loop {
                let t0 = SubMsTimer::tick();
                let step = it.next();
                let ns = t0.elapsed_ns();
                match step {
                    Some(_) => {
                        s.record(ns);
                        emitted += 1;
                        if emitted >= ENTRIES {
                            break;
                        }
                    }
                    None => break,
                }
            }
        }
    }

    let summary = summarize(&h);
    let mut stdout = io::stdout();
    summary_to_json(&summary, &mut stdout)?;
    writeln!(stdout)?;
    Ok(())
}
