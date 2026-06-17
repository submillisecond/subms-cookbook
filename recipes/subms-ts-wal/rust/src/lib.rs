//! `subms-ts-wal` - an append-only write-ahead log for f64-valued time-series
//! records, with truncation-safe crash recovery.
//!
//! The log is a directory of fixed-width segment files. Each record is 28 bytes,
//! little-endian, with a trailing CRC-32 over its 24-byte payload:
//!
//! ```text
//! [series_id u64 LE][ts i64 LE][value_bits u64 LE][crc32 u32 LE]
//! ```
//!
//! `value_bits` is `f64::to_bits`, so the value round-trips bit-exact. Segments
//! roll to a fresh file every [`SEGMENT_MAX_RECORDS`]; the active segment is the
//! one with the highest sequence number. The point of the recipe is the recovery
//! path: a process can die mid-`append`, leaving a torn tail in the active
//! segment. [`TsWal::replay`] CRC-validates every record and stops at the first
//! short, garbage, or checksum-failing record, returning the valid prefix. A
//! crash never poisons recovery - it costs you only the records that were never
//! durably committed.
//!
//! This is the same contract RocksDB, etcd, and Kafka's log segments hold: the
//! WAL is the source of truth for durability, replay reconstructs in-memory
//! state, and a partial trailing write is discarded rather than fatal. Here the
//! replayed [`TsWalRecord`]s are meant to be fed into a `TsSeries` (the recipe
//! is independent - it does not depend on `subms-ts`; the consumer owns that
//! step).
//!
//! ```no_run
//! use subms_ts_wal::{TsFsyncPolicy, TsWal};
//!
//! let dir = std::env::temp_dir().join("wal-doctest");
//! let mut wal = TsWal::open(&dir, TsFsyncPolicy::EveryNAppends(64))?;
//! wal.append(7, 100, 1.5)?;
//! wal.append(7, 101, 2.5)?;
//! wal.flush()?;
//!
//! let records = wal.replay()?;
//! assert_eq!(records.len(), 2);
//! assert_eq!(records[0].value, 1.5);
//! # Ok::<(), subms_ts_wal::TsWalError>(())
//! ```

mod crc32;

#[cfg(feature = "harness")]
pub mod recipe;

use std::fs::{self, File, OpenOptions};
use std::io::{self, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Records per segment before the log rolls to a fresh file.
pub const SEGMENT_MAX_RECORDS: u64 = 4096;

/// Wire size of one record: 8 + 8 + 8 + 4.
pub const RECORD_LEN: usize = 28;

/// Payload bytes the CRC is taken over (everything but the trailing checksum).
const PAYLOAD_LEN: usize = 24;

/// One durably-logged sample. `value` round-trips bit-exact through the log.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TsWalRecord {
    pub series_id: u64,
    pub ts: i64,
    pub value: f64,
}

/// When to fsync the active segment.
///
/// `Always` issues a `sync_data` on every append: durable per-record, but
/// fsync-floor-limited and NOT a sub-ms guarantee on slow storage. The batched
/// policies amortise the fsync across many appends and are what the recipe's
/// sub-ms claim covers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TsFsyncPolicy {
    Always,
    EveryNAppends(u32),
    EveryNMillis(u32),
    Never,
}

/// Errors from log operations. `Corrupt` is reserved for malformed segment
/// directory state; the replay path treats a torn tail as a clean stop, not an
/// error, so it does not surface here.
#[derive(Debug)]
pub enum TsWalError {
    Io(io::Error),
    Corrupt(String),
}

impl std::fmt::Display for TsWalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TsWalError::Io(e) => write!(f, "wal io error: {e}"),
            TsWalError::Corrupt(m) => write!(f, "wal corrupt: {m}"),
        }
    }
}

impl std::error::Error for TsWalError {}

impl From<io::Error> for TsWalError {
    fn from(e: io::Error) -> Self {
        TsWalError::Io(e)
    }
}

/// Encode a record into its 28-byte on-disk form.
pub fn encode_record(series_id: u64, ts: i64, value: f64) -> [u8; RECORD_LEN] {
    let mut buf = [0u8; RECORD_LEN];
    buf[0..8].copy_from_slice(&series_id.to_le_bytes());
    buf[8..16].copy_from_slice(&ts.to_le_bytes());
    buf[16..24].copy_from_slice(&value.to_bits().to_le_bytes());
    let crc = crc32::crc32(&buf[0..PAYLOAD_LEN]);
    buf[24..28].copy_from_slice(&crc.to_le_bytes());
    buf
}

/// Decode + CRC-validate one record. Returns `None` if the slice is short or the
/// checksum does not match the payload - both signal a torn or corrupt tail.
fn decode_record(buf: &[u8]) -> Option<TsWalRecord> {
    if buf.len() < RECORD_LEN {
        return None;
    }
    let stored = u32::from_le_bytes(buf[24..28].try_into().ok()?);
    if crc32::crc32(&buf[0..PAYLOAD_LEN]) != stored {
        return None;
    }
    let series_id = u64::from_le_bytes(buf[0..8].try_into().ok()?);
    let ts = i64::from_le_bytes(buf[8..16].try_into().ok()?);
    let value_bits = u64::from_le_bytes(buf[16..24].try_into().ok()?);
    Some(TsWalRecord {
        series_id,
        ts,
        value: f64::from_bits(value_bits),
    })
}

fn segment_name(seq: u64) -> String {
    format!("wal-{seq:010}.log")
}

/// Parse a `wal-<seq>.log` filename back to its sequence number.
fn parse_segment_seq(name: &str) -> Option<u64> {
    let rest = name.strip_prefix("wal-")?.strip_suffix(".log")?;
    if rest.len() != 10 || !rest.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    rest.parse().ok()
}

/// Append-only write-ahead log over a directory of segment files.
pub struct TsWal {
    dir: PathBuf,
    policy: TsFsyncPolicy,
    active_seq: u64,
    writer: BufWriter<File>,
    records_in_segment: u64,
    appends_since_sync: u32,
    last_sync: Instant,
    dirty: bool,
}

impl TsWal {
    /// Open (creating if absent) the log at `dir`. Scans existing `wal-*.log`
    /// segments and starts a fresh active segment after the highest existing
    /// sequence, so a reopen preserves all prior data for [`replay`](Self::replay).
    pub fn open(dir: impl AsRef<Path>, policy: TsFsyncPolicy) -> Result<Self, TsWalError> {
        let dir = dir.as_ref().to_path_buf();
        fs::create_dir_all(&dir)?;

        let highest = Self::scan_segments(&dir)?.last().copied();
        // Fresh active segment after the highest existing seq. Sealed segments
        // stay untouched on disk; only the new one is written to. A first-ever
        // open starts at seq 0.
        let active_seq = highest.map_or(0, |h| h + 1);

        let writer = Self::open_segment_writer(&dir, active_seq)?;
        Ok(Self {
            dir,
            policy,
            active_seq,
            writer,
            records_in_segment: 0,
            appends_since_sync: 0,
            last_sync: Instant::now(),
            dirty: false,
        })
    }

    /// All segment sequence numbers present in `dir`, ascending.
    fn scan_segments(dir: &Path) -> Result<Vec<u64>, TsWalError> {
        let mut seqs = Vec::new();
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            if let Some(name) = entry.file_name().to_str()
                && let Some(seq) = parse_segment_seq(name)
            {
                seqs.push(seq);
            }
        }
        seqs.sort_unstable();
        Ok(seqs)
    }

    fn open_segment_writer(dir: &Path, seq: u64) -> Result<BufWriter<File>, TsWalError> {
        let path = dir.join(segment_name(seq));
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        Ok(BufWriter::new(file))
    }

    /// Append one record. Rolls to a new segment when the active one is full,
    /// then applies the fsync policy.
    pub fn append(&mut self, series_id: u64, ts: i64, value: f64) -> Result<(), TsWalError> {
        if self.records_in_segment >= SEGMENT_MAX_RECORDS {
            self.roll_segment()?;
        }
        let buf = encode_record(series_id, ts, value);
        self.writer.write_all(&buf)?;
        self.records_in_segment += 1;
        self.appends_since_sync += 1;
        self.dirty = true;
        self.maybe_sync()?;
        Ok(())
    }

    /// Seal the active segment and open the next one.
    fn roll_segment(&mut self) -> Result<(), TsWalError> {
        self.sync_now()?;
        self.active_seq += 1;
        self.writer = Self::open_segment_writer(&self.dir, self.active_seq)?;
        self.records_in_segment = 0;
        Ok(())
    }

    fn maybe_sync(&mut self) -> Result<(), TsWalError> {
        let should = match self.policy {
            TsFsyncPolicy::Always => true,
            TsFsyncPolicy::EveryNAppends(n) => n != 0 && self.appends_since_sync >= n,
            TsFsyncPolicy::EveryNMillis(ms) => {
                self.last_sync.elapsed().as_millis() as u64 >= ms as u64
            }
            TsFsyncPolicy::Never => false,
        };
        if should {
            self.sync_now()?;
        }
        Ok(())
    }

    fn sync_now(&mut self) -> Result<(), TsWalError> {
        self.writer.flush()?;
        self.writer.get_ref().sync_data()?;
        self.appends_since_sync = 0;
        self.last_sync = Instant::now();
        self.dirty = false;
        Ok(())
    }

    /// Flush the write buffer and fsync the active segment, regardless of policy.
    pub fn flush(&mut self) -> Result<(), TsWalError> {
        self.sync_now()
    }

    /// Replay every segment in sequence order, CRC-validating each record.
    ///
    /// Truncation-safe: at the first short, garbage, or checksum-failing record
    /// in any segment, replay stops and returns the valid prefix accumulated so
    /// far. A crash that left a torn write in the active segment therefore loses
    /// only the uncommitted tail; recovery never errors on it.
    pub fn replay(&self) -> Result<Vec<TsWalRecord>, TsWalError> {
        let mut out = Vec::new();
        for seq in Self::scan_segments(&self.dir)? {
            let path = self.dir.join(segment_name(seq));
            let mut file = File::open(&path)?;
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes)?;

            let mut offset = 0;
            // Walk fixed-width records. A short final chunk (offset + RECORD_LEN
            // past EOF) is a torn append and stops replay cleanly.
            while offset + RECORD_LEN <= bytes.len() {
                match decode_record(&bytes[offset..offset + RECORD_LEN]) {
                    Some(rec) => {
                        out.push(rec);
                        offset += RECORD_LEN;
                    }
                    None => return Ok(out),
                }
            }
            if offset < bytes.len() {
                // Trailing partial record: torn tail, stop here.
                return Ok(out);
            }
        }
        Ok(out)
    }

    /// Delete whole SEALED segments whose last record's `ts` is strictly less
    /// than `cutoff`. The active segment is never touched, and a segment that
    /// straddles the cutoff (any record at or after it) is kept whole. Returns
    /// the number of segments removed.
    pub fn truncate_before(&mut self, cutoff: i64) -> Result<usize, TsWalError> {
        let mut removed = 0;
        for seq in Self::scan_segments(&self.dir)? {
            if seq == self.active_seq {
                continue;
            }
            let path = self.dir.join(segment_name(seq));
            if let Some(last_ts) = Self::last_record_ts(&path)?
                && last_ts < cutoff
            {
                fs::remove_file(&path)?;
                removed += 1;
            }
        }
        Ok(removed)
    }

    /// The `ts` of the last valid record in a segment, or `None` if the segment
    /// holds no valid records.
    fn last_record_ts(path: &Path) -> Result<Option<i64>, TsWalError> {
        let mut file = File::open(path)?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        let mut offset = 0;
        let mut last = None;
        while offset + RECORD_LEN <= bytes.len() {
            match decode_record(&bytes[offset..offset + RECORD_LEN]) {
                Some(rec) => {
                    last = Some(rec.ts);
                    offset += RECORD_LEN;
                }
                None => break,
            }
        }
        Ok(last)
    }

    /// Active segment sequence number (for diagnostics + tests).
    pub fn active_seq(&self) -> u64 {
        self.active_seq
    }
}

impl Drop for TsWal {
    fn drop(&mut self) {
        // Best-effort durability on close. Errors on drop have nowhere to go;
        // a caller that needs the guarantee calls flush() explicitly.
        if self.dirty {
            let _ = self.writer.flush();
            let _ = self.writer.get_ref().sync_data();
        }
    }
}
