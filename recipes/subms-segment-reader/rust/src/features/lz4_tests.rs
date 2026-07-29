use super::*;

#[test]
fn round_trip_compressible_payload() {
    // Highly compressible: the writer should pick the lz4 path.
    let payload = vec![b'a'; 4096];
    let mut buf = Vec::new();
    Lz4BlockWriter::new(&mut buf).write(&payload).unwrap();
    // The on-disk size must be smaller than the raw payload (modulo
    // the 9 byte header).
    assert!(
        buf.len() < payload.len(),
        "lz4 path expected for compressible input"
    );
    let mut r = Lz4SegmentReader::new(buf.as_slice());
    assert_eq!(r.next_record().unwrap().unwrap(), &payload[..]);
    assert!(r.next_record().unwrap().is_none());
}

#[test]
fn round_trip_stored_path_for_incompressible_payload() {
    // Random-ish bytes - lz4 won't help.
    let mut payload = Vec::with_capacity(64);
    for i in 0..64u32 {
        payload.push((i.wrapping_mul(2654435761) >> 24) as u8);
    }
    let mut buf = Vec::new();
    Lz4BlockWriter::new(&mut buf).write(&payload).unwrap();
    // Header is 1 (tag) + 4 (uncompressed_len) + 4 (compressed_len) = 9 bytes.
    assert_eq!(
        buf[0], TAG_STORED,
        "stored path expected when lz4 would inflate"
    );
    assert_eq!(buf.len(), 9 + payload.len());
    let mut r = Lz4SegmentReader::new(buf.as_slice());
    assert_eq!(r.next_record().unwrap().unwrap(), &payload[..]);
}

#[test]
fn empty_segment_yields_none() {
    let mut r = Lz4SegmentReader::new(&[][..]);
    assert!(r.next_record().unwrap().is_none());
}

#[test]
fn unknown_algo_tag_rejected() {
    // Hand-craft a block with tag = 99.
    let mut buf = Vec::new();
    buf.push(99);
    buf.extend_from_slice(&5u32.to_be_bytes());
    buf.extend_from_slice(&5u32.to_be_bytes());
    buf.extend_from_slice(b"hello");
    let mut r = Lz4SegmentReader::new(buf.as_slice());
    assert!(matches!(r.next_record(), Err(Error::DecompressionFailed)));
}

#[test]
fn truncated_header_surfaces_typed_error() {
    let mut buf = Vec::new();
    buf.push(TAG_STORED);
    buf.extend_from_slice(&[0u8; 3]); // 3 of 8 trailing header bytes
    let mut r = Lz4SegmentReader::new(buf.as_slice());
    assert!(matches!(r.next_record(), Err(Error::TruncatedFrame)));
}

#[test]
fn truncated_payload_surfaces_typed_error() {
    let mut buf = Vec::new();
    buf.push(TAG_STORED);
    buf.extend_from_slice(&10u32.to_be_bytes()); // uncompressed=10
    buf.extend_from_slice(&10u32.to_be_bytes()); // compressed=10
    buf.extend_from_slice(b"abc"); // only 3 actual bytes
    let mut r = Lz4SegmentReader::new(buf.as_slice());
    assert!(matches!(r.next_record(), Err(Error::TruncatedFrame)));
}

#[test]
fn corrupted_lz4_payload_returns_decompression_failed() {
    let payload = vec![b'a'; 4096];
    let mut buf = Vec::new();
    Lz4BlockWriter::new(&mut buf).write_lz4(&payload).unwrap();
    // Flip a byte deep into the compressed body to break the LZ4
    // sequence. The 9-byte header is intact.
    let i = buf.len() - 4;
    buf[i] ^= 0xff;
    buf[i - 1] ^= 0xff;
    let mut r = Lz4SegmentReader::new(buf.as_slice());
    // Either decompress fails outright, or it succeeds with the
    // wrong length and the checksum-style length guard catches it.
    match r.next_record() {
        Err(Error::DecompressionFailed) => {}
        Ok(Some(out)) => assert_ne!(out, &payload[..], "corrupted lz4 must not round-trip"),
        other => panic!("unexpected: {other:?}"),
    }
}

#[test]
fn stored_block_size_mismatch_rejected() {
    // Hand-craft a stored block claiming uncompressed=10 but compressed=8.
    let mut buf = Vec::new();
    buf.push(TAG_STORED);
    buf.extend_from_slice(&10u32.to_be_bytes());
    buf.extend_from_slice(&8u32.to_be_bytes());
    buf.extend_from_slice(b"12345678");
    let mut r = Lz4SegmentReader::new(buf.as_slice());
    assert!(matches!(r.next_record(), Err(Error::DecompressionFailed)));
}

#[test]
fn multiple_blocks_round_trip() {
    let mut buf = Vec::new();
    {
        let mut w = Lz4BlockWriter::new(&mut buf);
        w.write(&vec![b'x'; 1024]).unwrap();
        w.write(b"small").unwrap();
        w.write(&vec![b'z'; 2048]).unwrap();
    }
    let mut r = Lz4SegmentReader::new(buf.as_slice());
    assert_eq!(r.next_record().unwrap().unwrap(), &vec![b'x'; 1024][..]);
    assert_eq!(r.next_record().unwrap().unwrap(), b"small");
    assert_eq!(r.next_record().unwrap().unwrap(), &vec![b'z'; 2048][..]);
    assert!(r.next_record().unwrap().is_none());
}
