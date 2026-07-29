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
    let c = ZstdBlockCompressor::new();
    let input = repetitive(4096);
    let enc = c.compress(&input).unwrap();
    assert!(
        enc.len() < input.len(),
        "compressible payload should shrink"
    );
    let dec = c.decompress(&enc).unwrap();
    assert_eq!(dec, input);
}

#[test]
fn round_trip_empty() {
    let c = ZstdBlockCompressor::new();
    let enc = c.compress(&[]).unwrap();
    let dec = c.decompress(&enc).unwrap();
    assert!(dec.is_empty());
}

#[test]
fn level_is_clamped_to_valid_range() {
    let c_low = ZstdBlockCompressor::with_level(-99);
    assert_eq!(c_low.level(), MIN_LEVEL);
    let c_high = ZstdBlockCompressor::with_level(99);
    assert_eq!(c_high.level(), MAX_LEVEL);
    let c_mid = ZstdBlockCompressor::with_level(10);
    assert_eq!(c_mid.level(), 10);
}

#[test]
fn incompressible_falls_back_to_stored() {
    let c = ZstdBlockCompressor::new();
    let mut s = 0xDEAD_BEEF_u32;
    let mut input = Vec::with_capacity(64);
    for _ in 0..64 {
        s ^= s << 13;
        s ^= s >> 17;
        s ^= s << 5;
        input.push(s as u8);
    }
    let enc = c.compress(&input).unwrap();
    // Tiny + random: zstd cannot shrink. Verify algo byte path.
    assert_eq!(enc[1], ALGO_STORED, "incompressible takes the stored path");
    let dec = c.decompress(&enc).unwrap();
    assert_eq!(dec, input);
}

#[test]
fn bad_marker_errors() {
    let c = ZstdBlockCompressor::new();
    let mut enc = c.compress(b"hello world hello world hello world").unwrap();
    enc[0] = 0x00;
    let err = c.decompress(&enc).unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::InvalidData);
}

#[test]
fn too_short_buffer_errors() {
    let c = ZstdBlockCompressor::new();
    let err = c.decompress(&[0u8; 3]).unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::InvalidData);
}

#[test]
fn unknown_algo_byte_errors() {
    let c = ZstdBlockCompressor::new();
    let mut enc = c.compress(b"abcdefghij").unwrap();
    enc[1] = 0x77;
    assert!(c.decompress(&enc).is_err());
}
