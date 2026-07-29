//! Pins the behaviour each section of the `sample_app` example demonstrates:
//! journal replay stops cleanly on a torn tail, and every optional reader
//! recovers the same frames it was handed.

use super::*;

fn build_journal(events: &[&str]) -> Vec<u8> {
    let mut buf = Vec::new();
    let mut w = SegmentWriter::new(&mut buf);
    for e in events {
        w.write(e.as_bytes()).unwrap();
    }
    buf
}

#[test]
fn journal_replay_stops_on_torn_tail() {
    let mut segment = build_journal(&[
        "NEW  AAPL  buy  100 @ 150.25",
        "NEW  MSFT  sell  50 @ 402.10",
        "CXL  AAPL  ord-1",
    ]);
    segment.extend_from_slice(&[0, 0, 0, 32]);
    segment.extend_from_slice(b"NEW ");

    let mut r = SegmentReader::new(segment.as_slice());
    let mut replayed = 0usize;
    let outcome = loop {
        match r.next_record() {
            Ok(Some(_)) => replayed += 1,
            Ok(None) => break "clean",
            Err(Error::TruncatedFrame) => break "torn",
            Err(e) => panic!("unexpected error: {e:?}"),
        }
    };
    assert_eq!(replayed, 3, "every intact event recovered before the tail");
    assert_eq!(outcome, "torn", "the crash tail surfaces as TruncatedFrame");
}

#[cfg(feature = "mmap")]
#[test]
fn mmap_reads_same_frames() {
    use crate::MmapSegmentReader;
    use std::io::Write;

    let segment = build_journal(&["TICK AAPL 150.25", "TICK MSFT 402.11"]);
    let mut path = std::env::temp_dir();
    path.push(format!(
        "subms-segment-sample-test-{}-{}.capture",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::File::create(&path)
        .unwrap()
        .write_all(&segment)
        .unwrap();

    let mut r = MmapSegmentReader::open(&path).unwrap();
    assert_eq!(r.next_record().unwrap().unwrap(), b"TICK AAPL 150.25");
    assert_eq!(r.next_record().unwrap().unwrap(), b"TICK MSFT 402.11");
    assert!(r.next_record().unwrap().is_none());
    std::fs::remove_file(&path).ok();
}

#[cfg(feature = "crc32")]
#[test]
fn crc32_catches_bit_flip() {
    use crate::Crc32SegmentReader;
    use crate::features::crc32::Crc32SegmentWriter;

    let mut segment = Vec::new();
    {
        let mut w = Crc32SegmentWriter::new(&mut segment);
        w.write(b"FILL AAPL 100 @ 150.25").unwrap();
    }
    segment[4] ^= 0x08;
    let mut r = Crc32SegmentReader::new(segment.as_slice());
    assert!(matches!(r.next_record(), Err(Error::ChecksumMismatch)));
}

#[cfg(feature = "xxh3")]
#[test]
fn xxh3_round_trips_clean_blocks() {
    use crate::Xxh3SegmentReader;
    use crate::features::xxh3::Xxh3SegmentWriter;

    let mut segment = Vec::new();
    {
        let mut w = Xxh3SegmentWriter::new(&mut segment);
        w.write(b"FILL AAPL 100 @ 150.25").unwrap();
        w.write(b"FILL AAPL  25 @ 150.26").unwrap();
    }
    let mut r = Xxh3SegmentReader::new(segment.as_slice());
    assert_eq!(r.next_record().unwrap().unwrap(), b"FILL AAPL 100 @ 150.25");
    assert_eq!(r.next_record().unwrap().unwrap(), b"FILL AAPL  25 @ 150.26");
    assert!(r.next_record().unwrap().is_none());
}

#[cfg(feature = "lz4")]
#[test]
fn lz4_compresses_and_round_trips() {
    use crate::{Lz4BlockWriter, Lz4SegmentReader};

    let payload = "NEW AAPL buy 100 @ 150.25\n".repeat(256);
    let mut segment = Vec::new();
    Lz4BlockWriter::new(&mut segment)
        .write(payload.as_bytes())
        .unwrap();
    assert!(segment.len() < payload.len(), "repetitive block compressed");

    let mut r = Lz4SegmentReader::new(segment.as_slice());
    assert_eq!(r.next_record().unwrap().unwrap(), payload.as_bytes());
}

#[cfg(feature = "seek-index")]
#[test]
fn seek_index_lands_on_requested_event() {
    use crate::IndexedSegmentReader;

    let events: Vec<String> = (0..200).map(|i| format!("SEQ {i}: TICK AAPL")).collect();
    let refs: Vec<&str> = events.iter().map(String::as_str).collect();
    let segment = build_journal(&refs);

    let mut r = IndexedSegmentReader::open(&segment).unwrap();
    assert_eq!(r.total_blocks(), 200);
    r.seek_to_block(100).unwrap();
    assert_eq!(r.next_record().unwrap().unwrap(), b"SEQ 100: TICK AAPL");
}

#[cfg(feature = "wal-cursor")]
#[test]
fn wal_cursor_gates_on_watermark() {
    use crate::WalCursorReader;

    let segment = build_journal(&["FILL ord-1", "FILL ord-2", "FILL ord-3"]);
    let after_first = 4 + "FILL ord-1".len();

    let mut r = WalCursorReader::new(&segment);
    assert!(r.read_committed().unwrap().is_none(), "nothing durable yet");

    r.set_committed(after_first);
    assert_eq!(r.read_committed().unwrap().unwrap(), b"FILL ord-1");
    assert!(
        r.read_committed().unwrap().is_none(),
        "second block not committed"
    );

    r.set_committed(segment.len());
    assert_eq!(r.read_committed().unwrap().unwrap(), b"FILL ord-2");
    assert_eq!(r.read_committed().unwrap().unwrap(), b"FILL ord-3");
    assert!(r.read_committed().unwrap().is_none());
}
