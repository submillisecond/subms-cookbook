use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use subms_ts_wal::{
    RECORD_LEN, SEGMENT_MAX_RECORDS, TsFsyncPolicy, TsWal, TsWalRecord, encode_record,
};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// Unique scratch dir under the system temp dir. Process id + a monotonic
/// counter keeps parallel test threads from colliding.
struct Scratch {
    path: PathBuf,
}

impl Scratch {
    fn new() -> Self {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("subms-ts-wal-test-{}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        Self { path }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn list_segments(dir: &PathBuf) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n.starts_with("wal-") && n.ends_with(".log"))
        .collect();
    names.sort();
    names
}

#[test]
fn append_replay_round_trip_bit_exact() {
    let s = Scratch::new();
    let vals = [1.5, -2.25, 0.0, f64::MAX, f64::MIN_POSITIVE, 12345.678901];
    {
        let mut wal = TsWal::open(&s.path, TsFsyncPolicy::Never).unwrap();
        for (i, &v) in vals.iter().enumerate() {
            wal.append(42, i as i64, v).unwrap();
        }
        wal.flush().unwrap();
    }
    let wal = TsWal::open(&s.path, TsFsyncPolicy::Never).unwrap();
    let recs = wal.replay().unwrap();
    assert_eq!(recs.len(), vals.len());
    for (i, &v) in vals.iter().enumerate() {
        assert_eq!(recs[i].series_id, 42);
        assert_eq!(recs[i].ts, i as i64);
        // bit-exact: NaN payloads aside, the bits must match.
        assert_eq!(recs[i].value.to_bits(), v.to_bits());
    }
}

#[test]
fn nan_value_round_trips_bit_exact() {
    let s = Scratch::new();
    let nan = f64::from_bits(0x7ff8_0000_0000_0001);
    {
        let mut wal = TsWal::open(&s.path, TsFsyncPolicy::Never).unwrap();
        wal.append(1, 1, nan).unwrap();
        wal.flush().unwrap();
    }
    let recs = TsWal::open(&s.path, TsFsyncPolicy::Never)
        .unwrap()
        .replay()
        .unwrap();
    assert_eq!(recs.len(), 1);
    assert!(recs[0].value.is_nan());
    assert_eq!(recs[0].value.to_bits(), nan.to_bits());
}

#[test]
fn reopen_preserves_both_sessions_in_order() {
    let s = Scratch::new();
    {
        let mut wal = TsWal::open(&s.path, TsFsyncPolicy::Always).unwrap();
        wal.append(1, 10, 1.0).unwrap();
        wal.append(1, 11, 2.0).unwrap();
    } // drop flushes (Always already synced)
    {
        let mut wal = TsWal::open(&s.path, TsFsyncPolicy::Always).unwrap();
        wal.append(1, 12, 3.0).unwrap();
        wal.append(1, 13, 4.0).unwrap();
        wal.flush().unwrap();
    }
    let recs = TsWal::open(&s.path, TsFsyncPolicy::Never)
        .unwrap()
        .replay()
        .unwrap();
    let tss: Vec<i64> = recs.iter().map(|r| r.ts).collect();
    assert_eq!(tss, vec![10, 11, 12, 13]);
    let vals: Vec<f64> = recs.iter().map(|r| r.value).collect();
    assert_eq!(vals, vec![1.0, 2.0, 3.0, 4.0]);
}

#[test]
fn reopen_starts_fresh_segment_after_highest_seq() {
    let s = Scratch::new();
    {
        let mut wal = TsWal::open(&s.path, TsFsyncPolicy::Never).unwrap();
        wal.append(1, 1, 1.0).unwrap();
        wal.flush().unwrap();
        assert_eq!(wal.active_seq(), 0);
    }
    let wal = TsWal::open(&s.path, TsFsyncPolicy::Never).unwrap();
    // First open used seq 0; reopen must start at seq 1 so prior data is sealed.
    assert_eq!(wal.active_seq(), 1);
}

#[test]
fn segment_rollover_produces_multiple_files_and_replays_all() {
    let s = Scratch::new();
    let total = SEGMENT_MAX_RECORDS * 2 + 100;
    {
        let mut wal = TsWal::open(&s.path, TsFsyncPolicy::Never).unwrap();
        for i in 0..total {
            wal.append(9, i as i64, i as f64).unwrap();
        }
        wal.flush().unwrap();
    }
    let segs = list_segments(&s.path);
    assert!(segs.len() >= 3, "expected >=3 segments, got {segs:?}");
    assert_eq!(segs[0], "wal-0000000000.log");

    let recs = TsWal::open(&s.path, TsFsyncPolicy::Never)
        .unwrap()
        .replay()
        .unwrap();
    assert_eq!(recs.len() as u64, total);
    for i in 0..total {
        assert_eq!(recs[i as usize].ts, i as i64);
    }
}

#[test]
fn truncation_safety_garbage_tail_returns_valid_prefix() {
    let s = Scratch::new();
    let good = 50u64;
    {
        let mut wal = TsWal::open(&s.path, TsFsyncPolicy::Never).unwrap();
        for i in 0..good {
            wal.append(3, i as i64, i as f64).unwrap();
        }
        wal.flush().unwrap();
    }
    // Simulate a crash mid-append: 10 stray bytes appended to the active segment.
    let active = s.path.join("wal-0000000000.log");
    let mut f = OpenOptions::new().append(true).open(&active).unwrap();
    f.write_all(&[0xAB; 10]).unwrap();
    f.flush().unwrap();
    drop(f);

    let recs = TsWal::open(&s.path, TsFsyncPolicy::Never)
        .unwrap()
        .replay()
        .unwrap();
    assert_eq!(
        recs.len() as u64,
        good,
        "torn tail must not poison recovery"
    );
    for i in 0..good {
        assert_eq!(recs[i as usize].ts, i as i64);
    }
}

#[test]
fn truncation_safety_half_record_dropped() {
    let s = Scratch::new();
    {
        let mut wal = TsWal::open(&s.path, TsFsyncPolicy::Never).unwrap();
        wal.append(1, 0, 0.0).unwrap();
        wal.append(1, 1, 1.0).unwrap();
        wal.flush().unwrap();
    }
    // A full record minus its last 4 bytes: a torn write of a third record.
    let partial = encode_record(1, 2, 2.0);
    let active = s.path.join("wal-0000000000.log");
    let mut f = OpenOptions::new().append(true).open(&active).unwrap();
    f.write_all(&partial[..RECORD_LEN - 4]).unwrap();
    f.flush().unwrap();
    drop(f);

    let recs = TsWal::open(&s.path, TsFsyncPolicy::Never)
        .unwrap()
        .replay()
        .unwrap();
    assert_eq!(recs.len(), 2);
    assert_eq!(recs[1].ts, 1);
}

#[test]
fn crc_corruption_stops_at_bad_record() {
    let s = Scratch::new();
    {
        let mut wal = TsWal::open(&s.path, TsFsyncPolicy::Never).unwrap();
        for i in 0..5 {
            wal.append(1, i as i64, i as f64).unwrap();
        }
        wal.flush().unwrap();
    }
    // Flip a byte inside record index 2's payload (its value field). The CRC no
    // longer matches, so replay must drop record 2 and everything after it.
    let active = s.path.join("wal-0000000000.log");
    let mut bytes = fs::read(&active).unwrap();
    let target = 2 * RECORD_LEN + 16; // first byte of record 2's value_bits
    bytes[target] ^= 0xFF;
    fs::write(&active, &bytes).unwrap();

    let recs = TsWal::open(&s.path, TsFsyncPolicy::Never)
        .unwrap()
        .replay()
        .unwrap();
    assert_eq!(recs.len(), 2, "replay stops at the corrupt record");
    assert_eq!(recs[0].ts, 0);
    assert_eq!(recs[1].ts, 1);
}

#[test]
fn truncate_before_drops_only_old_sealed_segments() {
    let s = Scratch::new();
    let per = SEGMENT_MAX_RECORDS;
    {
        let mut wal = TsWal::open(&s.path, TsFsyncPolicy::Never).unwrap();
        // Segment 0: ts 0..per (all old). Segment 1: ts per..2per (straddles).
        // Then a few into segment 2 (active).
        for i in 0..(per * 2 + 5) {
            wal.append(1, i as i64, i as f64).unwrap();
        }
        wal.flush().unwrap();

        let before = list_segments(&s.path).len();
        assert!(before >= 3);

        // Cutoff inside segment 1's range: segment 0 (last ts = per-1 < cutoff)
        // is fully old and dropped; segment 1 straddles and survives; active
        // segment is never touched.
        let cutoff = (per + 10) as i64;
        let removed = wal.truncate_before(cutoff).unwrap();
        assert_eq!(removed, 1, "only segment 0 should be removed");
        assert!(!s.path.join("wal-0000000000.log").exists());
        assert!(s.path.join("wal-0000000001.log").exists());

        let recs = wal.replay().unwrap();
        // segment 0 gone (ts 0..per), survivors start at ts == per.
        assert_eq!(recs.first().unwrap().ts, per as i64);
        assert_eq!(recs.last().unwrap().ts, (per * 2 + 4) as i64);
    }
}

#[test]
fn truncate_before_never_touches_active_segment() {
    let s = Scratch::new();
    {
        let mut wal = TsWal::open(&s.path, TsFsyncPolicy::Never).unwrap();
        wal.append(1, 1, 1.0).unwrap();
        wal.append(1, 2, 2.0).unwrap();
        wal.flush().unwrap();
        // Cutoff far in the future; the active segment must still survive.
        let removed = wal.truncate_before(1_000_000).unwrap();
        assert_eq!(removed, 0);
        let recs = wal.replay().unwrap();
        assert_eq!(recs.len(), 2);
    }
}

#[test]
fn policy_every_n_appends_correctness() {
    let s = Scratch::new();
    {
        let mut wal = TsWal::open(&s.path, TsFsyncPolicy::EveryNAppends(8)).unwrap();
        for i in 0..20 {
            wal.append(1, i as i64, i as f64).unwrap();
        }
        wal.flush().unwrap();
    }
    let recs = TsWal::open(&s.path, TsFsyncPolicy::Never)
        .unwrap()
        .replay()
        .unwrap();
    assert_eq!(recs.len(), 20);
}

#[test]
fn policy_every_n_millis_correctness() {
    let s = Scratch::new();
    {
        let mut wal = TsWal::open(&s.path, TsFsyncPolicy::EveryNMillis(1)).unwrap();
        for i in 0..30 {
            wal.append(1, i as i64, i as f64).unwrap();
        }
        wal.flush().unwrap();
    }
    let recs = TsWal::open(&s.path, TsFsyncPolicy::Never)
        .unwrap()
        .replay()
        .unwrap();
    assert_eq!(recs.len(), 30);
}

#[test]
fn policy_always_correctness() {
    let s = Scratch::new();
    {
        let mut wal = TsWal::open(&s.path, TsFsyncPolicy::Always).unwrap();
        for i in 0..15 {
            wal.append(1, i as i64, i as f64).unwrap();
        }
    } // no explicit flush; Always already synced every append
    let recs = TsWal::open(&s.path, TsFsyncPolicy::Never)
        .unwrap()
        .replay()
        .unwrap();
    assert_eq!(recs.len(), 15);
}

#[test]
fn empty_wal_replays_empty() {
    let s = Scratch::new();
    let wal = TsWal::open(&s.path, TsFsyncPolicy::Never).unwrap();
    assert!(wal.replay().unwrap().is_empty());
}

#[test]
fn fresh_open_creates_dir_if_absent() {
    let s = Scratch::new();
    let nested = s.path.join("a").join("b");
    let wal = TsWal::open(&nested, TsFsyncPolicy::Never).unwrap();
    assert!(nested.is_dir());
    assert!(wal.replay().unwrap().is_empty());
}

// Pins the cross-language record layout: series_id=7, ts=100, value=1.5.
// The Java port asserts the identical 28-byte hex, proving the LE field order
// and the hand-rolled CRC-32 match java.util.zip.CRC32 bit-for-bit.
const RECORD_FIXTURE: &str = "07000000000000006400000000000000000000000000f83fdf7aaa15";

fn to_hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

#[test]
fn record_encoding_matches_fixture() {
    let buf = encode_record(7, 100, 1.5);
    assert_eq!(to_hex(&buf), RECORD_FIXTURE);
}

#[test]
fn record_struct_is_copy() {
    let r = TsWalRecord {
        series_id: 1,
        ts: 2,
        value: 3.0,
    };
    let r2 = r; // Copy
    assert_eq!(r, r2);
}
