//! `subms-ts-aggregator` - a streaming rolling-window aggregator. Push points
//! in time order; read `min` / `max` / `sum` / `mean` / `count` over the last
//! `window_ns` at O(1) amortised per push. Mergeable across partitions for
//! horizontal fan-out. The streaming-query surface of the `timeseries` arc.
//!
//! min/max use monotonic deques (each point is pushed + popped at most once,
//! so the amortised cost is O(1)); sum/count use a running total + a window
//! buffer.
//!
//! ```
//! use subms_ts_aggregator::TsWindowedAggregator;
//!
//! let mut a = TsWindowedAggregator::new(1_000); // 1000 ns window
//! a.push(0, 5.0);
//! a.push(500, 1.0);
//! a.push(900, 9.0);
//! assert_eq!(a.min(), Some(1.0));
//! assert_eq!(a.max(), Some(9.0));
//! a.push(1_500, 2.0); // ts 0 + 500 now older than 1000 ns -> expired
//! assert_eq!(a.count(), 2); // 900 and 1500 remain
//! assert_eq!(a.min(), Some(2.0));
//! ```

use std::collections::VecDeque;

use subms_ts::TsPoint;

/// Rolling-window aggregator. The window keeps points whose timestamp is
/// within `window_ns` of the most recent push (`latest - ts < window_ns`).
#[derive(Clone, Debug)]
pub struct TsWindowedAggregator {
    window_ns: i64,
    buf: VecDeque<(i64, f64)>,
    sum: f64,
    // monotonic: min_dq increasing by value (front = min), max_dq decreasing
    min_dq: VecDeque<(i64, f64)>,
    max_dq: VecDeque<(i64, f64)>,
}

impl TsWindowedAggregator {
    pub fn new(window_ns: i64) -> Self {
        Self {
            window_ns: window_ns.max(1),
            buf: VecDeque::new(),
            sum: 0.0,
            min_dq: VecDeque::new(),
            max_dq: VecDeque::new(),
        }
    }

    pub fn window_ns(&self) -> i64 {
        self.window_ns
    }

    /// Push a point (timestamps must be non-decreasing) and expire anything
    /// that has now fallen out of the window.
    pub fn push(&mut self, ts: i64, value: f64) {
        self.buf.push_back((ts, value));
        self.sum += value;

        while let Some(&(_, v)) = self.min_dq.back() {
            if v >= value {
                self.min_dq.pop_back();
            } else {
                break;
            }
        }
        self.min_dq.push_back((ts, value));

        while let Some(&(_, v)) = self.max_dq.back() {
            if v <= value {
                self.max_dq.pop_back();
            } else {
                break;
            }
        }
        self.max_dq.push_back((ts, value));

        self.expire(ts);
    }

    fn expire(&mut self, now: i64) {
        let cutoff = now - self.window_ns;
        while let Some(&(t, v)) = self.buf.front() {
            if t <= cutoff {
                self.buf.pop_front();
                self.sum -= v;
            } else {
                break;
            }
        }
        while let Some(&(t, _)) = self.min_dq.front() {
            if t <= cutoff {
                self.min_dq.pop_front();
            } else {
                break;
            }
        }
        while let Some(&(t, _)) = self.max_dq.front() {
            if t <= cutoff {
                self.max_dq.pop_front();
            } else {
                break;
            }
        }
    }

    pub fn count(&self) -> usize {
        self.buf.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    pub fn sum(&self) -> f64 {
        self.sum
    }

    pub fn min(&self) -> Option<f64> {
        self.min_dq.front().map(|&(_, v)| v)
    }

    pub fn max(&self) -> Option<f64> {
        self.max_dq.front().map(|&(_, v)| v)
    }

    pub fn mean(&self) -> Option<f64> {
        if self.buf.is_empty() {
            None
        } else {
            Some(self.sum / self.buf.len() as f64)
        }
    }

    /// The points currently in the window, in time order.
    pub fn window(&self) -> impl Iterator<Item = TsPoint<f64>> + '_ {
        self.buf.iter().map(|&(ts, value)| TsPoint { ts, value })
    }

    /// Merge another aggregator (same logical window) into a new one, e.g. to
    /// fold per-partition windows on a coordinator. Points from both are
    /// replayed in time order, so the result is the correct rolling state as
    /// of the latest timestamp across both.
    pub fn merge(&self, other: &Self) -> Self {
        let mut all: Vec<(i64, f64)> = self.buf.iter().chain(other.buf.iter()).copied().collect();
        all.sort_by_key(|&(ts, _)| ts);
        let mut out = Self::new(self.window_ns.max(other.window_ns));
        for (ts, v) in all {
            out.push(ts, v);
        }
        out
    }

    // ---------- distributed-merge wire format ----------

    /// Serialise the partial window for shipping to a coordinator that will
    /// [`merge`](Self::merge) it. Only `window_ns` + the in-window points cross
    /// the wire - the sum and the min/max deques are derived state that
    /// [`from_wire`](Self::from_wire) rebuilds by replaying the points. Wire
    /// form, little-endian:
    ///
    /// ```text
    /// [version u8][window_ns i64][count u32][count x (ts i64, value_bits u64)]
    /// ```
    ///
    /// `value_bits` is `f64::to_bits`, so the round-trip is bit-exact. This
    /// layout is byte-equivalent to the Java port; a partial window encoded on
    /// a Rust shard decodes on a Java coordinator and vice versa.
    pub fn to_wire(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(WIRE_HEADER + self.buf.len() * WIRE_POINT);
        out.push(WIRE_VERSION);
        out.extend_from_slice(&self.window_ns.to_le_bytes());
        out.extend_from_slice(&(self.buf.len() as u32).to_le_bytes());
        for &(ts, value) in &self.buf {
            out.extend_from_slice(&ts.to_le_bytes());
            out.extend_from_slice(&value.to_bits().to_le_bytes());
        }
        out
    }

    /// Reconstruct a partial window from [`to_wire`](Self::to_wire) bytes,
    /// replaying the points so the sum + min/max deques are rebuilt.
    pub fn from_wire(bytes: &[u8]) -> Result<Self, TsAggWireError> {
        if bytes.is_empty() {
            return Err(TsAggWireError::Truncated);
        }
        if bytes[0] != WIRE_VERSION {
            return Err(TsAggWireError::BadVersion(bytes[0]));
        }
        if bytes.len() < WIRE_HEADER {
            return Err(TsAggWireError::Truncated);
        }
        let window_ns = i64::from_le_bytes(bytes[1..9].try_into().unwrap());
        let count = u32::from_le_bytes(bytes[9..13].try_into().unwrap()) as usize;
        if bytes.len() < WIRE_HEADER + count * WIRE_POINT {
            return Err(TsAggWireError::Truncated);
        }
        let mut agg = Self::new(window_ns);
        let mut off = WIRE_HEADER;
        for _ in 0..count {
            let ts = i64::from_le_bytes(bytes[off..off + 8].try_into().unwrap());
            let vbits = u64::from_le_bytes(bytes[off + 8..off + 16].try_into().unwrap());
            agg.push(ts, f64::from_bits(vbits));
            off += WIRE_POINT;
        }
        Ok(agg)
    }
}

const WIRE_VERSION: u8 = 1;
const WIRE_HEADER: usize = 1 + 8 + 4; // version + window_ns + count
const WIRE_POINT: usize = 8 + 8; // ts + value_bits

/// Failure decoding a [`TsWindowedAggregator::from_wire`] buffer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TsAggWireError {
    BadVersion(u8),
    Truncated,
}

impl std::fmt::Display for TsAggWireError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TsAggWireError::BadVersion(v) => write!(f, "unknown aggregator wire version {v}"),
            TsAggWireError::Truncated => write!(f, "truncated aggregator wire buffer"),
        }
    }
}

impl std::error::Error for TsAggWireError {}

#[cfg(feature = "harness")]
pub mod recipe;
