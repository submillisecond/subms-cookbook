//! Feature classification bench. Each feature's write+read round trip is
//! swept across three image widths (1 / 8 / 64 words); `classify_feature`
//! decides the category from the shape of that sweep and a single-op p99 per
//! stage is merge-written into `.subms/features/rust.json`.
//!
//! Single-threaded on purpose: a reader under a live writer measures the
//! retry rate, which is a property of the workload, not of the feature.
//!
//! Run:
//!   cargo run --release --example perf_features --features "harness array latch stats"

use std::collections::BTreeMap;
use std::hint::black_box;
use std::io::{self, Write};
use std::path::PathBuf;

use subms::{SubMsFeatureManifest, SubMsP99Source, SubMsPerfHarness, classify_feature, summarize};
use subms_seqlock::seqlock;

const WIDTHS: [usize; 3] = [1, 8, 64];
const ITEMS_PER_SAMPLE: usize = 1_024;
const SAMPLES: usize = 256;
const ROUNDS: usize = 7;
const OPS: usize = 50_000;
const WARM_NANOS: u64 = 300_000_000;
const WARM_MAX_SAMPLES: usize = 1_000;
const SLOTS: usize = 64;

fn stat(h: &SubMsPerfHarness, median: bool) -> u64 {
    summarize(h)
        .stages
        .iter()
        .find(|s| s.name == "op")
        .map_or(0, |s| if median { s.p50_ns } else { s.p99_ns })
}

fn batched(mut op: impl FnMut(usize) -> u64) -> u64 {
    let mut i = 0usize;
    let start = std::time::Instant::now();
    for _ in 0..WARM_MAX_SAMPLES {
        let mut acc = 0u64;
        for _ in 0..ITEMS_PER_SAMPLE {
            acc = acc.wrapping_add(op(i));
            i += 1;
        }
        black_box(acc);
        if start.elapsed().as_nanos() as u64 >= WARM_NANOS {
            break;
        }
    }
    let mut h = SubMsPerfHarness::new("seqlock-feature", "rust");
    let st = h.stage("op", SAMPLES);
    for _ in 0..SAMPLES {
        st.time(|| {
            let mut acc = 0u64;
            for _ in 0..ITEMS_PER_SAMPLE {
                acc = acc.wrapping_add(op(i));
                i += 1;
            }
            black_box(acc);
        });
    }
    stat(&h, true)
}

fn single(mut op: impl FnMut(usize) -> u64) -> u64 {
    for i in 0..OPS {
        black_box(op(i));
    }
    let mut h = SubMsPerfHarness::new("seqlock-feature", "rust");
    let st = h.stage("op", OPS);
    for i in 0..OPS {
        st.time(|| black_box(op(i)));
    }
    stat(&h, false)
}

/// One measurement per width, `ROUNDS` times width-interleaved, minimum kept.
fn sweep(label: &str, at: [fn() -> u64; 3]) -> Vec<(usize, u64)> {
    let mut runs: [Vec<u64>; 3] = std::array::from_fn(|_| Vec::with_capacity(ROUNDS));
    for _ in 0..ROUNDS {
        for (run, f) in runs.iter_mut().zip(&at) {
            run.push(f());
        }
    }
    let rows: Vec<(usize, u64)> = WIDTHS
        .iter()
        .zip(&runs)
        .map(|(n, rs)| (*n, *rs.iter().min().unwrap()))
        .collect();
    eprintln!("sweep {label}: {rows:?} raw {runs:?}");
    rows
}

fn base<const N: usize>() -> u64 {
    let (mut w, r) = seqlock([0u64; N]);
    batched(|i| {
        w.write(|img| img[0] = i as u64);
        r.read()[0]
    })
}

#[cfg(feature = "array")]
fn array<const N: usize>() -> u64 {
    let (mut w, r) = subms_seqlock::seqlock_array(&[[0u64; N]; SLOTS]);
    batched(|i| {
        w.write(i % SLOTS, |img| img[0] = i as u64);
        r.read(i % SLOTS)[0]
    })
}

#[cfg(feature = "latch")]
fn latch<const N: usize>() -> u64 {
    let (mut w, r) = subms_seqlock::seqlatch([0u64; N]);
    batched(|i| {
        w.write(|img| img[0] = i as u64);
        r.read()[0]
    })
}

#[cfg(feature = "stats")]
fn stats<const N: usize>() -> u64 {
    let (mut w, r) = seqlock([0u64; N]);
    let r = subms_seqlock::InstrumentedReader::new(r);
    batched(|i| {
        w.write(|img| img[0] = i as u64);
        r.read()[0]
    })
}

fn main() -> io::Result<()> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(".subms")
        .join("features")
        .join("rust.json");
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    let mut manifest = SubMsFeatureManifest::load_str("rust", &existing);
    let (source, instance) = SubMsP99Source::from_env();
    manifest.set_p99_source(source, instance.as_deref());

    let base_sweep = sweep("base/write+read", [base::<1>, base::<8>, base::<64>]);
    let base_p50 = base_sweep.last().map_or(0, |(_, v)| *v);
    eprintln!("base write+read p50 per {ITEMS_PER_SAMPLE}-op sample at 64 words: {base_p50}ns");

    #[cfg(feature = "array")]
    {
        let sw = sweep("array/write+read", [array::<1>, array::<8>, array::<64>]);
        let (cat, reason) = classify_feature(&sw, Some(base_p50), None);
        let (mut w, r) = subms_seqlock::seqlock_array(&[[0u64; 64]; SLOTS]);
        let write = single(|i| {
            w.write(i % SLOTS, |img| img[0] = i as u64);
            0
        });
        let read = single(|i| r.read(i % SLOTS)[0]);
        let mut p99 = BTreeMap::new();
        p99.insert("slot_write".to_string(), write);
        p99.insert("slot_read".to_string(), read);
        manifest.set_feature("array", cat, &p99, &reason);
    }

    #[cfg(feature = "latch")]
    {
        let sw = sweep("latch/write+read", [latch::<1>, latch::<8>, latch::<64>]);
        let (cat, reason) = classify_feature(&sw, Some(base_p50), None);
        let (mut w, r) = subms_seqlock::seqlatch([0u64; 64]);
        let write = single(|i| {
            w.write(|img| img[0] = i as u64);
            0
        });
        let read = single(|_| r.read()[0]);
        let mut p99 = BTreeMap::new();
        p99.insert("latch_write".to_string(), write);
        p99.insert("latch_read".to_string(), read);
        manifest.set_feature("latch", cat, &p99, &reason);
    }

    #[cfg(feature = "stats")]
    {
        let sw = sweep("stats/write+read", [stats::<1>, stats::<8>, stats::<64>]);
        let (cat, reason) = classify_feature(&sw, Some(base_p50), None);
        let (_w, r) = seqlock([0u64; 64]);
        let r = subms_seqlock::InstrumentedReader::new(r);
        let read = single(|_| r.read()[0]);
        let mut p99 = BTreeMap::new();
        p99.insert("counted_read".to_string(), read);
        manifest.set_feature("stats", cat, &p99, &reason);
    }

    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(&path, manifest.to_json())?;
    io::stdout().write_all(manifest.to_json().as_bytes())?;
    Ok(())
}
