//! `subms-ts-resample` - snap an irregular series onto a regular time grid.
//! Points are grouped into fixed-width buckets `[k*period, (k+1)*period)` and
//! each bucket collapses to one value per the chosen mode (mean / last /
//! first / sum / count / min / max), emitted at the bucket start. The step
//! that turns ragged event data into the evenly-spaced series a chart axis or
//! a model expects.
//!
//! ```
//! use subms_ts::TsSeries;
//! use subms_ts_resample::{resample_to_grid, TsResampleMode};
//!
//! let mut s = TsSeries::<f64>::new();
//! s.push(0, 1.0).unwrap();
//! s.push(3, 3.0).unwrap();   // bucket [0,10)
//! s.push(11, 5.0).unwrap();  // bucket [10,20)
//! let g = resample_to_grid(&s, 10, TsResampleMode::Mean);
//! let v: Vec<(i64, f64)> = g.iter().map(|p| (p.ts, p.value)).collect();
//! assert_eq!(v, vec![(0, 2.0), (10, 5.0)]); // mean of [1,3], then [5]
//! ```

use subms_ts::{TsPoint, TsSeries};

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum TsResampleMode {
    Mean,
    Last,
    First,
    Sum,
    Count,
    Min,
    Max,
}

struct Bucket {
    start: i64,
    count: u32,
    sum: f64,
    first: f64,
    last: f64,
    min: f64,
    max: f64,
}

impl Bucket {
    fn new(start: i64, value: f64) -> Self {
        Self {
            start,
            count: 1,
            sum: value,
            first: value,
            last: value,
            min: value,
            max: value,
        }
    }

    fn update(&mut self, value: f64) {
        self.count += 1;
        self.sum += value;
        self.last = value;
        if value < self.min {
            self.min = value;
        }
        if value > self.max {
            self.max = value;
        }
    }

    fn value(&self, mode: TsResampleMode) -> f64 {
        match mode {
            TsResampleMode::Mean => self.sum / self.count as f64,
            TsResampleMode::Last => self.last,
            TsResampleMode::First => self.first,
            TsResampleMode::Sum => self.sum,
            TsResampleMode::Count => self.count as f64,
            TsResampleMode::Min => self.min,
            TsResampleMode::Max => self.max,
        }
    }
}

/// Resample `series` onto a `period_ns` grid using `mode`. Empty buckets (no
/// points) are not emitted - the output is as sparse as the input's bucket
/// coverage. `period_ns <= 0` returns an empty series.
pub fn resample_to_grid(
    series: &TsSeries<f64>,
    period_ns: i64,
    mode: TsResampleMode,
) -> TsSeries<f64> {
    let mut out = TsSeries::new();
    if period_ns <= 0 {
        return out;
    }
    let mut cur: Option<Bucket> = None;
    for TsPoint { ts, value } in series.iter() {
        let start = ts.div_euclid(period_ns) * period_ns;
        match cur {
            Some(ref b) if b.start == start => cur.as_mut().unwrap().update(value),
            Some(b) => {
                let _ = out.push(b.start, b.value(mode));
                cur = Some(Bucket::new(start, value));
            }
            None => cur = Some(Bucket::new(start, value)),
        }
    }
    if let Some(b) = cur {
        let _ = out.push(b.start, b.value(mode));
    }
    out
}

#[cfg(feature = "harness")]
pub mod recipe;
