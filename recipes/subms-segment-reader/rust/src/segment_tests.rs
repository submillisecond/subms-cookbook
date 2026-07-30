use super::*;

fn build(records: &[&[u8]]) -> Vec<u8> {
    let mut buf = Vec::new();
    let mut w = SegmentWriter::new(&mut buf);
    for r in records {
        w.write(r).unwrap();
    }
    buf
}

#[test]
fn round_trip_single_record() {
    let buf = build(&[b"hello"]);
    let mut r = SegmentReader::new(buf.as_slice());
    assert_eq!(r.next_record().unwrap().unwrap(), b"hello");
    assert!(r.next_record().unwrap().is_none());
}

#[test]
fn round_trip_multiple_records() {
    let buf = build(&[b"alice", b"bob", b"carol"]);
    let mut r = SegmentReader::new(buf.as_slice());
    assert_eq!(r.next_record().unwrap().unwrap(), b"alice");
    assert_eq!(r.next_record().unwrap().unwrap(), b"bob");
    assert_eq!(r.next_record().unwrap().unwrap(), b"carol");
    assert!(r.next_record().unwrap().is_none());
}

#[test]
fn empty_segment_yields_none() {
    let buf: Vec<u8> = Vec::new();
    let mut r = SegmentReader::new(buf.as_slice());
    assert!(r.next_record().unwrap().is_none());
}

#[test]
fn truncated_header_surfaces_typed_error() {
    let mut buf = build(&[b"first"]);
    buf.extend_from_slice(&[0x00, 0x00]); // 2 bytes of a 4-byte header
    let mut r = SegmentReader::new(buf.as_slice());
    assert_eq!(r.next_record().unwrap().unwrap(), b"first");
    match r.next_record() {
        Err(Error::TruncatedFrame) => {}
        other => panic!("expected TruncatedFrame, got {other:?}"),
    }
}

#[test]
fn truncated_payload_surfaces_typed_error() {
    let mut buf = Vec::new();
    let mut w = SegmentWriter::new(&mut buf);
    w.write(b"first").unwrap();
    // Hand-craft a header claiming 10 bytes follow but write 3.
    buf.extend_from_slice(&[0x00, 0x00, 0x00, 0x0a]);
    buf.extend_from_slice(b"abc");
    let mut r = SegmentReader::new(buf.as_slice());
    assert_eq!(r.next_record().unwrap().unwrap(), b"first");
    match r.next_record() {
        Err(Error::TruncatedFrame) => {}
        other => panic!("expected TruncatedFrame, got {other:?}"),
    }
}

#[test]
fn zero_length_record() {
    let buf = build(&[&[], b"after-empty"]);
    let mut r = SegmentReader::new(buf.as_slice());
    assert_eq!(r.next_record().unwrap().unwrap(), b"");
    assert_eq!(r.next_record().unwrap().unwrap(), b"after-empty");
}

#[test]
fn large_record_roundtrip() {
    let big = vec![0xabu8; 8192];
    let buf = build(&[&big[..]]);
    let mut r = SegmentReader::new(buf.as_slice());
    assert_eq!(r.next_record().unwrap().unwrap(), &big[..]);
}

#[test]
fn many_records_in_a_row() {
    let mut records: Vec<Vec<u8>> = Vec::new();
    for i in 0..1000u32 {
        records.push(format!("rec-{i}").into_bytes());
    }
    let refs: Vec<&[u8]> = records.iter().map(|v| v.as_slice()).collect();
    let buf = build(&refs);
    let mut r = SegmentReader::new(buf.as_slice());
    for expected in &records {
        assert_eq!(r.next_record().unwrap().unwrap(), expected.as_slice());
    }
    assert!(r.next_record().unwrap().is_none());
}

#[test]
fn one_byte_after_clean_eof_is_truncated_header() {
    let mut buf = build(&[b"a"]);
    buf.push(0xff);
    let mut r = SegmentReader::new(buf.as_slice());
    assert_eq!(r.next_record().unwrap().unwrap(), b"a");
    assert!(matches!(r.next_record(), Err(Error::TruncatedFrame)));
}

#[test]
fn error_display_is_descriptive() {
    let err = Error::TruncatedFrame;
    let msg = format!("{err}");
    assert!(msg.contains("truncated") || msg.contains("frame"));
}

#[test]
fn every_error_variant_has_a_distinct_message() {
    let io: Error = io::Error::other("boom").into();
    assert!(format!("{io}").contains("io error"));
    assert!(format!("{}", Error::ChecksumMismatch).contains("checksum"));
    assert!(format!("{}", Error::DecompressionFailed).contains("decompression"));
    // std::error::Error is implemented; exercise it through the trait object.
    let dyn_err: &dyn std::error::Error = &Error::TruncatedFrame;
    assert!(dyn_err.source().is_none());
}

// Yields a well-formed 4-byte length header on the first read, then fails the
// payload read with a non-EOF IO error - drives the `Error::Io` arm of the
// payload map_err that a plain slice reader (UnexpectedEof only) never reaches.
struct HeaderThenIoError {
    sent_header: bool,
}

impl Read for HeaderThenIoError {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if !self.sent_header {
            self.sent_header = true;
            buf[..4].copy_from_slice(&[0, 0, 0, 4]);
            return Ok(4);
        }
        Err(io::Error::new(io::ErrorKind::BrokenPipe, "device gone"))
    }
}

#[test]
fn non_eof_payload_error_surfaces_as_io() {
    let mut r = SegmentReader::new(HeaderThenIoError { sent_header: false });
    match r.next_record() {
        Err(Error::Io(e)) => assert_eq!(e.kind(), io::ErrorKind::BrokenPipe),
        other => panic!("expected Io(BrokenPipe), got {other:?}"),
    }
}

#[test]
fn writer_flush_is_reachable() {
    let mut buf = Vec::new();
    {
        let mut w = SegmentWriter::new(&mut buf);
        w.write(b"x").unwrap();
        w.flush().unwrap();
    }
    let mut r = SegmentReader::new(buf.as_slice());
    assert_eq!(r.next_record().unwrap().unwrap(), b"x");
}
