//! `subms-gorilla-block` - a Gorilla-encoded time-series column block
//! (Pelkonen et al., VLDB 2015): delta-of-delta timestamps + XOR-delta `f64`
//! values, ~1.5 bytes per point versus 16 raw. The compression scheme
//! Prometheus / VictoriaMetrics / M3 / InfluxDB all run internally, here as a
//! clean embeddable library + the cold tier behind `subms-ts`'s `TsRange`.
//!
//! The wire format is byte-equivalent across the Rust + Java ports: a block
//! encoded in one decodes byte-for-byte in the other.
//!
//! ```
//! use subms_gorilla_block::TsGorillaBlock;
//!
//! let mut b = TsGorillaBlock::new();
//! for i in 0..1000 {
//!     b.append(1_000 + i, 42.0 + (i as f64).sin());
//! }
//! let bytes = b.bytes();
//! let decoded = TsGorillaBlock::from_bytes(&bytes).unwrap();
//! assert_eq!(decoded.iter().count(), 1000);
//! assert!(bytes.len() < 1000 * 16); // compressed vs raw (ts,val)
//! ```

mod bits;
mod codec;

#[cfg(feature = "harness")]
pub mod recipe;

pub use codec::TsGorillaCodec;

use bits::{BitReader, BitWriter};
use subms_ts::TsPoint;

const VERSION: u8 = 1;
const NO_WINDOW: u32 = u32::MAX;

#[derive(Clone, Debug, PartialEq)]
pub struct TsBlockStats {
    pub count: u32,
    pub ts_min: i64,
    pub ts_max: i64,
    pub value_min: f64,
    pub value_max: f64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TsBlockError {
    BadVersion(u8),
    Truncated,
}

impl std::fmt::Display for TsBlockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TsBlockError::BadVersion(v) => write!(f, "unknown block version {v}"),
            TsBlockError::Truncated => write!(f, "truncated block bitstream"),
        }
    }
}

impl std::error::Error for TsBlockError {}

/// A Gorilla-compressed run of `(i64 ts, f64 value)` points. Append-only;
/// timestamps must be non-decreasing. Decode is whole-block (the Gorilla
/// stream is not random-access), which suits the cold-tier scan pattern.
#[derive(Clone)]
pub struct TsGorillaBlock {
    writer: BitWriter,
    count: u32,
    first_ts: i64,
    last_ts: i64,
    prev_delta: i64,
    prev_value: u64,
    prev_leading: u32,
    prev_trailing: u32,
    val_min: f64,
    val_max: f64,
}

impl Default for TsGorillaBlock {
    fn default() -> Self {
        Self::new()
    }
}

impl TsGorillaBlock {
    pub fn new() -> Self {
        Self::with_capacity(64)
    }

    pub fn with_capacity(byte_cap: usize) -> Self {
        Self {
            writer: BitWriter::with_capacity(byte_cap),
            count: 0,
            first_ts: 0,
            last_ts: 0,
            prev_delta: 0,
            prev_value: 0,
            prev_leading: NO_WINDOW,
            prev_trailing: 0,
            val_min: f64::INFINITY,
            val_max: f64::NEG_INFINITY,
        }
    }

    pub fn len(&self) -> usize {
        self.count as usize
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Append a point. Timestamps must be non-decreasing across calls.
    pub fn append(&mut self, ts: i64, value: f64) {
        let vbits = value.to_bits();
        if self.count == 0 {
            self.writer.write_bits(ts as u64, 64);
            self.writer.write_bits(vbits, 64);
            self.first_ts = ts;
            self.last_ts = ts;
            self.prev_value = vbits;
        } else {
            let delta = ts - self.last_ts;
            let dod = delta - self.prev_delta;
            encode_dod(&mut self.writer, dod);
            self.last_ts = ts;
            self.prev_delta = delta;
            encode_value(
                &mut self.writer,
                vbits,
                &mut self.prev_value,
                &mut self.prev_leading,
                &mut self.prev_trailing,
            );
        }
        if value < self.val_min {
            self.val_min = value;
        }
        if value > self.val_max {
            self.val_max = value;
        }
        self.count += 1;
    }

    /// Versioned wire bytes: `[version u8][count u32 LE][bitstream]`.
    pub fn bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(5 + self.count as usize * 2);
        out.push(VERSION);
        out.extend_from_slice(&self.count.to_le_bytes());
        if self.count > 0 {
            out.extend_from_slice(&self.writer.snapshot());
        }
        out
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, TsBlockError> {
        let points = decode_all(bytes)?;
        let mut b = Self::with_capacity(bytes.len());
        for p in points {
            b.append(p.ts, p.value);
        }
        Ok(b)
    }

    /// Decode bytes straight to points without rebuilding an appendable block.
    /// The cold-tier scan path: cheaper than `from_bytes` when you only read.
    pub fn decode(bytes: &[u8]) -> Result<Vec<TsPoint<f64>>, TsBlockError> {
        decode_all(bytes)
    }

    pub fn iter(&self) -> impl Iterator<Item = TsPoint<f64>> {
        decode_all(&self.bytes()).unwrap_or_default().into_iter()
    }

    /// Inclusive `[lo, hi]` filter over the decoded points.
    pub fn range(&self, lo: i64, hi: i64) -> impl Iterator<Item = TsPoint<f64>> {
        self.iter().filter(move |p| p.ts >= lo && p.ts <= hi)
    }

    /// Concatenate two blocks. Points are merged in non-decreasing ts order,
    /// so a block sealed earlier can fold into a later one.
    pub fn merge(&self, other: &Self) -> Self {
        let mut all: Vec<TsPoint<f64>> = self.iter().chain(other.iter()).collect();
        all.sort_by_key(|p| p.ts);
        let mut b = Self::with_capacity(self.bytes().len() + other.bytes().len());
        for p in all {
            b.append(p.ts, p.value);
        }
        b
    }

    pub fn stats(&self) -> TsBlockStats {
        TsBlockStats {
            count: self.count,
            ts_min: self.first_ts,
            ts_max: self.last_ts,
            value_min: if self.count == 0 { 0.0 } else { self.val_min },
            value_max: if self.count == 0 { 0.0 } else { self.val_max },
        }
    }
}

fn encode_dod(w: &mut BitWriter, dod: i64) {
    if dod == 0 {
        w.write_bit(0);
    } else if (-64..=63).contains(&dod) {
        w.write_bits(0b10, 2);
        w.write_bits(dod as u64 & 0x7F, 7);
    } else if (-256..=255).contains(&dod) {
        w.write_bits(0b110, 3);
        w.write_bits(dod as u64 & 0x1FF, 9);
    } else if (-2048..=2047).contains(&dod) {
        w.write_bits(0b1110, 4);
        w.write_bits(dod as u64 & 0xFFF, 12);
    } else {
        w.write_bits(0b1111, 4);
        w.write_bits(dod as u64, 64);
    }
}

fn encode_value(
    w: &mut BitWriter,
    vbits: u64,
    prev_value: &mut u64,
    prev_leading: &mut u32,
    prev_trailing: &mut u32,
) {
    let xor = vbits ^ *prev_value;
    if xor == 0 {
        w.write_bit(0);
    } else {
        w.write_bit(1);
        let leading = xor.leading_zeros().min(31);
        let trailing = xor.trailing_zeros();
        if *prev_leading != NO_WINDOW && leading >= *prev_leading && trailing >= *prev_trailing {
            w.write_bit(0);
            let mlen = 64 - *prev_leading - *prev_trailing;
            w.write_bits(xor >> *prev_trailing, mlen);
        } else {
            w.write_bit(1);
            w.write_bits(leading as u64, 5);
            let mlen = 64 - leading - trailing;
            // mlen is 1..=64; 64 stored as 0 in the 6-bit field.
            w.write_bits((mlen & 0x3F) as u64, 6);
            w.write_bits(xor >> trailing, mlen);
            *prev_leading = leading;
            *prev_trailing = trailing;
        }
    }
    *prev_value = vbits;
}

fn sign_extend(v: u64, n: u32) -> i64 {
    let shift = 64 - n;
    ((v << shift) as i64) >> shift
}

fn decode_dod(r: &mut BitReader) -> Result<i64, TsBlockError> {
    let b0 = r.read_bit().ok_or(TsBlockError::Truncated)?;
    if b0 == 0 {
        return Ok(0);
    }
    let (n, _) = if r.read_bit().ok_or(TsBlockError::Truncated)? == 0 {
        (7u32, ())
    } else if r.read_bit().ok_or(TsBlockError::Truncated)? == 0 {
        (9, ())
    } else if r.read_bit().ok_or(TsBlockError::Truncated)? == 0 {
        (12, ())
    } else {
        (64, ())
    };
    let raw = r.read_bits(n).ok_or(TsBlockError::Truncated)?;
    Ok(sign_extend(raw, n))
}

fn decode_all(bytes: &[u8]) -> Result<Vec<TsPoint<f64>>, TsBlockError> {
    if bytes.is_empty() {
        return Ok(Vec::new());
    }
    if bytes[0] != VERSION {
        return Err(TsBlockError::BadVersion(bytes[0]));
    }
    if bytes.len() < 5 {
        return Err(TsBlockError::Truncated);
    }
    let count = u32::from_le_bytes([bytes[1], bytes[2], bytes[3], bytes[4]]);
    if count == 0 {
        return Ok(Vec::new());
    }
    let mut r = BitReader::new(&bytes[5..]);
    let ts0 = r.read_bits(64).ok_or(TsBlockError::Truncated)? as i64;
    let v0 = r.read_bits(64).ok_or(TsBlockError::Truncated)?;
    let mut out = Vec::with_capacity(count as usize);
    out.push(TsPoint::new(ts0, f64::from_bits(v0)));

    let mut last_ts = ts0;
    let mut prev_delta = 0i64;
    let mut prev_value = v0;
    let mut leading = 0u32;
    let mut trailing = 0u32;

    for _ in 1..count {
        let dod = decode_dod(&mut r)?;
        let delta = prev_delta + dod;
        let ts = last_ts + delta;
        last_ts = ts;
        prev_delta = delta;

        if r.read_bit().ok_or(TsBlockError::Truncated)? == 1 {
            if r.read_bit().ok_or(TsBlockError::Truncated)? == 0 {
                let mlen = 64 - leading - trailing;
                let mant = r.read_bits(mlen).ok_or(TsBlockError::Truncated)?;
                prev_value ^= mant << trailing;
            } else {
                leading = r.read_bits(5).ok_or(TsBlockError::Truncated)? as u32;
                let mut mlen = r.read_bits(6).ok_or(TsBlockError::Truncated)? as u32;
                if mlen == 0 {
                    mlen = 64;
                }
                trailing = 64 - leading - mlen;
                let mant = r.read_bits(mlen).ok_or(TsBlockError::Truncated)?;
                prev_value ^= mant << trailing;
            }
        }
        out.push(TsPoint::new(ts, f64::from_bits(prev_value)));
    }
    Ok(out)
}

/// Min/max over decoded points - used by the mmap path, where the wire header
/// carries only version + count, so block stats need a decode pass.
#[cfg(feature = "mmap")]
fn stats_of(points: &[TsPoint<f64>]) -> TsBlockStats {
    if points.is_empty() {
        return TsBlockStats { count: 0, ts_min: 0, ts_max: 0, value_min: 0.0, value_max: 0.0 };
    }
    let mut vmin = points[0].value;
    let mut vmax = points[0].value;
    for p in &points[1..] {
        if p.value < vmin {
            vmin = p.value;
        }
        if p.value > vmax {
            vmax = p.value;
        }
    }
    TsBlockStats {
        count: points.len() as u32,
        ts_min: points[0].ts,
        ts_max: points[points.len() - 1].ts,
        value_min: vmin,
        value_max: vmax,
    }
}

/// A block backed by a memory-mapped file. The cold-tier read path: the block
/// bytes stay in the page cache and are decoded straight from the mapping, so
/// opening a block never copies the file onto the heap. Decode is still
/// whole-block (the Gorilla stream is not random-access); the win here is the
/// absent heap copy, not random access.
///
/// Requires the `mmap` feature. The mapping is read-only; mutating the file
/// underneath a live `TsMmapBlock` is the caller's hazard (the standard mmap
/// aliasing caveat), which is why [`open`](TsMmapBlock::open) is the only
/// constructor and there is no write path.
#[cfg(feature = "mmap")]
pub struct TsMmapBlock {
    mmap: memmap2::Mmap,
}

#[cfg(feature = "mmap")]
impl TsMmapBlock {
    /// Map a block file read-only.
    pub fn open(path: impl AsRef<std::path::Path>) -> std::io::Result<Self> {
        let file = std::fs::File::open(path)?;
        // Safety: read-only mapping; we never write through it. See the type
        // doc for the external-mutation caveat the caller owns.
        let mmap = unsafe { memmap2::Mmap::map(&file)? };
        Ok(Self { mmap })
    }

    /// The mapped bytes, exactly the [`TsGorillaBlock::bytes`] wire form.
    pub fn bytes(&self) -> &[u8] {
        &self.mmap
    }

    /// Decode every point straight from the mapping.
    pub fn decode(&self) -> Result<Vec<TsPoint<f64>>, TsBlockError> {
        decode_all(&self.mmap)
    }

    pub fn iter(&self) -> impl Iterator<Item = TsPoint<f64>> {
        decode_all(&self.mmap).unwrap_or_default().into_iter()
    }

    /// Inclusive `[lo, hi]` scan over the mapped block.
    pub fn range(&self, lo: i64, hi: i64) -> impl Iterator<Item = TsPoint<f64>> {
        self.iter().filter(move |p| p.ts >= lo && p.ts <= hi)
    }

    pub fn stats(&self) -> Result<TsBlockStats, TsBlockError> {
        Ok(stats_of(&self.decode()?))
    }
}
