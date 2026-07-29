use super::*;
use crate::SegmentWriter;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

fn temp_path(label: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    p.push(format!(
        "subms-segment-mmap-{label}-{stamp}-{}.bin",
        std::process::id()
    ));
    p
}

fn write_segment(path: &Path, records: &[&[u8]]) {
    let mut f = fs::File::create(path).unwrap();
    let mut w = SegmentWriter::new(&mut f);
    for r in records {
        w.write(r).unwrap();
    }
    f.flush().unwrap();
}

#[test]
fn round_trip_via_mmap() {
    let path = temp_path("round-trip");
    write_segment(&path, &[b"alice", b"bob", b"carol"]);
    let mut r = MmapSegmentReader::open(&path).unwrap();
    assert_eq!(r.next_record().unwrap().unwrap(), b"alice");
    assert_eq!(r.next_record().unwrap().unwrap(), b"bob");
    assert_eq!(r.next_record().unwrap().unwrap(), b"carol");
    assert!(r.next_record().unwrap().is_none());
    fs::remove_file(&path).ok();
}

#[test]
fn empty_file_yields_none() {
    let path = temp_path("empty");
    fs::write(&path, b"").unwrap();
    let mut r = MmapSegmentReader::open(&path).unwrap();
    assert_eq!(r.len(), 0);
    assert!(r.is_empty());
    assert!(r.next_record().unwrap().is_none());
    fs::remove_file(&path).ok();
}

#[test]
fn missing_file_returns_io_error() {
    let path = temp_path("missing");
    match MmapSegmentReader::open(&path) {
        Err(e) => assert_eq!(e.kind(), io::ErrorKind::NotFound),
        Ok(_) => panic!("expected NotFound, got Ok"),
    }
}

#[test]
fn truncated_header_surfaces_typed_error() {
    let path = temp_path("trunc-header");
    let mut bytes = Vec::new();
    {
        let mut w = SegmentWriter::new(&mut bytes);
        w.write(b"first").unwrap();
    }
    bytes.extend_from_slice(&[0x00, 0x00]); // 2 bytes of a 4-byte header
    fs::write(&path, &bytes).unwrap();
    let mut r = MmapSegmentReader::open(&path).unwrap();
    assert_eq!(r.next_record().unwrap().unwrap(), b"first");
    assert!(matches!(r.next_record(), Err(Error::TruncatedFrame)));
    fs::remove_file(&path).ok();
}

#[test]
fn truncated_payload_surfaces_typed_error() {
    let path = temp_path("trunc-payload");
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&[0x00, 0x00, 0x00, 0x0a]); // claim 10 bytes
    bytes.extend_from_slice(b"abc"); // only 3 actually
    fs::write(&path, &bytes).unwrap();
    let mut r = MmapSegmentReader::open(&path).unwrap();
    assert!(matches!(r.next_record(), Err(Error::TruncatedFrame)));
    fs::remove_file(&path).ok();
}

#[test]
fn rewind_replays_from_start() {
    let path = temp_path("rewind");
    write_segment(&path, &[b"one", b"two"]);
    let mut r = MmapSegmentReader::open(&path).unwrap();
    assert_eq!(r.next_record().unwrap().unwrap(), b"one");
    r.rewind();
    assert_eq!(r.next_record().unwrap().unwrap(), b"one");
    assert_eq!(r.next_record().unwrap().unwrap(), b"two");
    fs::remove_file(&path).ok();
}

#[test]
fn as_bytes_returns_full_mapping() {
    let path = temp_path("as-bytes");
    write_segment(&path, &[b"xyz"]);
    let r = MmapSegmentReader::open(&path).unwrap();
    // 4 byte header + 3 byte payload.
    assert_eq!(r.as_bytes().len(), 7);
    assert_eq!(r.len(), 7);
    assert!(!r.is_empty());
    fs::remove_file(&path).ok();
}
