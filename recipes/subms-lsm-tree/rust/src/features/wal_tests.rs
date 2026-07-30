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

fn framed_record(op: u8, key: &[u8], value: &[u8]) -> Vec<u8> {
    let mut crc = Crc32::new();
    crc.update(&[op]);
    crc.update(key);
    crc.update(value);
    let checksum = crc.finalize();
    let mut out = Vec::new();
    out.push(op);
    out.extend_from_slice(&(key.len() as u32).to_be_bytes());
    out.extend_from_slice(key);
    out.extend_from_slice(&(value.len() as u32).to_be_bytes());
    out.extend_from_slice(value);
    out.extend_from_slice(&checksum.to_be_bytes());
    out
}

#[test]
fn path_reports_the_backing_file() {
    let path = fresh_path("path");
    let wal = WriteAheadLog::open(&path).unwrap();
    assert_eq!(wal.path(), path.as_path());
    cleanup(&path);
}

#[test]
fn unknown_op_stops_replay_after_prefix() {
    let path = fresh_path("unknownop");
    let mut bytes = framed_record(OP_PUT, b"k", b"v");
    // A CRC-valid record whose op byte is neither put nor delete.
    bytes.extend(framed_record(0x7f, b"bad", b"x"));
    fs::write(&path, &bytes).unwrap();
    let entries = WriteAheadLog::replay(&path).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].key, "k");
    cleanup(&path);
}

#[test]
fn invalid_utf8_key_stops_replay_after_prefix() {
    let path = fresh_path("badutf8");
    let mut bytes = framed_record(OP_PUT, b"ok", b"v");
    // CRC covers the raw bytes, so this record passes the checksum but the
    // key is not valid UTF-8 - replay must stop at it, keeping the prefix.
    bytes.extend(framed_record(OP_PUT, &[0xff, 0xfe], b"x"));
    fs::write(&path, &bytes).unwrap();
    let entries = WriteAheadLog::replay(&path).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].key, "ok");
    cleanup(&path);
}

#[test]
fn trailing_partial_header_stops_replay() {
    let path = fresh_path("partialheader");
    let mut bytes = framed_record(OP_PUT, b"k", b"v");
    bytes.extend_from_slice(&[0x00, 0x00, 0x00]); // fewer than the 5-byte header
    fs::write(&path, &bytes).unwrap();
    let entries = WriteAheadLog::replay(&path).unwrap();
    assert_eq!(entries.len(), 1);
    cleanup(&path);
}

#[test]
fn truncated_value_stops_replay() {
    let path = fresh_path("truncval");
    let mut bytes = framed_record(OP_PUT, b"k", b"v");
    // op + key_len=1 + key + value_len=100, but the 100 value bytes are absent.
    bytes.push(OP_PUT);
    bytes.extend_from_slice(&1u32.to_be_bytes());
    bytes.push(b'z');
    bytes.extend_from_slice(&100u32.to_be_bytes());
    fs::write(&path, &bytes).unwrap();
    let entries = WriteAheadLog::replay(&path).unwrap();
    assert_eq!(entries.len(), 1);
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
