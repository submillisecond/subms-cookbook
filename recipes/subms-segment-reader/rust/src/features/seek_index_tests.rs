use super::*;
use crate::SegmentWriter;

fn build_n(n: u32) -> Vec<u8> {
    let mut buf = Vec::new();
    let mut w = SegmentWriter::new(&mut buf);
    for i in 0..n {
        w.write(format!("rec-{i}").as_bytes()).unwrap();
    }
    buf
}

#[test]
fn index_stride_matches_block_layout() {
    let buf = build_n(200);
    let r = IndexedSegmentReader::open(&buf).unwrap();
    assert_eq!(r.total_blocks(), 200);
    // 200 blocks at stride 64 -> indexes at 0, 64, 128, 192 = 4 entries.
    assert_eq!(r.index_len(), 4);
}

#[test]
fn seek_to_first_block() {
    let buf = build_n(200);
    let mut r = IndexedSegmentReader::open(&buf).unwrap();
    r.seek_to_block(0).unwrap();
    assert_eq!(r.next_record().unwrap().unwrap(), b"rec-0");
}

#[test]
fn seek_to_indexed_block() {
    let buf = build_n(200);
    let mut r = IndexedSegmentReader::open(&buf).unwrap();
    r.seek_to_block(128).unwrap();
    assert_eq!(r.next_record().unwrap().unwrap(), b"rec-128");
}

#[test]
fn seek_to_unindexed_block_scans_forward() {
    let buf = build_n(200);
    let mut r = IndexedSegmentReader::open(&buf).unwrap();
    // Block 100: nearest entry below is block 64 (entry idx 1).
    r.seek_to_block(100).unwrap();
    assert_eq!(r.next_record().unwrap().unwrap(), b"rec-100");
}

#[test]
fn seek_past_end_yields_none() {
    let buf = build_n(50);
    let mut r = IndexedSegmentReader::open(&buf).unwrap();
    r.seek_to_block(9999).unwrap();
    assert!(r.next_record().unwrap().is_none());
}

#[test]
fn open_on_corrupted_tail_errors() {
    let mut buf = build_n(10);
    buf.extend_from_slice(&[0, 0, 0]); // truncated header
    assert!(matches!(
        IndexedSegmentReader::open(&buf),
        Err(Error::TruncatedFrame)
    ));
}

#[test]
fn sequential_next_record_walks_every_block() {
    let buf = build_n(40);
    let mut r = IndexedSegmentReader::open(&buf).unwrap();
    for i in 0..40u32 {
        let got = r.next_record().unwrap().unwrap();
        assert_eq!(got, format!("rec-{i}").as_bytes());
    }
    assert!(r.next_record().unwrap().is_none());
}

#[test]
fn empty_segment_index_is_empty() {
    let buf: Vec<u8> = Vec::new();
    let r = IndexedSegmentReader::open(&buf).unwrap();
    assert_eq!(r.total_blocks(), 0);
    assert_eq!(r.index_len(), 0);
}

#[test]
fn open_truncated_payload_errors() {
    let mut buf = build_n(3);
    // Header claims 20 payload bytes; only 5 follow.
    buf.extend_from_slice(&[0, 0, 0, 20]);
    buf.extend_from_slice(b"short");
    assert!(matches!(
        IndexedSegmentReader::open(&buf),
        Err(Error::TruncatedFrame)
    ));
}

#[test]
fn seek_forward_scan_across_many_blocks() {
    let buf = build_n(200);
    let mut r = IndexedSegmentReader::open(&buf).unwrap();
    // Every non-multiple-of-64 target forces the linear forward scan from
    // the nearest index entry, exercising the scan loop body.
    for target in [1u32, 63, 65, 100, 191, 199] {
        r.seek_to_block(target).unwrap();
        assert_eq!(
            r.next_record().unwrap().unwrap(),
            format!("rec-{target}").as_bytes()
        );
    }
}
