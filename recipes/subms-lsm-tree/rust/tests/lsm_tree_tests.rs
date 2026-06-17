use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use subms_lsm_tree::LsmTree;

static SEQ: AtomicU64 = AtomicU64::new(0);

fn fresh_dir(label: &str) -> PathBuf {
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = env::temp_dir().join(format!("lsm-test-{}-{}-{}", label, std::process::id(), n));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn round_trip() {
    let dir = fresh_dir("round_trip");
    let mut lsm = LsmTree::open(&dir, 1 << 20).unwrap();
    lsm.put("k1", b"v1").unwrap();
    lsm.put("k2", b"v2").unwrap();
    assert_eq!(lsm.get("k1").unwrap().as_deref(), Some(&b"v1"[..]));
    assert_eq!(lsm.get("k2").unwrap().as_deref(), Some(&b"v2"[..]));
    assert!(lsm.get("nope").unwrap().is_none());
    lsm.flush().unwrap();
    assert_eq!(lsm.get("k1").unwrap().as_deref(), Some(&b"v1"[..]));
}

#[test]
fn tombstone_shadows_older_value() {
    let dir = fresh_dir("tomb");
    let mut lsm = LsmTree::open(&dir, 1 << 20).unwrap();
    lsm.put("k", b"v").unwrap();
    lsm.flush().unwrap();
    lsm.delete("k").unwrap();
    assert!(lsm.get("k").unwrap().is_none(), "memtable tombstone");
    lsm.flush().unwrap();
    assert!(lsm.get("k").unwrap().is_none(), "flushed tombstone");
}

#[test]
fn newer_sstable_shadows_older() {
    let dir = fresh_dir("newer");
    let mut lsm = LsmTree::open(&dir, 1 << 20).unwrap();
    lsm.put("k", b"old").unwrap();
    lsm.flush().unwrap();
    lsm.put("k", b"new").unwrap();
    lsm.flush().unwrap();
    assert_eq!(lsm.get("k").unwrap().as_deref(), Some(&b"new"[..]));
}

#[test]
fn reopen_sees_prior_flushes() {
    let dir = fresh_dir("reopen");
    {
        let mut lsm = LsmTree::open(&dir, 1 << 20).unwrap();
        lsm.put("durable", b"yes").unwrap();
        lsm.flush().unwrap();
    }
    let lsm = LsmTree::open(&dir, 1 << 20).unwrap();
    assert_eq!(lsm.get("durable").unwrap().as_deref(), Some(&b"yes"[..]));
}

#[test]
fn flush_triggered_by_threshold() {
    let dir = fresh_dir("threshold");
    let mut lsm = LsmTree::open(&dir, 32).unwrap();
    for i in 0..20 {
        lsm.put(&format!("key{i}"), format!("v{i}").as_bytes())
            .unwrap();
    }
    assert!(
        lsm.sstable_count() >= 1,
        "threshold-driven flush rolled at least one sstable"
    );
}

#[test]
fn bloom_off_still_finds_keys() {
    let dir = fresh_dir("bloom_off");
    let mut lsm =
        subms_lsm_tree::LsmTree::open_with(&dir, 256, subms_lsm_tree::BloomMode::Off).unwrap();
    for i in 0..100 {
        lsm.put(&format!("k{i}"), format!("v{i}").as_bytes())
            .unwrap();
    }
    lsm.flush().unwrap();
    for i in 0..100 {
        assert_eq!(
            lsm.get(&format!("k{i}")).unwrap().as_deref(),
            Some(format!("v{i}").as_bytes()),
        );
    }
    assert!(lsm.get("missing").unwrap().is_none());
}

#[test]
fn bloom_does_not_lose_present_keys() {
    let dir = fresh_dir("bloom");
    let mut lsm = LsmTree::open(&dir, 1024).unwrap();
    for i in 0..1000 {
        lsm.put(&format!("k{i}"), format!("v{i}").as_bytes())
            .unwrap();
    }
    lsm.flush().unwrap();
    for i in 0..1000 {
        let got = lsm.get(&format!("k{i}")).unwrap();
        assert_eq!(got.as_deref(), Some(format!("v{i}").as_bytes()));
    }
    assert!(lsm.get("absolutely-not-here").unwrap().is_none());
}

#[test]
fn get_on_empty_tree_returns_none() {
    let dir = fresh_dir("empty");
    let lsm = LsmTree::open(&dir, 1024).unwrap();
    assert!(
        lsm.get("anything").unwrap().is_none(),
        "fresh LSM has no keys"
    );
}

#[test]
fn overwrite_preserves_latest_value_across_flushes() {
    let dir = fresh_dir("overwrite");
    let mut lsm = LsmTree::open(&dir, 1024).unwrap();
    lsm.put("k", b"v1").unwrap();
    lsm.flush().unwrap();
    lsm.put("k", b"v2").unwrap();
    lsm.flush().unwrap();
    assert_eq!(
        lsm.get("k").unwrap().as_deref(),
        Some(&b"v2"[..]),
        "later put must shadow earlier put across flushes"
    );
}
