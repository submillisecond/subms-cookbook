use super::*;

fn build(records: &[&[u8]]) -> Vec<u8> {
    let mut buf = Vec::new();
    let mut w = Crc32SegmentWriter::new(&mut buf);
    for r in records {
        w.write(r).unwrap();
    }
    buf
}

#[test]
fn round_trip_with_checksum() {
    let buf = build(&[b"alice", b"bob", b"carol"]);
    let mut r = Crc32SegmentReader::new(buf.as_slice());
    assert_eq!(r.next_record().unwrap().unwrap(), b"alice");
    assert_eq!(r.next_record().unwrap().unwrap(), b"bob");
    assert_eq!(r.next_record().unwrap().unwrap(), b"carol");
    assert!(r.next_record().unwrap().is_none());
}

#[test]
fn empty_segment_yields_none() {
    let mut r = Crc32SegmentReader::new(&[][..]);
    assert!(r.next_record().unwrap().is_none());
}

#[test]
fn corrupted_payload_detected() {
    let mut buf = build(&[b"hello"]);
    // Header (4) + payload (5). Flip one bit in the payload.
    buf[4] ^= 0x80;
    let mut r = Crc32SegmentReader::new(buf.as_slice());
    assert!(matches!(r.next_record(), Err(Error::ChecksumMismatch)));
}

#[test]
fn corrupted_trailer_detected() {
    let mut buf = build(&[b"hello"]);
    // Trailer sits at the last 4 bytes; corrupt one.
    let last = buf.len() - 1;
    buf[last] ^= 0xff;
    let mut r = Crc32SegmentReader::new(buf.as_slice());
    assert!(matches!(r.next_record(), Err(Error::ChecksumMismatch)));
}

#[test]
fn truncated_trailer_surfaces_typed_error() {
    let mut buf = build(&[b"hello"]);
    buf.truncate(buf.len() - 2); // chop 2 bytes of the trailer
    let mut r = Crc32SegmentReader::new(buf.as_slice());
    assert!(matches!(r.next_record(), Err(Error::TruncatedFrame)));
}

#[test]
fn truncated_payload_surfaces_typed_error() {
    let mut buf = Vec::new();
    buf.extend_from_slice(&[0, 0, 0, 10]); // claim 10 bytes
    buf.extend_from_slice(b"abc");
    let mut r = Crc32SegmentReader::new(buf.as_slice());
    assert!(matches!(r.next_record(), Err(Error::TruncatedFrame)));
}

#[test]
fn zero_length_record_round_trips() {
    let buf = build(&[&[], b"after-empty"]);
    let mut r = Crc32SegmentReader::new(buf.as_slice());
    assert_eq!(r.next_record().unwrap().unwrap(), b"");
    assert_eq!(r.next_record().unwrap().unwrap(), b"after-empty");
}
