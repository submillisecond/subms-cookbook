#![cfg(feature = "mmap")]

use std::io::Write;
use subms_gorilla_block::{TsGorillaBlock, TsMmapBlock};

fn temp_path(tag: &str) -> std::path::PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("subms_gorilla_mmap_{}_{}.blk", std::process::id(), tag));
    p
}

fn write_block(path: &std::path::Path, points: &[(i64, f64)]) {
    let mut b = TsGorillaBlock::new();
    for &(t, v) in points {
        b.append(t, v);
    }
    let mut f = std::fs::File::create(path).unwrap();
    f.write_all(&b.bytes()).unwrap();
    f.sync_all().unwrap();
}

#[test]
fn mmap_decode_matches_in_memory() {
    let pts: Vec<(i64, f64)> = (0..2048)
        .map(|i| (1_000 + i * 1_000, 42.0 + (i as f64 * 0.01).sin()))
        .collect();
    let path = temp_path("roundtrip");
    write_block(&path, &pts);

    let mm = TsMmapBlock::open(&path).unwrap();
    let decoded: Vec<(i64, f64)> = mm.decode().unwrap().iter().map(|p| (p.ts, p.value)).collect();
    assert_eq!(decoded, pts);

    // bytes() is the exact wire form, decodable by the in-memory path too.
    let via_bytes = TsGorillaBlock::from_bytes(mm.bytes()).unwrap();
    assert_eq!(via_bytes.iter().count(), pts.len());

    std::fs::remove_file(&path).ok();
}

#[test]
fn mmap_range_and_stats() {
    let pts: Vec<(i64, f64)> = (0..1024).map(|i| (i, i as f64)).collect();
    let path = temp_path("range");
    write_block(&path, &pts);

    let mm = TsMmapBlock::open(&path).unwrap();
    let in_window = mm.range(100, 200).count();
    assert_eq!(in_window, 101); // inclusive [100, 200]

    let st = mm.stats().unwrap();
    assert_eq!(st.count, 1024);
    assert_eq!(st.ts_min, 0);
    assert_eq!(st.ts_max, 1023);
    assert_eq!(st.value_min, 0.0);
    assert_eq!(st.value_max, 1023.0);

    std::fs::remove_file(&path).ok();
}

#[test]
fn mmap_empty_block() {
    let path = temp_path("empty");
    write_block(&path, &[]);
    let mm = TsMmapBlock::open(&path).unwrap();
    assert_eq!(mm.decode().unwrap().len(), 0);
    assert_eq!(mm.stats().unwrap().count, 0);
    std::fs::remove_file(&path).ok();
}
