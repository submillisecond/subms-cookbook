use super::*;

fn build(records: &[&[u8]]) -> Vec<u8> {
    let mut buf = Vec::new();
    let mut w = Xxh3SegmentWriter::new(&mut buf);
    for r in records {
        w.write(r).unwrap();
    }
    buf
}

#[test]
fn round_trip_with_hash() {
    let buf = build(&[b"alpha", b"beta", b"gamma"]);
    let mut r = Xxh3SegmentReader::new(buf.as_slice());
    assert_eq!(r.next_record().unwrap().unwrap(), b"alpha");
    assert_eq!(r.next_record().unwrap().unwrap(), b"beta");
    assert_eq!(r.next_record().unwrap().unwrap(), b"gamma");
    assert!(r.next_record().unwrap().is_none());
}

#[test]
fn empty_segment_yields_none() {
    let mut r = Xxh3SegmentReader::new(&[][..]);
    assert!(r.next_record().unwrap().is_none());
}

#[test]
fn corrupted_payload_detected() {
    let mut buf = build(&[b"hello"]);
    buf[4] ^= 0x40;
    let mut r = Xxh3SegmentReader::new(buf.as_slice());
    assert!(matches!(r.next_record(), Err(Error::ChecksumMismatch)));
}

#[test]
fn corrupted_trailer_detected() {
    let mut buf = build(&[b"hello"]);
    let last = buf.len() - 1;
    buf[last] ^= 0xff;
    let mut r = Xxh3SegmentReader::new(buf.as_slice());
    assert!(matches!(r.next_record(), Err(Error::ChecksumMismatch)));
}

#[test]
fn truncated_trailer_surfaces_typed_error() {
    let mut buf = build(&[b"hello"]);
    buf.truncate(buf.len() - 4); // trailer is 8 bytes; chop half
    let mut r = Xxh3SegmentReader::new(buf.as_slice());
    assert!(matches!(r.next_record(), Err(Error::TruncatedFrame)));
}

#[test]
fn truncated_payload_surfaces_typed_error() {
    let mut buf = Vec::new();
    buf.extend_from_slice(&[0, 0, 0, 10]);
    buf.extend_from_slice(b"abc");
    let mut r = Xxh3SegmentReader::new(buf.as_slice());
    assert!(matches!(r.next_record(), Err(Error::TruncatedFrame)));
}

#[test]
fn zero_length_record_round_trips() {
    let buf = build(&[&[], b"after-empty"]);
    let mut r = Xxh3SegmentReader::new(buf.as_slice());
    assert_eq!(r.next_record().unwrap().unwrap(), b"");
    assert_eq!(r.next_record().unwrap().unwrap(), b"after-empty");
}
