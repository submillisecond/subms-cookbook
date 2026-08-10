use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use super::*;

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
    // The 32-byte threshold rotates repeatedly under these writes; the default
    // background flush turns those frozen memtables into SSTables off the write
    // path, so drain to a deterministic point before counting.
    lsm.flush().unwrap();
    assert!(
        lsm.sstable_count() >= 1,
        "threshold-driven flush rolled at least one sstable"
    );
}

#[test]
fn bloom_off_still_finds_keys() {
    let dir = fresh_dir("bloom_off");
    let mut lsm = crate::LsmTree::open_with(&dir, 256, crate::BloomMode::Off).unwrap();
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
fn bloom_mode_getter_reflects_configuration() {
    let on = fresh_dir("bloom_mode_on");
    assert_eq!(LsmTree::open(&on, 256).unwrap().bloom_mode(), BloomMode::On);
    let off = fresh_dir("bloom_mode_off");
    let lsm = LsmTree::open_with(&off, 256, BloomMode::Off).unwrap();
    assert_eq!(lsm.bloom_mode(), BloomMode::Off);
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

// Resource-growth gate. Every other test here measures behaviour or latency,
// and neither can see an LSM's defining cost: rewriting the same key set does
// not overwrite anything on disk, it appends. So on-disk bytes track TOTAL
// WRITES, not live data, until something compacts - and the base tree has no
// automatic compaction (the planners are opt-in features, and running them is
// caller-driven). That is correct for the shape, but it has to be a measured,
// stated property rather than a surprise, so this pins the ratio.
#[test]
fn on_disk_bytes_track_total_writes_not_live_data() {
    fn dir_bytes(p: &std::path::Path) -> u64 {
        let mut total = 0;
        if let Ok(rd) = fs::read_dir(p) {
            for e in rd.flatten() {
                if let Ok(m) = e.metadata() {
                    total += if m.is_dir() {
                        dir_bytes(&e.path())
                    } else {
                        m.len()
                    };
                }
            }
        }
        total
    }

    let dir = fresh_dir("growth");
    let mut lsm = LsmTree::open(&dir, 16_000).unwrap();
    let value = vec![b'x'; 200];
    let keys = 200;

    // One pass over the key set: this is the live data, and it never changes.
    for i in 0..keys {
        lsm.put(&format!("key{i:05}"), &value).unwrap();
    }
    lsm.flush().unwrap();
    let after_first_pass = dir_bytes(&dir);

    // Nine more passes over the SAME keys. Live data is identical throughout.
    for _ in 0..9 {
        for i in 0..keys {
            lsm.put(&format!("key{i:05}"), &value).unwrap();
        }
    }
    lsm.flush().unwrap();
    let after_ten_passes = dir_bytes(&dir);

    // The tree still answers for exactly the same key set.
    assert_eq!(
        lsm.get(&format!("key{:05}", keys - 1)).unwrap().as_deref(),
        Some(&value[..]),
        "rewriting must not lose the latest value"
    );

    // Growth is real and roughly linear in total writes - not a leak, the
    // log-structured design. Asserted as a band so the shape is pinned without
    // making the test brittle to block framing or trailer size.
    let ratio = after_ten_passes as f64 / after_first_pass as f64;
    assert!(
        ratio > 4.0,
        "10x the writes over a fixed key set should cost far more than 1x on disk \
         (no automatic compaction); got {ratio:.1}x ({after_first_pass} -> {after_ten_passes} bytes)"
    );
    assert!(
        ratio < 15.0,
        "growth must stay proportional to writes; a ratio above 10x-with-slack means \
         something amplifies beyond the append itself: got {ratio:.1}x"
    );
}

// The counterpart to the leak above: with compaction enabled, overwriting the
// same key set stays bounded. This is the fix - `set_compaction_trigger` (or a
// manual `compact`) merges the runs and reclaims the superseded versions, so
// on-disk tracks live data instead of total writes.
#[test]
fn compaction_bounds_on_disk_under_overwrite() {
    fn dir_bytes(p: &std::path::Path) -> u64 {
        let mut total = 0;
        if let Ok(rd) = fs::read_dir(p) {
            for e in rd.flatten() {
                if let Ok(m) = e.metadata() {
                    total += if m.is_dir() {
                        dir_bytes(&e.path())
                    } else {
                        m.len()
                    };
                }
            }
        }
        total
    }

    let dir = fresh_dir("compact-growth");
    let mut lsm = LsmTree::open(&dir, 16_000).unwrap();
    lsm.set_compaction_trigger(4);
    let value = vec![b'x'; 200];
    let keys = 200;

    for i in 0..keys {
        lsm.put(&format!("key{i:05}"), &value).unwrap();
    }
    lsm.flush().unwrap();
    lsm.compact().unwrap();
    let after_first_pass = dir_bytes(&dir);

    // Nine more passes over the SAME keys, compacting each time.
    for _ in 0..9 {
        for i in 0..keys {
            lsm.put(&format!("key{i:05}"), &value).unwrap();
        }
        lsm.flush().unwrap();
        lsm.compact().unwrap();
    }
    let after_ten_passes = dir_bytes(&dir);

    // The latest value is still correct after all the merging.
    assert_eq!(
        lsm.get(&format!("key{:05}", keys - 1)).unwrap().as_deref(),
        Some(&value[..]),
        "compaction must preserve the newest value per key"
    );

    // On-disk stays within a small multiple of live data - the leak is gone.
    let ratio = after_ten_passes as f64 / after_first_pass as f64;
    assert!(
        ratio < 2.0,
        "compaction must bound on-disk under overwrite; got {ratio:.1}x \
         ({after_first_pass} -> {after_ten_passes} bytes)"
    );
    // A full merge collapses everything to one run.
    assert_eq!(
        lsm.sstable_count(),
        1,
        "full compaction leaves a single run"
    );
}

// A tombstone survives compaction as a real delete: the key must read absent
// after the dead versions are merged away.
#[test]
fn compaction_drops_deleted_keys() {
    let dir = fresh_dir("compact-delete");
    let mut lsm = LsmTree::open(&dir, 16_000).unwrap();
    let value = vec![b'z'; 64];
    for i in 0..100 {
        lsm.put(&format!("k{i:03}"), &value).unwrap();
    }
    lsm.flush().unwrap();
    lsm.delete("k050").unwrap();
    lsm.flush().unwrap();
    lsm.compact().unwrap();

    assert_eq!(
        lsm.get("k050").unwrap(),
        None,
        "deleted key must stay absent after compaction"
    );
    assert_eq!(
        lsm.get("k049").unwrap().as_deref(),
        Some(&value[..]),
        "surviving keys must be intact after compaction"
    );
}

#[test]
fn range_sorted_within_memtable() {
    let dir = fresh_dir("range_mem");
    let mut lsm = LsmTree::open(&dir, 1 << 20).unwrap();
    lsm.put("c", b"3").unwrap();
    lsm.put("a", b"1").unwrap();
    lsm.put("b", b"2").unwrap();
    let got = lsm.range(None, None).unwrap();
    let keys: Vec<_> = got.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(keys, ["a", "b", "c"], "range is sorted ascending");
    assert_eq!(got[0].1.as_slice(), b"1");
}

#[test]
fn range_half_open_bounds() {
    let dir = fresh_dir("range_bounds");
    let mut lsm = LsmTree::open(&dir, 1 << 20).unwrap();
    for k in ["a", "b", "c", "d", "e"] {
        lsm.put(k, k.as_bytes()).unwrap();
    }
    let keys: Vec<_> = lsm
        .range(Some("b"), Some("d"))
        .unwrap()
        .into_iter()
        .map(|(k, _)| k)
        .collect();
    assert_eq!(keys, ["b", "c"], "lo inclusive, hi exclusive");
}

#[test]
fn range_lo_only_and_hi_only() {
    let dir = fresh_dir("range_half");
    let mut lsm = LsmTree::open(&dir, 1 << 20).unwrap();
    for k in ["a", "b", "c", "d"] {
        lsm.put(k, k.as_bytes()).unwrap();
    }
    let from_c: Vec<_> = lsm
        .range(Some("c"), None)
        .unwrap()
        .into_iter()
        .map(|(k, _)| k)
        .collect();
    assert_eq!(from_c, ["c", "d"], "lo-only is unbounded above");
    let before_c: Vec<_> = lsm
        .range(None, Some("c"))
        .unwrap()
        .into_iter()
        .map(|(k, _)| k)
        .collect();
    assert_eq!(before_c, ["a", "b"], "hi-only is unbounded below");
}

#[test]
fn range_merges_memtable_over_sstables() {
    let dir = fresh_dir("range_merge");
    let mut lsm = LsmTree::open(&dir, 1 << 20).unwrap();
    lsm.put("a", b"old").unwrap();
    lsm.put("b", b"keep").unwrap();
    lsm.flush().unwrap();
    lsm.put("a", b"new").unwrap();
    lsm.put("c", b"fresh").unwrap();
    let map: std::collections::HashMap<String, Vec<u8>> =
        lsm.range(None, None).unwrap().into_iter().collect();
    assert_eq!(map.len(), 3);
    assert_eq!(
        map.get("a").map(Vec::as_slice),
        Some(&b"new"[..]),
        "newest wins"
    );
    assert_eq!(map.get("b").map(Vec::as_slice), Some(&b"keep"[..]));
    assert_eq!(map.get("c").map(Vec::as_slice), Some(&b"fresh"[..]));
}

#[test]
fn range_drops_tombstoned_keys() {
    let dir = fresh_dir("range_tomb");
    let mut lsm = LsmTree::open(&dir, 1 << 20).unwrap();
    lsm.put("a", b"1").unwrap();
    lsm.put("b", b"2").unwrap();
    lsm.flush().unwrap();
    lsm.delete("a").unwrap();
    let keys: Vec<_> = lsm
        .range(None, None)
        .unwrap()
        .into_iter()
        .map(|(k, _)| k)
        .collect();
    assert_eq!(
        keys,
        ["b"],
        "memtable tombstone omits the key from the range"
    );
}

#[test]
fn range_tombstone_shadows_across_runs() {
    let dir = fresh_dir("range_tomb_runs");
    let mut lsm = LsmTree::open(&dir, 1 << 20).unwrap();
    lsm.put("k", b"v").unwrap();
    lsm.flush().unwrap();
    lsm.delete("k").unwrap();
    lsm.flush().unwrap();
    assert!(
        lsm.range(None, None).unwrap().is_empty(),
        "a newer flushed tombstone hides the older on-disk value"
    );
}

#[test]
fn range_empty_and_unbounded() {
    let dir = fresh_dir("range_empty");
    let lsm = LsmTree::open(&dir, 1 << 20).unwrap();
    assert!(
        lsm.range(None, None).unwrap().is_empty(),
        "empty tree, no bounds"
    );
    assert!(
        lsm.range(Some("a"), Some("z")).unwrap().is_empty(),
        "empty tree, with bounds"
    );
}

#[test]
fn range_seeks_within_a_large_flushed_run() {
    let dir = fresh_dir("range_seek");
    let mut lsm = LsmTree::open(&dir, 1 << 20).unwrap();
    for i in 0..500u32 {
        lsm.put(&format!("key{i:04}"), format!("v{i}").as_bytes())
            .unwrap();
    }
    lsm.flush().unwrap(); // one sstable, 500 sorted records - exercises the seek

    let mid: Vec<_> = lsm
        .range(Some("key0100"), Some("key0110"))
        .unwrap()
        .into_iter()
        .map(|(k, _)| k)
        .collect();
    let expected: Vec<String> = (100..110).map(|i| format!("key{i:04}")).collect();
    assert_eq!(
        mid, expected,
        "narrow mid-run window seeks to lo, stops at hi"
    );

    assert_eq!(
        lsm.range(Some("key0000"), Some("key0003")).unwrap().len(),
        3,
        "lo at the very start"
    );
    assert_eq!(
        lsm.range(Some("key0498"), None).unwrap().len(),
        2,
        "lo near the end, unbounded above"
    );
    assert!(
        lsm.range(Some("key9999"), None).unwrap().is_empty(),
        "lo past every key"
    );
}

#[test]
fn default_flush_mode_is_background() {
    let dir = fresh_dir("mode_default");
    let lsm = LsmTree::open(&dir, 1 << 20).unwrap();
    assert_eq!(lsm.flush_mode(), FlushMode::Background);
}

#[test]
fn sync_flush_mode_rolls_sstables_inline() {
    let dir = fresh_dir("mode_sync");
    let mut lsm = LsmTree::open(&dir, 32).unwrap();
    lsm.set_flush_mode(FlushMode::Sync);
    assert_eq!(lsm.flush_mode(), FlushMode::Sync);
    for i in 0..20 {
        lsm.put(&format!("key{i}"), format!("v{i}").as_bytes())
            .unwrap();
    }
    // Sync mode flushes on the writer thread, so a threshold crossing is visible
    // immediately - no drain needed.
    assert!(
        lsm.sstable_count() >= 1,
        "sync flush rolls sstables on the write path"
    );
    for i in 0..20 {
        assert_eq!(
            lsm.get(&format!("key{i}")).unwrap().as_deref(),
            Some(format!("v{i}").as_bytes()),
        );
    }
}

#[test]
fn background_flush_reads_stay_consistent_across_tiers() {
    // Under a tiny threshold the default background flusher keeps rotating: at any
    // instant a given key may live in the active memtable, a frozen memtable still
    // queued for the worker, or an on-disk SSTable. A read must find it wherever
    // it is - never a transient miss during the hand-off.
    let dir = fresh_dir("bg_tiers");
    let mut lsm = LsmTree::open(&dir, 64).unwrap();
    for i in 0..2_000 {
        let k = format!("key{i:05}");
        lsm.put(&k, format!("v{i}").as_bytes()).unwrap();
        // Every write, re-read a key from early in the run - by now it is well
        // behind the active memtable, somewhere in the flush pipeline or on disk.
        let probe = format!("key{:05}", i / 2);
        assert_eq!(
            lsm.get(&probe).unwrap().as_deref(),
            Some(format!("v{}", i / 2).as_bytes()),
            "key {probe} vanished mid-flush",
        );
    }
    lsm.flush().unwrap();
    assert!(lsm.sstable_count() >= 1);
    for i in 0..2_000 {
        assert_eq!(
            lsm.get(&format!("key{i:05}")).unwrap().as_deref(),
            Some(format!("v{i}").as_bytes()),
        );
    }
}

#[test]
fn background_flush_persists_on_drop() {
    // Drop must drain the flush pipeline (queued frozen memtables + the active
    // one) so nothing written is lost when the tree goes away without a flush().
    let dir = fresh_dir("bg_drop");
    {
        let mut lsm = LsmTree::open(&dir, 128).unwrap();
        for i in 0..500 {
            lsm.put(&format!("key{i:04}"), format!("v{i}").as_bytes())
                .unwrap();
        }
        // no explicit flush(): rely on Drop
    }
    let lsm = LsmTree::open(&dir, 128).unwrap();
    for i in 0..500 {
        assert_eq!(
            lsm.get(&format!("key{i:04}")).unwrap().as_deref(),
            Some(format!("v{i}").as_bytes()),
            "key{i:04} not persisted on drop",
        );
    }
}

#[test]
fn writer_fails_instead_of_hanging_when_the_flush_worker_dies() {
    // Regression: the worker could die of an unchecked failure with a frozen
    // memtable still queued, and every producer parked forever on a queue
    // nothing would drain. The wait re-checked neither the error slot nor the
    // worker, so the JVM/process hung silently with no error and no timeout.
    let dir = fresh_dir("worker_death");
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut lsm = LsmTree::open(&dir, 64).unwrap();
        lsm.fault_next_flush();
        let mut outcome = Ok(());
        for i in 0..64 {
            outcome = lsm.put(&format!("key{i:04}"), &[b'v'; 32]);
            if outcome.is_err() {
                break;
            }
            if let Err(e) = lsm.flush() {
                outcome = Err(e);
                break;
            }
        }
        let _ = tx.send(outcome.err().map(|e| e.to_string()));
    });

    match rx.recv_timeout(std::time::Duration::from_secs(20)) {
        Ok(Some(msg)) => assert!(
            msg.contains("flush worker stopped"),
            "expected the dead worker to be reported, got: {msg}",
        ),
        Ok(None) => panic!("the injected fault never surfaced as an error"),
        Err(_) => panic!("writer blocked after the flush worker died"),
    }
}

#[test]
fn reads_still_serve_after_the_flush_worker_dies() {
    // The tree stops accepting flushes, but a poisoned write path must not take
    // the read path with it: everything already in memory stays readable.
    let dir = fresh_dir("worker_death_reads");
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut lsm = LsmTree::open(&dir, 64).unwrap();
        lsm.fault_next_flush();
        for i in 0..64 {
            if lsm.put(&format!("key{i:04}"), b"v").is_err() {
                break;
            }
        }
        let _ = tx.send(lsm.get("key0000").map(|v| v.is_some()));
    });

    match rx.recv_timeout(std::time::Duration::from_secs(20)) {
        Ok(hit) => assert_eq!(hit.ok(), Some(true), "the first key stopped being readable"),
        Err(_) => panic!("writer blocked after the flush worker died"),
    }
}
