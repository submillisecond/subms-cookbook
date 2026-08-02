//! Feature classification bench. Each feature's representative op is swept
//! across three SEGMENT LENGTHS, `classify_feature` DECIDES the category from
//! the shape of that sweep, and the decision plus a measured `p99ByStage` is
//! merge-written into `.subms/features/rust.json`.
//!
//! RECORD COUNT is the sweep axis; the record itself is a fixed 4 KiB block.
//! The checksum and compression features cost per BYTE, so growing the payload
//! alongside the segment would leave any slope with two explanations. With the
//! block pinned, a per-record read has to stay flat however long the segment
//! gets, and only an op that walks the whole segment can climb. 4 KiB is also
//! what puts every reader clear of the platform timer's 100 ns tick: at the
//! 12-byte payload `perf_main` uses, a base read costs well under one tick and
//! every feature comparison collapses into quantisation noise.
//!
//! Run:
//!   cargo run --release --example perf_features \
//!       --features "harness mmap crc32 xxh3 lz4 seek-index wal-cursor"

use std::collections::BTreeMap;
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Instant;

use subms::{
    SubMsFeatureManifest, SubMsP99Source, SubMsPerfHarness, SubMsTimer, classify_feature, summarize,
};
use subms_segment_reader::{SegmentReader, SegmentWriter};

/// One canonical block. Matches the 4 KiB SSTable/segment block size LevelDB
/// and RocksDB default to, and is held FIXED across the sweep.
const RECORD_BYTES: usize = 4096;
/// 1.05 / 4.2 / 16.8 MB of segment - a 16x span that starts inside cache and
/// ends outside it, so a per-record read that secretly depends on segment
/// length has somewhere to show it. The span was 4x longer until the mmap
/// numbers turned into a lottery: a 67 MB mapped segment does not keep its page
/// cache on a box with under a gigabyte free, so the measured drain paid faults
/// the warm drain had already paid and the p50 moved 300 -> 2200 ns between runs
/// of unchanged code. Sweep span is worth less than a working set that stays
/// resident.
const RECORDS: [usize; 3] = [256, 1024, 4096];
const CANON_N: usize = RECORDS[RECORDS.len() - 1];
/// Timed ops per measurement, FIXED across the sweep so a slope has one cause -
/// only the segment the ops run against grows. A drain repeats until it has
/// this many samples (every sweep length divides it), which also spreads the
/// figure over several reader instances: one instance is one heap placement for
/// the reader's scratch buffer, and a single unlucky placement moved the xxh3
/// p50 between 400 and 1000 ns across runs of unchanged code.
const OPS: usize = 65_536;
/// The open measurement runs far fewer reps than everything else: each one
/// leaves a mapping of the whole canonical segment behind until it is reclaimed,
/// and the Java port has no explicit unmap, so a full-size loop there is 65k
/// live mappings of a 67 MB file.
const MMAP_OPEN_REPS: usize = 1024;
const MMAP_OPEN_WARM: usize = 256;
/// Warm is budgeted by TIME and by op count, never by a fixed rep count. A
/// fixed count under-warms the smallest sweep point, which reads as a curve
/// that FALLS with size - the opposite of the structural signal and just as
/// wrong. Rust has no JIT, but the readers allocate per record and both the
/// allocator and (for mmap) the page tables need the same settling.
const WARM_NANOS: u64 = 300_000_000;
const WARM_OPS: usize = 200_000;

/// Token size and dictionary depth of the synthetic block content.
const TOKEN: usize = 16;
const DICT_TOKENS: usize = 64;

fn dict() -> &'static [u8] {
    static D: OnceLock<Vec<u8>> = OnceLock::new();
    D.get_or_init(|| {
        let mut d = vec![0u8; DICT_TOKENS * TOKEN];
        let mut s: u64 = 0x2545_f491_4f6c_dd1d;
        for b in d.iter_mut() {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            *b = (s >> 24) as u8;
        }
        d
    })
}

/// Fills one block with a pseudo-random sequence of 16-byte tokens drawn from a
/// fixed 64-entry dictionary. LZ4 lands near 2.5:1 and, more to the point, has
/// to walk a couple of hundred match/literal steps per block.
///
/// The first attempt was a random quarter repeated three times. It hits a
/// similar ratio and is completely wrong: LZ4 encodes it as one literal run
/// plus one 3 KB match, so the decode is two memcpys and the recipe would have
/// published memcpy throughput (300 ns for a 4 KiB block) as its decompression
/// cost. Compression ratio is not evidence that a decoder is doing work.
fn fill_record(i: usize, out: &mut [u8]) {
    let d = dict();
    let mut s = (i as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1;
    for chunk in out.chunks_mut(TOKEN) {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        let t = ((s >> 33) as usize % DICT_TOKENS) * TOKEN;
        chunk.copy_from_slice(&d[t..t + chunk.len()]);
    }
}

fn build_base(n: usize) -> Vec<u8> {
    let mut buf = Vec::with_capacity(n * (RECORD_BYTES + 4));
    let mut rec = vec![0u8; RECORD_BYTES];
    {
        let mut w = SegmentWriter::new(&mut buf);
        for i in 0..n {
            fill_record(i, &mut rec);
            w.write(&rec).expect("write");
        }
    }
    buf
}

fn stat(h: &SubMsPerfHarness, median: bool) -> u64 {
    summarize(h)
        .stages
        .iter()
        .find(|s| s.name == "op")
        .map_or(0, |s| if median { s.p50_ns } else { s.p99_ns })
}

/// Reads every record of a segment through one reader, timing each read.
/// `reset` puts the reader back at the head OUTSIDE the timed region - for the
/// in-memory readers that is a fresh construction, for mmap it is a rewind of
/// the single live map, which is what keeps first-touch page faults in the warm
/// loop instead of on the measured pass.
fn drain<R>(
    n: usize,
    r: &mut R,
    mut reset: impl FnMut(&mut R),
    mut next: impl FnMut(&mut R),
    median: bool,
) -> u64 {
    let start = Instant::now();
    let mut warmed = 0usize;
    while warmed < WARM_OPS && (start.elapsed().as_nanos() as u64) < WARM_NANOS {
        reset(r);
        for _ in 0..n {
            next(r);
        }
        warmed += n;
    }
    let mut h = SubMsPerfHarness::new("segment-feature", "rust");
    let st = h.stage("op", OPS);
    let mut done = 0usize;
    while done < OPS {
        reset(r);
        for _ in 0..n {
            st.time(|| next(r));
        }
        done += n;
    }
    stat(&h, median)
}

/// A random-access op. `op` returns the nanoseconds to record, so an op that
/// needs untimed positioning first (seek, then read) can exclude it without a
/// second closure fighting the first for the reader.
fn keyed(ops: usize, warm_ops: usize, mut op: impl FnMut(usize) -> u64, median: bool) -> u64 {
    let start = Instant::now();
    let mut i = 0usize;
    while i < warm_ops && (start.elapsed().as_nanos() as u64) < WARM_NANOS {
        op(i);
        i += 1;
    }
    let mut h = SubMsPerfHarness::new("segment-feature", "rust");
    let st = h.stage("op", ops);
    for i in 0..ops {
        let ns = op(i);
        st.record(ns);
    }
    stat(&h, median)
}

/// Sweeps and PRINTS the curve. A ratio-compressed or non-monotonic curve
/// classifies as flat and the rows are the only place it shows.
fn sweep(label: &str, mut at: impl FnMut(usize) -> u64) -> Vec<(usize, u64)> {
    let rows: Vec<(usize, u64)> = RECORDS.iter().map(|&n| (n, at(n))).collect();
    eprintln!("sweep {label}: {rows:?}");
    rows
}

fn seek_targets(total: u32) -> Vec<u32> {
    let mut state: u64 = 0x9e37_79b9_7f4a_7c15;
    (0..OPS)
        .map(|_| {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((state >> 33) as u32) % total
        })
        .collect()
}

fn unique_temp_dir() -> PathBuf {
    let mut p = std::env::temp_dir();
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    p.push(format!(
        "subms-segment-features-{stamp}-{}",
        std::process::id()
    ));
    p
}

fn main() -> io::Result<()> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(".subms")
        .join("features")
        .join("rust.json");
    let mut manifest = SubMsFeatureManifest::load("rust", &path);
    // Stamp the box these numbers came from. The bench runs wherever it is
    // invoked, so an unstamped manifest is indistinguishable from a fleet
    // capture; the renderer will not publish one it cannot attribute.
    let (source, instance) = SubMsP99Source::from_env();
    manifest.set_p99_source(source, instance.as_deref());

    // The baseline: a plain sequential `next_record` over a base segment. Every
    // feature decorates that read, so that is what each is measured against.
    //
    // The FIRST drain of the process is discarded. It is not a measurement, it
    // is the process warming up, and the 300 ms warm inside one drain does not
    // cover it: probed four times in a row the Java port read 1200, 300, 300,
    // 300 ns. This reading is the divisor for every feature category, and an
    // inflated one silently demotes real hot-path features to auxiliary. Rust's
    // first pass is usually already warm; the discard is kept here so both ports
    // arrive at the baseline the same way.
    let canon = build_base(CANON_N);
    let mut br = SegmentReader::new(canon.as_slice());
    let cold = drain(
        CANON_N,
        &mut br,
        |r| *r = SegmentReader::new(canon.as_slice()),
        |r| {
            r.next_record().expect("read");
        },
        true,
    );
    let base_p50 = drain(
        CANON_N,
        &mut br,
        |r| *r = SegmentReader::new(canon.as_slice()),
        |r| {
            r.next_record().expect("read");
        },
        true,
    );
    eprintln!(
        "base next p50: {base_p50}ns (first pass {cold}ns, discarded) over \
         {CANON_N} x {RECORD_BYTES}B records"
    );

    // ---------- mmap: zero-copy read out of a mapped file ----------
    #[cfg(feature = "mmap")]
    {
        use subms_segment_reader::MmapSegmentReader;
        let dir = unique_temp_dir();
        std::fs::create_dir_all(&dir)?;

        // One map per sweep point, rewound between passes. Mapping afresh for
        // every pass would charge each measured read a minor fault - roughly one
        // per 4 KiB record here - and publish the page-fault floor as the read
        // cost. The warm drain touches every page once; what is measured after
        // it is the parse, and the first-touch cost is a property of the OS and
        // the storage, not of this code.
        let sw = sweep("mmap/next", |n| {
            let seg = build_base(n);
            let p = dir.join(format!("sweep-{n}.bin"));
            std::fs::write(&p, &seg).expect("write segment");
            let mut r = MmapSegmentReader::open(&p).expect("mmap open");
            let v = drain(
                n,
                &mut r,
                |r| r.rewind(),
                |r| {
                    r.next_record().expect("read");
                },
                true,
            );
            drop(r);
            std::fs::remove_file(&p).ok();
            v
        });
        let (cat, reason) = classify_feature(&sw, Some(base_p50), None);

        let p = dir.join("canon.bin");
        std::fs::write(&p, &canon)?;
        let mut r = MmapSegmentReader::open(&p).expect("mmap open");
        let mut p99 = BTreeMap::new();
        p99.insert(
            "next".to_string(),
            drain(
                CANON_N,
                &mut r,
                |r| r.rewind(),
                |r| {
                    r.next_record().expect("read");
                },
                false,
            ),
        );
        drop(r);
        // The feature's other claim: open is constant-time because nothing is
        // read, only mapped.
        p99.insert(
            "open".to_string(),
            keyed(
                MMAP_OPEN_REPS,
                MMAP_OPEN_WARM,
                |_| {
                    let t0 = SubMsTimer::tick();
                    let m = MmapSegmentReader::open(&p).expect("mmap open");
                    let ns = t0.elapsed_ns();
                    drop(m);
                    ns
                },
                false,
            ),
        );
        manifest.set_feature("mmap", cat, &p99, &reason);

        std::fs::remove_file(&p).ok();
        std::fs::remove_dir(&dir).ok();
    }

    // ---------- crc32: CRC32C trailer verified per block ----------
    #[cfg(feature = "crc32")]
    {
        use subms_segment_reader::Crc32SegmentReader;
        use subms_segment_reader::features::crc32::Crc32SegmentWriter;

        fn build(n: usize) -> Vec<u8> {
            let mut buf = Vec::with_capacity(n * (RECORD_BYTES + 8));
            let mut rec = vec![0u8; RECORD_BYTES];
            {
                let mut w = Crc32SegmentWriter::new(&mut buf);
                for i in 0..n {
                    fill_record(i, &mut rec);
                    w.write(&rec).expect("write");
                }
            }
            buf
        }

        let sw = sweep("crc32/next", |n| {
            let seg = build(n);
            let mut r = Crc32SegmentReader::new(seg.as_slice());
            drain(
                n,
                &mut r,
                |r| *r = Crc32SegmentReader::new(seg.as_slice()),
                |r| {
                    r.next_record().expect("read");
                },
                true,
            )
        });
        let (cat, reason) = classify_feature(&sw, Some(base_p50), None);

        let seg = build(CANON_N);
        let mut r = Crc32SegmentReader::new(seg.as_slice());
        let mut p99 = BTreeMap::new();
        p99.insert(
            "next".to_string(),
            drain(
                CANON_N,
                &mut r,
                |r| *r = Crc32SegmentReader::new(seg.as_slice()),
                |r| {
                    r.next_record().expect("read");
                },
                false,
            ),
        );
        manifest.set_feature("crc32", cat, &p99, &reason);
    }

    // ---------- xxh3: xxHash3-64 trailer verified per block ----------
    #[cfg(feature = "xxh3")]
    {
        use subms_segment_reader::Xxh3SegmentReader;
        use subms_segment_reader::features::xxh3::Xxh3SegmentWriter;

        fn build(n: usize) -> Vec<u8> {
            let mut buf = Vec::with_capacity(n * (RECORD_BYTES + 12));
            let mut rec = vec![0u8; RECORD_BYTES];
            {
                let mut w = Xxh3SegmentWriter::new(&mut buf);
                for i in 0..n {
                    fill_record(i, &mut rec);
                    w.write(&rec).expect("write");
                }
            }
            buf
        }

        let sw = sweep("xxh3/next", |n| {
            let seg = build(n);
            let mut r = Xxh3SegmentReader::new(seg.as_slice());
            drain(
                n,
                &mut r,
                |r| *r = Xxh3SegmentReader::new(seg.as_slice()),
                |r| {
                    r.next_record().expect("read");
                },
                true,
            )
        });
        let (cat, reason) = classify_feature(&sw, Some(base_p50), None);

        let seg = build(CANON_N);
        let mut r = Xxh3SegmentReader::new(seg.as_slice());
        let mut p99 = BTreeMap::new();
        p99.insert(
            "next".to_string(),
            drain(
                CANON_N,
                &mut r,
                |r| *r = Xxh3SegmentReader::new(seg.as_slice()),
                |r| {
                    r.next_record().expect("read");
                },
                false,
            ),
        );
        manifest.set_feature("xxh3", cat, &p99, &reason);
    }

    // ---------- lz4: compressed blocks, decompressed on read ----------
    #[cfg(feature = "lz4")]
    {
        use subms_segment_reader::{Lz4BlockWriter, Lz4SegmentReader};

        fn build(n: usize) -> Vec<u8> {
            let mut buf = Vec::with_capacity(n * RECORD_BYTES / 2);
            let mut rec = vec![0u8; RECORD_BYTES];
            {
                let mut w = Lz4BlockWriter::new(&mut buf);
                for i in 0..n {
                    fill_record(i, &mut rec);
                    w.write(&rec).expect("write");
                }
            }
            buf
        }

        // Ratio is a property of `fill_record`, so it is identical at every
        // sweep point; printed because a compression bench with an unstated
        // ratio is not reproducible.
        let ratio = (CANON_N * RECORD_BYTES) as f64 / build(CANON_N).len() as f64;
        eprintln!("lz4 compression ratio: {ratio:.2}x");

        let sw = sweep("lz4/next", |n| {
            let seg = build(n);
            let mut r = Lz4SegmentReader::new(seg.as_slice());
            drain(
                n,
                &mut r,
                |r| *r = Lz4SegmentReader::new(seg.as_slice()),
                |r| {
                    r.next_record().expect("read");
                },
                true,
            )
        });
        let (cat, reason) = classify_feature(&sw, Some(base_p50), None);

        let seg = build(CANON_N);
        let mut r = Lz4SegmentReader::new(seg.as_slice());
        let mut p99 = BTreeMap::new();
        p99.insert(
            "next".to_string(),
            drain(
                CANON_N,
                &mut r,
                |r| *r = Lz4SegmentReader::new(seg.as_slice()),
                |r| {
                    r.next_record().expect("read");
                },
                false,
            ),
        );
        manifest.set_feature("lz4", cat, &p99, &reason);
    }

    // ---------- seek-index: sparse index, random-access positioning ----------
    #[cfg(feature = "seek-index")]
    {
        use subms_segment_reader::IndexedSegmentReader;

        // Swept on `seek_to_block`, which is the op the feature exists for. The
        // index build in `open` is the other half of the bargain and it IS O(n),
        // but it is a construction cost paid once per reader, not an op a caller
        // repeats - publishing it as a per-op stage would read as a latency
        // claim on something nobody calls in a loop. The seek itself is a binary
        // search over n/64 entries plus a scan bounded by the 64-block stride,
        // so it should stay flat and only widen as the target span outgrows
        // cache.
        let sw = sweep("seek-index/seek", |n| {
            let seg = build_base(n);
            let mut r = IndexedSegmentReader::open(seg.as_slice()).expect("index open");
            let targets = seek_targets(r.total_blocks());
            keyed(
                OPS,
                WARM_OPS,
                |i| {
                    let t = targets[i % targets.len()];
                    let t0 = SubMsTimer::tick();
                    r.seek_to_block(t).expect("seek");
                    t0.elapsed_ns()
                },
                true,
            )
        });
        let (cat, reason) = classify_feature(&sw, Some(base_p50), None);

        let mut r = IndexedSegmentReader::open(canon.as_slice()).expect("index open");
        let targets = seek_targets(r.total_blocks());
        let mut p99 = BTreeMap::new();
        p99.insert(
            "seek".to_string(),
            keyed(
                OPS,
                WARM_OPS,
                |i| {
                    let t = targets[i % targets.len()];
                    let t0 = SubMsTimer::tick();
                    r.seek_to_block(t).expect("seek");
                    t0.elapsed_ns()
                },
                false,
            ),
        );
        // The read that follows a seek lands on a cold line every time, which is
        // the cost a random-access reader actually pays; the sequential base
        // number is prefetched and does not describe it.
        p99.insert(
            "next_after_seek".to_string(),
            keyed(
                OPS,
                WARM_OPS,
                |i| {
                    r.seek_to_block(targets[i % targets.len()]).expect("seek");
                    let t0 = SubMsTimer::tick();
                    r.next_record().expect("read");
                    t0.elapsed_ns()
                },
                false,
            ),
        );
        manifest.set_feature("seek-index", cat, &p99, &reason);
    }

    // ---------- wal-cursor: durability watermark on the read path ----------
    #[cfg(feature = "wal-cursor")]
    {
        use subms_segment_reader::WalCursorReader;

        // `read_committed` returns ONE block, not everything below the
        // watermark, so this is a per-record op and not a scan - there is no
        // bulk drain in the API to sweep instead. The watermark check is a
        // single comparison on top of the base parse, so a flat curve at base
        // cost is the expected result and a rise would mean the watermark is
        // doing more than it claims.
        let sw = sweep("wal-cursor/read_committed", |n| {
            let seg = build_base(n);
            let mut r = WalCursorReader::with_committed(seg.as_slice(), seg.len());
            drain(
                n,
                &mut r,
                |r| *r = WalCursorReader::with_committed(seg.as_slice(), seg.len()),
                |r| {
                    r.read_committed().expect("read");
                },
                true,
            )
        });
        let (cat, reason) = classify_feature(&sw, Some(base_p50), None);

        let mut r = WalCursorReader::with_committed(canon.as_slice(), canon.len());
        let mut p99 = BTreeMap::new();
        p99.insert(
            "read_committed".to_string(),
            drain(
                CANON_N,
                &mut r,
                |r| *r = WalCursorReader::with_committed(canon.as_slice(), canon.len()),
                |r| {
                    r.read_committed().expect("read");
                },
                false,
            ),
        );
        p99.insert(
            "next_record".to_string(),
            drain(
                CANON_N,
                &mut r,
                |r| *r = WalCursorReader::with_committed(canon.as_slice(), canon.len()),
                |r| {
                    r.next_record().expect("read");
                },
                false,
            ),
        );
        manifest.set_feature("wal-cursor", cat, &p99, &reason);
    }

    manifest.save(&path)?;
    io::stdout().write_all(manifest.to_json().as_bytes())?;
    Ok(())
}
