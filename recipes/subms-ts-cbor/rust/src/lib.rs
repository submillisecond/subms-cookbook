//! `subms-ts-cbor` - a zero-dep CBOR codec for `TsSeries<f64>`. Implements the
//! [`TsCodec`](subms_ts::TsCodec) substrate from `subms-ts` with a compact,
//! deterministic columnar encoding: a 2-key map `{"ts": [..], "v": [..]}`,
//! timestamps as CBOR integers and values as IEEE-754 float64.
//!
//! The encoding is canonical - fixed key order, minimal-width integer heads,
//! definite-length arrays - so the bytes are byte-equivalent across the Rust
//! and Java ports: a series encoded in one decodes byte-for-byte in the other.
//! Like the columnar JSON codec, this carries the data columns only; series
//! metadata is not part of the wire.
//!
//! ```
//! use subms_ts::{TsCodec, TsSeries};
//! use subms_ts_cbor::TsCborCodec;
//!
//! let mut s = TsSeries::<f64>::new();
//! s.push(1, 1.5).unwrap();
//! s.push(2, 2.5).unwrap();
//! let codec = TsCborCodec::new();
//! let bytes = codec.encode(&s);
//! let back = codec.decode(&bytes).unwrap();
//! assert_eq!(back.len(), 2);
//! ```

use subms_ts::{TsCodec, TsSeries};

#[cfg(feature = "harness")]
pub mod recipe;

/// Zero-dep CBOR codec for the scalar `f64` series.
#[derive(Clone, Debug, Default)]
pub struct TsCborCodec;

impl TsCborCodec {
    pub fn new() -> Self {
        Self
    }
}

/// Failure decoding a CBOR buffer that is not a well-formed series image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TsCborError {
    Truncated,
    /// A CBOR head whose major type / value was not what the grammar expects.
    Unexpected(String),
}

impl std::fmt::Display for TsCborError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TsCborError::Truncated => write!(f, "truncated CBOR buffer"),
            TsCborError::Unexpected(m) => write!(f, "unexpected CBOR token: {m}"),
        }
    }
}

impl std::error::Error for TsCborError {}

const MT_UINT: u8 = 0;
const MT_NINT: u8 = 1;
const MT_TEXT: u8 = 3;
const MT_ARRAY: u8 = 4;
const MT_MAP: u8 = 5;
const F64_HEAD: u8 = 0xfb;

fn write_head(out: &mut Vec<u8>, major: u8, n: u64) {
    let mt = major << 5;
    if n < 24 {
        out.push(mt | n as u8);
    } else if n < 0x100 {
        out.push(mt | 24);
        out.push(n as u8);
    } else if n < 0x1_0000 {
        out.push(mt | 25);
        out.extend_from_slice(&(n as u16).to_be_bytes());
    } else if n < 0x1_0000_0000 {
        out.push(mt | 26);
        out.extend_from_slice(&(n as u32).to_be_bytes());
    } else {
        out.push(mt | 27);
        out.extend_from_slice(&n.to_be_bytes());
    }
}

fn write_int(out: &mut Vec<u8>, v: i64) {
    if v >= 0 {
        write_head(out, MT_UINT, v as u64);
    } else {
        // CBOR negative int encodes -(n + 1); -1 - v is the unsigned payload.
        write_head(out, MT_NINT, (-1 - v) as u64);
    }
}

fn write_text(out: &mut Vec<u8>, s: &str) {
    write_head(out, MT_TEXT, s.len() as u64);
    out.extend_from_slice(s.as_bytes());
}

impl TsCodec<f64> for TsCborCodec {
    type Error = TsCborError;

    fn encode(&self, series: &TsSeries<f64>) -> Vec<u8> {
        let n = series.len();
        let mut out = Vec::with_capacity(2 + n * 13);
        write_head(&mut out, MT_MAP, 2);
        // fixed key order: "ts" then "v" - the canonical layout the Java port
        // mirrors, so the bytes match.
        write_text(&mut out, "ts");
        write_head(&mut out, MT_ARRAY, n as u64);
        for p in series.iter() {
            write_int(&mut out, p.ts);
        }
        write_text(&mut out, "v");
        write_head(&mut out, MT_ARRAY, n as u64);
        for p in series.iter() {
            out.push(F64_HEAD);
            out.extend_from_slice(&p.value.to_bits().to_be_bytes());
        }
        out
    }

    fn decode(&self, bytes: &[u8]) -> Result<TsSeries<f64>, Self::Error> {
        let mut r = Reader { buf: bytes, pos: 0 };
        let pairs = r.read_head(MT_MAP)?;
        let mut ts: Option<Vec<i64>> = None;
        let mut vals: Option<Vec<f64>> = None;
        for _ in 0..pairs {
            let key = r.read_text()?;
            match key.as_str() {
                "ts" => {
                    let len = r.read_head(MT_ARRAY)?;
                    let mut col = Vec::with_capacity(len as usize);
                    for _ in 0..len {
                        col.push(r.read_int()?);
                    }
                    ts = Some(col);
                }
                "v" => {
                    let len = r.read_head(MT_ARRAY)?;
                    let mut col = Vec::with_capacity(len as usize);
                    for _ in 0..len {
                        col.push(r.read_f64()?);
                    }
                    vals = Some(col);
                }
                other => return Err(TsCborError::Unexpected(format!("map key {other}"))),
            }
        }
        let ts = ts.ok_or_else(|| TsCborError::Unexpected("missing ts column".into()))?;
        let vals = vals.ok_or_else(|| TsCborError::Unexpected("missing v column".into()))?;
        if ts.len() != vals.len() {
            return Err(TsCborError::Unexpected(format!(
                "ts ({}) and v ({}) length mismatch",
                ts.len(),
                vals.len()
            )));
        }
        let mut s = TsSeries::with_capacity(ts.len());
        for (t, v) in ts.into_iter().zip(vals) {
            s.push(t, v)
                .map_err(|e| TsCborError::Unexpected(e.to_string()))?;
        }
        Ok(s)
    }

    fn format(&self) -> &str {
        "cbor"
    }
}

struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl Reader<'_> {
    fn byte(&mut self) -> Result<u8, TsCborError> {
        let b = *self.buf.get(self.pos).ok_or(TsCborError::Truncated)?;
        self.pos += 1;
        Ok(b)
    }

    fn take(&mut self, n: usize) -> Result<&[u8], TsCborError> {
        let end = self.pos.checked_add(n).ok_or(TsCborError::Truncated)?;
        let slice = self.buf.get(self.pos..end).ok_or(TsCborError::Truncated)?;
        self.pos = end;
        Ok(slice)
    }

    /// Read a head, require its major type, return the argument value.
    fn read_head(&mut self, want_major: u8) -> Result<u64, TsCborError> {
        let (major, arg) = self.read_any_head()?;
        if major != want_major {
            return Err(TsCborError::Unexpected(format!(
                "major {major}, wanted {want_major}"
            )));
        }
        Ok(arg)
    }

    fn read_any_head(&mut self) -> Result<(u8, u64), TsCborError> {
        let ib = self.byte()?;
        let major = ib >> 5;
        let info = ib & 0x1f;
        let arg = match info {
            0..=23 => info as u64,
            24 => self.byte()? as u64,
            25 => {
                let b = self.take(2)?;
                u16::from_be_bytes([b[0], b[1]]) as u64
            }
            26 => {
                let b = self.take(4)?;
                u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as u64
            }
            27 => {
                let b = self.take(8)?;
                u64::from_be_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]])
            }
            _ => return Err(TsCborError::Unexpected(format!("reserved info {info}"))),
        };
        Ok((major, arg))
    }

    fn read_text(&mut self) -> Result<String, TsCborError> {
        let len = self.read_head(MT_TEXT)? as usize;
        let bytes = self.take(len)?;
        String::from_utf8(bytes.to_vec()).map_err(|e| TsCborError::Unexpected(e.to_string()))
    }

    fn read_int(&mut self) -> Result<i64, TsCborError> {
        let (major, arg) = self.read_any_head()?;
        match major {
            MT_UINT => Ok(arg as i64),
            MT_NINT => Ok(-1 - arg as i64),
            other => Err(TsCborError::Unexpected(format!("int major {other}"))),
        }
    }

    fn read_f64(&mut self) -> Result<f64, TsCborError> {
        let head = self.byte()?;
        if head != F64_HEAD {
            return Err(TsCborError::Unexpected(format!("float head {head:#x}")));
        }
        let b = self.take(8)?;
        Ok(f64::from_bits(u64::from_be_bytes([
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
        ])))
    }
}
