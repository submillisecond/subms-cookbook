use super::*;
use std::env;
use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};

static SEQ: AtomicU64 = AtomicU64::new(0);

fn fresh_path(label: &str) -> PathBuf {
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = env::temp_dir().join(format!("subms-wal-{}-{}-{}", label, std::process::id(), n));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir.join("wal.log")
}

fn cleanup(p: &Path) {
    if let Some(parent) = p.parent() {
        let _ = fs::remove_dir_all(parent);
    }
}

#[test]
fn replay_is_empty_for_missing_file() {
    let path = fresh_path("missing");
    let p = path.with_file_name("does-not-exist.log");
    let entries = WriteAheadLog::replay(&p).unwrap();
    assert!(entries.is_empty());
    cleanup(&path);
}

#[test]
fn round_trip_put_and_delete() {
    let path = fresh_path("rt");
    {
        let mut wal = WriteAheadLog::open(&path).unwrap();
        wal.log_put("a", b"alpha").unwrap();
        wal.log_put("b", b"beta").unwrap();
        wal.log_delete("a").unwrap();
        wal.sync().unwrap();
    }
    let entries = WriteAheadLog::replay(&path).unwrap();
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].key, "a");
    assert_eq!(entries[0].value.as_deref(), Some(&b"alpha"[..]));
    assert_eq!(entries[1].key, "b");
    assert_eq!(entries[1].value.as_deref(), Some(&b"beta"[..]));
    assert_eq!(entries[2].key, "a");
    assert!(
        entries[2].value.is_none(),
        "delete must replay as tombstone"
    );
    cleanup(&path);
}

#[test]
fn truncate_drops_prior_records() {
    let path = fresh_path("truncate");
    {
        let mut wal = WriteAheadLog::open(&path).unwrap();
        wal.log_put("x", b"y").unwrap();
        wal.truncate().unwrap();
    }
    let entries = WriteAheadLog::replay(&path).unwrap();
    assert!(entries.is_empty(), "post-truncate replay must be empty");
    cleanup(&path);
}

#[test]
fn replay_recovers_across_reopen() {
    let path = fresh_path("reopen");
    {
        let mut wal = WriteAheadLog::open(&path).unwrap();
        wal.log_put("k1", b"v1").unwrap();
        wal.sync().unwrap();
    }
    {
        let mut wal = WriteAheadLog::open(&path).unwrap();
        wal.log_put("k2", b"v2").unwrap();
        wal.sync().unwrap();
    }
    let entries = WriteAheadLog::replay(&path).unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].key, "k1");
    assert_eq!(entries[1].key, "k2");
    cleanup(&path);
}

#[test]
fn torn_tail_is_dropped_without_corrupting_prefix() {
    let path = fresh_path("torn");
    {
        let mut wal = WriteAheadLog::open(&path).unwrap();
        wal.log_put("good", b"value").unwrap();
        wal.sync().unwrap();
    }
    // Append junk past the last valid record: simulates a half-written tail.
    {
        let mut f = OpenOptions::new().append(true).open(&path).unwrap();
        f.write_all(&[0x00, 0x00, 0x00, 0x00, 0x05]).unwrap();
        f.sync_all().unwrap();
    }
    let entries = WriteAheadLog::replay(&path).unwrap();
    assert_eq!(entries.len(), 1, "torn tail must not be replayed");
    assert_eq!(entries[0].key, "good");
    cleanup(&path);
}

#[test]
fn crc_corruption_truncates_at_bad_record() {
    let path = fresh_path("crc");
    {
        let mut wal = WriteAheadLog::open(&path).unwrap();
        wal.log_put("first", b"ok").unwrap();
        wal.log_put("second", b"corrupted").unwrap();
        wal.sync().unwrap();
    }
    // Flip a byte inside the SECOND record's payload.
    let mut buf = fs::read(&path).unwrap();
    let len = buf.len();
    // tail layout: ... value | crc(4). Flip a value byte ~14 from end.
    buf[len - 8] ^= 0xff;
    fs::write(&path, &buf).unwrap();
    let entries = WriteAheadLog::replay(&path).unwrap();
    assert_eq!(entries.len(), 1, "corrupt record drops everything after");
    assert_eq!(entries[0].key, "first");
    cleanup(&path);
}

#[test]
fn empty_value_put_round_trips() {
    let path = fresh_path("empty_value");
    {
        let mut wal = WriteAheadLog::open(&path).unwrap();
        wal.log_put("k", b"").unwrap();
        wal.sync().unwrap();
    }
    let entries = WriteAheadLog::replay(&path).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].value.as_deref(), Some(&b""[..]));
    cleanup(&path);
}
