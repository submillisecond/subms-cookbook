use super::*;

fn repetitive(n: usize) -> Vec<u8> {
    let pattern = b"the-quick-brown-fox-jumps-over-the-lazy-dog-";
    let mut out = Vec::with_capacity(n);
    while out.len() < n {
        out.extend_from_slice(pattern);
    }
    out.truncate(n);
    out
}

#[test]
fn round_trip_compressible() {
    let c = Lz4BlockCompressor::new();
    let input = repetitive(4096);
    let enc = c.compress(&input);
    assert!(
        enc.len() < input.len(),
        "compressible payload should shrink"
    );
    let dec = c.decompress(&enc).unwrap();
    assert_eq!(dec, input);
}

#[test]
fn round_trip_empty() {
    let c = Lz4BlockCompressor::new();
    let enc = c.compress(&[]);
    let dec = c.decompress(&enc).unwrap();
    assert!(dec.is_empty());
}

#[test]
fn incompressible_falls_back_to_stored() {
    let c = Lz4BlockCompressor::new();
    // Pseudorandom-but-deterministic bytes (xorshift) - LZ4 won't shrink these.
    let mut s = 0x9E37_79B9_u32;
    let mut input = Vec::with_capacity(2048);
    for _ in 0..2048 {
        s ^= s << 13;
        s ^= s >> 17;
        s ^= s << 5;
        input.push(s as u8);
    }
    let enc = c.compress(&input);
    // Algo byte should indicate stored path - second byte of the header.
    assert_eq!(enc[1], ALGO_STORED, "incompressible falls back to stored");
    let dec = c.decompress(&enc).unwrap();
    assert_eq!(dec, input);
}

#[test]
fn bad_marker_errors() {
    let c = Lz4BlockCompressor::new();
    let mut enc = c.compress(b"hello world hello world");
    enc[0] = 0xff;
    let err = c.decompress(&enc).unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::InvalidData);
}

#[test]
fn too_short_buffer_errors() {
    let c = Lz4BlockCompressor::new();
    let err = c.decompress(&[0u8; 3]).unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::InvalidData);
}

#[test]
fn unknown_algo_byte_errors() {
    let c = Lz4BlockCompressor::new();
    let mut enc = c.compress(b"abcdef");
    enc[1] = 0x7f;
    assert!(c.decompress(&enc).is_err());
}
