//! Write-ahead log: every memtable mutation is appended to a wal file
//! before returning. On reload the wal replays back into a fresh memtable;
//! a flush of the memtable then truncates the wal.
//!
//! File layout (big-endian, append-only):
//! ```text
//! record := op:u8  key_len:u32  key:utf-8  value_len:u32  value:bytes  crc:u32
//! op     := 0x00 (put) | 0x01 (delete)
//! ```
//!
//! The CRC32 (IEEE 802.3 polynomial, table-driven) covers `op | key | value`
//! and lets the replay path tear off a half-written tail without poisoning
//! the recovered state. A torn record is treated as end-of-log.
//!
//! Single-writer by construction. Sync policy is left to the caller via
//! [`WriteAheadLog::sync`]; the default is unsynced (Linux pagecache durability).

use std::fs::{File, OpenOptions};
use std::io::{self, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

const OP_PUT: u8 = 0x00;
const OP_DELETE: u8 = 0x01;

/// An entry replayed from disk. Mirrors the `Memtable` two-state value
/// shape: `Some(bytes)` for a put, `None` for a delete.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalEntry {
    pub key: String,
    pub value: Option<Vec<u8>>,
}

pub struct WriteAheadLog {
    path: PathBuf,
    /// `Option` so [`Self::truncate`] can drop the handle, replace the file,
    /// and reopen on Windows where set_len on an append-mode handle fails.
    writer: Option<BufWriter<File>>,
}

impl WriteAheadLog {
    /// Open (or create) the wal at `path`. Existing content is preserved
    /// so a subsequent [`Self::replay`] sees prior records.
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let path = path.as_ref().to_path_buf();
        let file = OpenOptions::new()
            .read(true)
            .create(true)
            .append(true)
            .open(&path)?;
        Ok(Self {
            path,
            writer: Some(BufWriter::new(file)),
        })
    }

    fn writer(&mut self) -> &mut BufWriter<File> {
        self.writer
            .as_mut()
            .expect("wal writer dropped without reopen")
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Append a put record. Returns after the record is buffered; call
    /// [`Self::sync`] to force durability.
    pub fn log_put(&mut self, key: &str, value: &[u8]) -> io::Result<()> {
        self.append(OP_PUT, key.as_bytes(), value)
    }

    /// Append a delete record (tombstone). Value bytes are empty.
    pub fn log_delete(&mut self, key: &str) -> io::Result<()> {
        self.append(OP_DELETE, key.as_bytes(), &[])
    }

    fn append(&mut self, op: u8, key: &[u8], value: &[u8]) -> io::Result<()> {
        let mut crc = Crc32::new();
        crc.update(&[op]);
        let kl = key.len() as u32;
        let vl = value.len() as u32;
        crc.update(key);
        crc.update(value);
        let checksum = crc.finalize();
        let w = self.writer();
        w.write_all(&[op])?;
        w.write_all(&kl.to_be_bytes())?;
        w.write_all(key)?;
        w.write_all(&vl.to_be_bytes())?;
        w.write_all(value)?;
        w.write_all(&checksum.to_be_bytes())?;
        w.flush()
    }

    /// fsync the underlying file. Call after a batch of writes when the
    /// caller needs crash-durability.
    pub fn sync(&mut self) -> io::Result<()> {
        let w = self.writer();
        w.flush()?;
        w.get_ref().sync_all()
    }

    /// Truncate the wal back to zero. Call after a successful memtable
    /// flush - the SSTable now owns the records' durability.
    ///
    /// Windows note: a file opened with `append` cannot be `set_len(0)`'d,
    /// so we close the handle, recreate the file with `truncate(true)`, and
    /// reopen in append mode.
    pub fn truncate(&mut self) -> io::Result<()> {
        if let Some(mut w) = self.writer.take() {
            w.flush()?;
            drop(w);
        }
        // Replace the file.
        let _ = OpenOptions::new()
            .write(true)
            .truncate(true)
            .create(true)
            .open(&self.path)?;
        let file = OpenOptions::new()
            .read(true)
            .create(true)
            .append(true)
            .open(&self.path)?;
        self.writer = Some(BufWriter::new(file));
        Ok(())
    }

    /// Replay every well-formed record from the wal. A torn final record
    /// (truncated mid-write or with a bad CRC) is treated as end-of-log
    /// and silently dropped - the surviving prefix is returned in order.
    pub fn replay(path: impl AsRef<Path>) -> io::Result<Vec<WalEntry>> {
        let path = path.as_ref();
        if !path.exists() {
            return Ok(Vec::new());
        }
        let mut buf = Vec::new();
        File::open(path)?.read_to_end(&mut buf)?;
        let mut entries = Vec::new();
        let mut p = 0usize;
        while p < buf.len() {
            // Header: op(1) + key_len(4)
            if p + 5 > buf.len() {
                break;
            }
            let op = buf[p];
            let key_len = u32::from_be_bytes(buf[p + 1..p + 5].try_into().unwrap()) as usize;
            // key + value_len(4) + crc(4)
            let after_key = p + 5 + key_len;
            if after_key + 4 > buf.len() {
                break;
            }
            let value_len =
                u32::from_be_bytes(buf[after_key..after_key + 4].try_into().unwrap()) as usize;
            let after_value = after_key + 4 + value_len;
            if after_value + 4 > buf.len() {
                break;
            }
            let key = &buf[p + 5..p + 5 + key_len];
            let value = &buf[after_key + 4..after_key + 4 + value_len];
            let stored_crc =
                u32::from_be_bytes(buf[after_value..after_value + 4].try_into().unwrap());
            let mut crc = Crc32::new();
            crc.update(&[op]);
            crc.update(key);
            crc.update(value);
            if crc.finalize() != stored_crc {
                // torn tail; stop here and keep the prefix.
                break;
            }
            let key_str = match std::str::from_utf8(key) {
                Ok(s) => s.to_string(),
                Err(_) => break,
            };
            let value_opt = match op {
                OP_PUT => Some(value.to_vec()),
                OP_DELETE => None,
                _ => break,
            };
            entries.push(WalEntry {
                key: key_str,
                value: value_opt,
            });
            p = after_value + 4;
        }
        Ok(entries)
    }
}

/// IEEE-802.3 CRC32 (same polynomial used by gzip / png). Table-driven
/// so the per-byte step is one xor + one lookup.
struct Crc32 {
    state: u32,
}

impl Crc32 {
    fn new() -> Self {
        Self { state: 0xffff_ffff }
    }
    fn update(&mut self, bytes: &[u8]) {
        let mut s = self.state;
        for &b in bytes {
            let i = ((s ^ b as u32) & 0xff) as usize;
            s = (s >> 8) ^ CRC32_TABLE[i];
        }
        self.state = s;
    }
    fn finalize(self) -> u32 {
        self.state ^ 0xffff_ffff
    }
}

const CRC32_POLY: u32 = 0xedb8_8320;
static CRC32_TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u32;
        let mut j = 0;
        while j < 8 {
            c = if c & 1 != 0 {
                (c >> 1) ^ CRC32_POLY
            } else {
                c >> 1
            };
            j += 1;
        }
        table[i] = c;
        i += 1;
    }
    table
};

#[cfg(test)]
mod tests {
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
}
