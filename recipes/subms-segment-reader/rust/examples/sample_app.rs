//! Sample app: a tour of `subms-segment-reader` against an order-event
//! journal, base API first, then each optional reader. Run the base with
//! `cargo run --example sample_app`; add `--all-features` (or a subset like
//! `--features crc32,seek-index`) to light up the feature sections.
//!
//! * base        - replay an order journal, stopping cleanly on a crash-torn tail
//! * mmap        - map a large capture file and page it in lazily
//! * crc32       - detect a bit-flip in an at-rest capture (RocksDB-style trailer)
//! * xxh3        - faster integrity check for a trusted internal pipeline
//! * lz4         - compressed capture blocks that decompress on read
//! * seek-index  - jump to the millionth event without scanning from the head
//! * wal-cursor  - replay only what the writer has fsync'd, never a partial tail

use subms_segment_reader::{SegmentReader, SegmentWriter};

fn main() {
    base_journal_replay();

    #[cfg(feature = "mmap")]
    mmap_capture_replay();

    #[cfg(feature = "crc32")]
    crc32_corruption_guard();

    #[cfg(feature = "xxh3")]
    xxh3_fast_integrity();

    #[cfg(feature = "lz4")]
    lz4_compressed_blocks();

    #[cfg(feature = "seek-index")]
    seek_to_event();

    #[cfg(feature = "wal-cursor")]
    durable_replay();
}

/// Encode a run of order events into a framed segment. Each record is one
/// line of an order journal; the writer just length-prefixes them.
fn build_journal(events: &[&str]) -> Vec<u8> {
    let mut buf = Vec::new();
    let mut w = SegmentWriter::new(&mut buf);
    for e in events {
        w.write(e.as_bytes()).unwrap();
    }
    buf
}

/// Base API: a matching engine replays its order journal on restart. The
/// process died mid-append last time, so the segment ends in a torn frame.
/// Replay walks every intact event and stops cleanly on the torn tail
/// instead of panicking.
fn base_journal_replay() {
    println!("== base: order-journal replay after a crash ==");
    let mut segment = build_journal(&[
        "NEW  AAPL  buy  100 @ 150.25",
        "NEW  MSFT  sell  50 @ 402.10",
        "CXL  AAPL  ord-1",
    ]);
    // The writer was cut mid-frame: a length header claiming 32 bytes, 4 written.
    segment.extend_from_slice(&[0, 0, 0, 32]);
    segment.extend_from_slice(b"NEW ");

    let mut r = SegmentReader::new(segment.as_slice());
    let mut replayed = 0usize;
    loop {
        match r.next_record() {
            Ok(Some(event)) => {
                println!("  replay {}", String::from_utf8_lossy(event));
                replayed += 1;
            }
            Ok(None) => {
                println!("  -> clean EOF");
                break;
            }
            Err(e) => {
                println!("  -> stopped at torn tail: {e}");
                break;
            }
        }
    }
    println!("  replayed {replayed} intact events");
    assert_eq!(replayed, 3, "every event before the torn tail is recovered");
}

/// `mmap` feature: a day's market-data capture is far larger than the
/// working set of any one replay. `MmapSegmentReader::open` maps it in
/// constant time and the OS pages in only the frames actually touched.
#[cfg(feature = "mmap")]
fn mmap_capture_replay() {
    use std::io::Write;
    use subms_segment_reader::MmapSegmentReader;
    println!("\n== mmap: replay a capture file without loading it whole ==");

    let segment = build_journal(&["TICK AAPL 150.25", "TICK AAPL 150.26", "TICK MSFT 402.11"]);
    let mut path = std::env::temp_dir();
    path.push(format!(
        "subms-segment-sample-{}.capture",
        std::process::id()
    ));
    std::fs::File::create(&path)
        .unwrap()
        .write_all(&segment)
        .unwrap();

    let mut r = MmapSegmentReader::open(&path).unwrap();
    println!("  mapped {} bytes, resident set grows on touch", r.len());
    let mut ticks = 0usize;
    while let Some(event) = r.next_record().unwrap() {
        println!("  {}", String::from_utf8_lossy(event));
        ticks += 1;
    }
    std::fs::remove_file(&path).ok();
    assert_eq!(
        ticks, 3,
        "mmap path reads the same frames as the base reader"
    );
}

/// `crc32` feature: an at-rest capture can bit-rot on disk. A CRC32C trailer
/// per block (the LevelDB / RocksDB / Parquet standard) turns silent
/// corruption into a typed `ChecksumMismatch` on read.
#[cfg(feature = "crc32")]
fn crc32_corruption_guard() {
    use subms_segment_reader::Crc32SegmentReader;
    use subms_segment_reader::Error;
    use subms_segment_reader::features::crc32::Crc32SegmentWriter;
    println!("\n== crc32: catch a bit-flip in an at-rest capture ==");

    let mut segment = Vec::new();
    {
        let mut w = Crc32SegmentWriter::new(&mut segment);
        w.write(b"FILL AAPL 100 @ 150.25").unwrap();
        w.write(b"FILL MSFT  50 @ 402.10").unwrap();
    }
    // Flip one bit in the first payload (a cosmic-ray / bad-sector stand-in).
    segment[4] ^= 0x08;

    let mut r = Crc32SegmentReader::new(segment.as_slice());
    match r.next_record() {
        Err(Error::ChecksumMismatch) => println!("  block 0: ChecksumMismatch (corruption caught)"),
        other => panic!("expected ChecksumMismatch, got {other:?}"),
    }
}

/// `xxh3` feature: inside a trusted, single-implementation pipeline where
/// throughput beats interoperability, xxHash3-64 is the cheaper integrity
/// check. It still catches accidental corruption; it is not adversary-safe.
#[cfg(feature = "xxh3")]
fn xxh3_fast_integrity() {
    use subms_segment_reader::Xxh3SegmentReader;
    use subms_segment_reader::features::xxh3::Xxh3SegmentWriter;
    println!("\n== xxh3: faster per-block integrity on a trusted pipeline ==");

    let mut segment = Vec::new();
    {
        let mut w = Xxh3SegmentWriter::new(&mut segment);
        for e in ["FILL AAPL 100 @ 150.25", "FILL AAPL  25 @ 150.26"] {
            w.write(e.as_bytes()).unwrap();
        }
    }
    let mut r = Xxh3SegmentReader::new(segment.as_slice());
    let mut ok = 0usize;
    while let Some(event) = r.next_record().unwrap() {
        println!("  verified {}", String::from_utf8_lossy(event));
        ok += 1;
    }
    assert_eq!(ok, 2, "both blocks pass their xxh3 trailer");
}

/// `lz4` feature: order events are highly repetitive, so the capture
/// compresses well. `Lz4BlockWriter` picks the smaller of stored / lz4 per
/// block; `Lz4SegmentReader` decompresses transparently on read.
#[cfg(feature = "lz4")]
fn lz4_compressed_blocks() {
    use subms_segment_reader::{Lz4BlockWriter, Lz4SegmentReader};
    println!("\n== lz4: compressed capture blocks ==");

    let payload = "NEW AAPL buy 100 @ 150.25\n".repeat(256);
    let mut segment = Vec::new();
    Lz4BlockWriter::new(&mut segment)
        .write(payload.as_bytes())
        .unwrap();
    println!(
        "  {} raw bytes stored in {} on-disk bytes",
        payload.len(),
        segment.len()
    );
    assert!(segment.len() < payload.len(), "repetitive block compressed");

    let mut r = Lz4SegmentReader::new(segment.as_slice());
    let block = r.next_record().unwrap().unwrap();
    assert_eq!(block, payload.as_bytes(), "decompresses byte-for-byte");
    println!("  decompressed back to {} bytes", block.len());
}

/// `seek-index` feature: a replay tool wants event 100 out of a long
/// capture without scanning the first 99. `IndexedSegmentReader` indexes
/// every 64th block at open, then binary-searches + scans a bounded tail.
#[cfg(feature = "seek-index")]
fn seek_to_event() {
    use subms_segment_reader::IndexedSegmentReader;
    println!("\n== seek-index: jump to event 100 without scanning the head ==");

    let events: Vec<String> = (0..200).map(|i| format!("SEQ {i}: TICK AAPL")).collect();
    let refs: Vec<&str> = events.iter().map(String::as_str).collect();
    let segment = build_journal(&refs);

    let mut r = IndexedSegmentReader::open(&segment).unwrap();
    println!(
        "  {} blocks, {} sparse index entries",
        r.total_blocks(),
        r.index_len()
    );
    r.seek_to_block(100).unwrap();
    let at = r.next_record().unwrap().unwrap();
    println!("  seek(100) -> {}", String::from_utf8_lossy(at));
    assert_eq!(at, b"SEQ 100: TICK AAPL", "landed on the requested event");
}

/// `wal-cursor` feature: a live tailer must never replay an event the
/// writer has not yet fsync'd. `read_committed()` stops at the durable
/// watermark; advancing it exposes the newly-committed prefix.
#[cfg(feature = "wal-cursor")]
fn durable_replay() {
    use subms_segment_reader::WalCursorReader;
    println!("\n== wal-cursor: replay only the fsync'd prefix ==");

    let segment = build_journal(&["FILL ord-1", "FILL ord-2", "FILL ord-3"]);
    // Byte offset of the tail of the first block: 4-byte header + 10-byte payload.
    let after_first = 4 + "FILL ord-1".len();

    let mut r = WalCursorReader::new(&segment);
    assert!(
        r.read_committed().unwrap().is_none(),
        "watermark at 0 -> nothing durable"
    );

    r.set_committed(after_first);
    let event = r.read_committed().unwrap().unwrap();
    println!("  after first fsync -> {}", String::from_utf8_lossy(event));
    assert!(
        r.read_committed().unwrap().is_none(),
        "second block not yet committed"
    );

    r.set_committed(segment.len());
    let mut tail = 0usize;
    while let Some(event) = r.read_committed().unwrap() {
        println!("  now durable -> {}", String::from_utf8_lossy(event));
        tail += 1;
    }
    assert_eq!(tail, 2, "the remaining committed events replay");
}
