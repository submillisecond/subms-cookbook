//! `subms-ts-anomaly` - a streaming rolling-window z-score anomaly detector.
//! Push points in time order; each value is scored against the mean + standard
//! deviation of the points already in the trailing `window_ns`. If the
//! z-score exceeds the threshold, the point is flagged. O(1) amortised per
//! push (running sum + sum-of-squares over a window deque). The live
//! regime-shift / spike detector of the `timeseries` arc.
//!
//! ```
//! use subms_ts_anomaly::TsAnomalyDetector;
//!
//! let mut d = TsAnomalyDetector::new(1_000, 3.0); // 1000 ns window, 3 sigma
//! for i in 0..50 { assert!(d.push(i, 10.0).is_none()); } // stable baseline
//! let hit = d.push(50, 100.0); // a 10x spike
//! assert!(hit.is_some());
//! assert!(hit.unwrap().zscore > 3.0);
//! ```

use std::collections::VecDeque;

/// A flagged point: its timestamp, value, and the z-score that tripped the
/// threshold (signed - positive above the baseline, negative below).
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct TsAnomaly {
    pub ts: i64,
    pub value: f64,
    pub zscore: f64,
}

/// Rolling-window z-score detector. Scores each value against the window of
/// points strictly before it (`latest - ts < window_ns`), then admits it.
#[derive(Clone, Debug)]
pub struct TsAnomalyDetector {
    window_ns: i64,
    sigma: f64,
    buf: VecDeque<(i64, f64)>,
    sum: f64,
    sum_sq: f64,
}

impl TsAnomalyDetector {
    pub fn new(window_ns: i64, sigma_threshold: f64) -> Self {
        Self {
            window_ns: window_ns.max(1),
            sigma: sigma_threshold,
            buf: VecDeque::new(),
            sum: 0.0,
            sum_sq: 0.0,
        }
    }

    pub fn window_count(&self) -> usize {
        self.buf.len()
    }

    /// Score `value` against the current window, flag it if `|z| >= sigma`,
    /// then admit it to the window. Returns `None` while the window is still
    /// warming up (fewer than 2 prior points).
    pub fn push(&mut self, ts: i64, value: f64) -> Option<TsAnomaly> {
        let cutoff = ts - self.window_ns;
        while let Some(&(t, v)) = self.buf.front() {
            if t <= cutoff {
                self.buf.pop_front();
                self.sum -= v;
                self.sum_sq -= v * v;
            } else {
                break;
            }
        }

        let n = self.buf.len();
        let result = if n >= 2 {
            let nf = n as f64;
            let mean = self.sum / nf;
            let var = (self.sum_sq / nf - mean * mean).max(0.0);
            // floor std so a jump off a flat baseline still scores (rather
            // than dividing by zero); the result is a large finite z.
            let std = var.sqrt().max(1e-12);
            let z = (value - mean) / std;
            if z.abs() >= self.sigma {
                Some(TsAnomaly {
                    ts,
                    value,
                    zscore: z,
                })
            } else {
                None
            }
        } else {
            None
        };

        self.buf.push_back((ts, value));
        self.sum += value;
        self.sum_sq += value * value;
        result
    }
}

#[cfg(feature = "harness")]
pub mod recipe;
