//! Per-feature bench: runs a 50k-block read workload against the base
//! `SegmentReader` plus each opt-in feature reader (`mmap`, `crc32`,
//! `xxh3`, `lz4`, `seek-index`, `wal-cursor`) when its Cargo feature is
//! enabled at compile time.
//!
//! The output JSON has one stage block per feature variant - `base_next`,
//! `mmap_next`, `crc32_next`, `xxh3_next`, `lz4_next`, `seek` +
//! `next_after_seek`, `read_committed` - so the cookbook page can fill in
//! the per-feature p99 table from a single JSON file.
//!
//! The mmap path reads a real temp file (that is the whole point of the
//! feature); every other reader takes a byte slice, matching how the base
//! reader and the recipe adapter are exercised. The temp file lives under
//! a unique subdir of `std::env::temp_dir()` and is removed at the end.
//!
//! Run:
//!   cargo run --release --example perf_features \
//!       --features "harness mmap crc32 xxh3 lz4 seek-index wal-cursor"

use std::io::{self, Write};
use std::path::PathBuf;

use subms::{SubMsPerfHarness, SubMsStageKind, SubMsTimer, summarize, summary_to_json};
use subms_segment_reader::{SegmentReader, SegmentWriter};

const ENTRIES: usize = 50_000;

/// Small fixed-shape payload - "record-{i}" is 8-13 bytes, matching the
/// recipe adapter's workload so the base number lines up with rust.json.
fn payload(i: usize) -> String {
    format!("record-{i}")
}

/// Build a plain length-prefix segment in memory.
fn build_base() -> Vec<u8> {
    let mut buf = Vec::with_capacity(ENTRIES * 24);
    let mut w = SegmentWriter::new(&mut buf);
    for i in 0..ENTRIES {
        w.write(payload(i).as_bytes()).expect("write");
    }
    buf
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
    let mut h = SubMsPerfHarness::new("segment-reader-features", "rust");
    h.input("entries", &ENTRIES.to_string());
    h.add_meta("subms.recipe.slug", "subms-segment-reader");
    h.add_meta("subms.recipe.category", "storage");

    let base = build_base();
    h.add_meta("segment_bytes", &base.len().to_string());

    // ---------- base ----------
    {
        h.add_meta("subms.workload.feature", "base");
        let s = h
            .stage("base_next", ENTRIES)
            .with_kind(SubMsStageKind::HotPath);
        let mut r = SegmentReader::new(base.as_slice());
        for _ in 0..ENTRIES {
            let t0 = SubMsTimer::tick();
            let _ = r.next_record().expect("read");
            s.record(t0.elapsed_ns());
        }
    }

    // ---------- mmap (reads a real temp file) ----------
    #[cfg(feature = "mmap")]
    {
        use std::fs;
        use subms_segment_reader::MmapSegmentReader;
        h.add_meta("subms.workload.feature", "mmap");

        let dir = unique_temp_dir();
        fs::create_dir_all(&dir)?;
        let path = dir.join("segment.bin");
        fs::write(&path, &base)?;

        let s = h
            .stage("mmap_next", ENTRIES)
            .with_kind(SubMsStageKind::HotPath);
        let mut r = MmapSegmentReader::open(&path).expect("mmap open");
        for _ in 0..ENTRIES {
            let t0 = SubMsTimer::tick();
            let _ = r.next_record().expect("read");
            s.record(t0.elapsed_ns());
        }

        drop(r);
        fs::remove_file(&path).ok();
        fs::remove_dir(&dir).ok();
    }

    // ---------- crc32 ----------
    #[cfg(feature = "crc32")]
    {
        use subms_segment_reader::Crc32SegmentReader;
        use subms_segment_reader::features::crc32::Crc32SegmentWriter;
        h.add_meta("subms.workload.feature", "crc32");

        let mut buf = Vec::with_capacity(ENTRIES * 28);
        {
            let mut w = Crc32SegmentWriter::new(&mut buf);
            for i in 0..ENTRIES {
                w.write(payload(i).as_bytes()).expect("write");
            }
        }
        let s = h
            .stage("crc32_next", ENTRIES)
            .with_kind(SubMsStageKind::HotPath);
        let mut r = Crc32SegmentReader::new(buf.as_slice());
        for _ in 0..ENTRIES {
            let t0 = SubMsTimer::tick();
            let _ = r.next_record().expect("read");
            s.record(t0.elapsed_ns());
        }
    }

    // ---------- xxh3 ----------
    #[cfg(feature = "xxh3")]
    {
        use subms_segment_reader::Xxh3SegmentReader;
        use subms_segment_reader::features::xxh3::Xxh3SegmentWriter;
        h.add_meta("subms.workload.feature", "xxh3");

        let mut buf = Vec::with_capacity(ENTRIES * 32);
        {
            let mut w = Xxh3SegmentWriter::new(&mut buf);
            for i in 0..ENTRIES {
                w.write(payload(i).as_bytes()).expect("write");
            }
        }
        let s = h
            .stage("xxh3_next", ENTRIES)
            .with_kind(SubMsStageKind::HotPath);
        let mut r = Xxh3SegmentReader::new(buf.as_slice());
        for _ in 0..ENTRIES {
            let t0 = SubMsTimer::tick();
            let _ = r.next_record().expect("read");
            s.record(t0.elapsed_ns());
        }
    }

    // ---------- lz4 (compressed blocks) ----------
    #[cfg(feature = "lz4")]
    {
        use subms_segment_reader::{Lz4BlockWriter, Lz4SegmentReader};
        h.add_meta("subms.workload.feature", "lz4");

        // Compressible payloads so the writer takes the LZ4 path and the
        // reader actually decompresses on every block.
        let mut buf = Vec::with_capacity(ENTRIES * 16);
        {
            let mut w = Lz4BlockWriter::new(&mut buf);
            for i in 0..ENTRIES {
                let body = format!("record-{i}-{}", "ab".repeat(24));
                w.write(body.as_bytes()).expect("write");
            }
        }
        let s = h
            .stage("lz4_next", ENTRIES)
            .with_kind(SubMsStageKind::HotPath);
        let mut r = Lz4SegmentReader::new(buf.as_slice());
        for _ in 0..ENTRIES {
            let t0 = SubMsTimer::tick();
            let _ = r.next_record().expect("read");
            s.record(t0.elapsed_ns());
        }
    }

    // ---------- seek-index ----------
    #[cfg(feature = "seek-index")]
    {
        use subms_segment_reader::IndexedSegmentReader;
        h.add_meta("subms.workload.feature", "seek-index");

        let mut r = IndexedSegmentReader::open(base.as_slice()).expect("index open");
        let total = r.total_blocks();

        // Pseudo-random seek targets via a cheap LCG so the seek path hits
        // a spread of index entries + forward scans rather than one offset.
        let mut state: u64 = 0x9e3779b97f4a7c15;
        let mut next_target = || {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (state >> 33) as u32 % total
        };

        let seek_targets: Vec<u32> = (0..ENTRIES).map(|_| next_target()).collect();

        {
            let s = h.stage("seek", ENTRIES).with_kind(SubMsStageKind::HotPath);
            for &target in &seek_targets {
                let t0 = SubMsTimer::tick();
                r.seek_to_block(target).expect("seek");
                s.record(t0.elapsed_ns());
            }
        }
        {
            let s = h
                .stage("next_after_seek", ENTRIES)
                .with_kind(SubMsStageKind::HotPath);
            for &target in &seek_targets {
                r.seek_to_block(target).expect("seek");
                let t0 = SubMsTimer::tick();
                let _ = r.next_record().expect("read");
                s.record(t0.elapsed_ns());
            }
        }
    }

    // ---------- wal-cursor ----------
    #[cfg(feature = "wal-cursor")]
    {
        use subms_segment_reader::WalCursorReader;
        h.add_meta("subms.workload.feature", "wal-cursor");

        // Fully committed watermark: read_committed walks the whole segment.
        let s = h
            .stage("read_committed", ENTRIES)
            .with_kind(SubMsStageKind::HotPath);
        let mut r = WalCursorReader::with_committed(base.as_slice(), base.len());
        for _ in 0..ENTRIES {
            let t0 = SubMsTimer::tick();
            let _ = r.read_committed().expect("read");
            s.record(t0.elapsed_ns());
        }
    }

    let summary = summarize(&h);
    let mut stdout = io::stdout();
    summary_to_json(&summary, &mut stdout)?;
    writeln!(stdout)?;
    Ok(())
}
