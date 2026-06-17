//! `subms-ts-fill` - gap fill for irregular time series. Where consecutive
//! points are more than `step_ns` apart, insert synthetic points every
//! `step_ns` between them, filled by one of three policies:
//! linear interpolation, last-observation-carried-forward (LOCF), or zero.
//! Original points are always preserved.
//!
//! ```
//! use subms_ts::TsSeries;
//! use subms_ts_fill::fill_linear;
//!
//! let mut s = TsSeries::<f64>::new();
//! s.push(0, 0.0).unwrap();
//! s.push(40, 4.0).unwrap();   // a 40-wide gap
//! let filled = fill_linear(&s, 10); // step 10 -> insert at 10, 20, 30
//! let vals: Vec<(i64, f64)> = filled.iter().map(|p| (p.ts, p.value)).collect();
//! assert_eq!(vals, vec![(0, 0.0), (10, 1.0), (20, 2.0), (30, 3.0), (40, 4.0)]);
//! ```

use subms_ts::{TsPoint, TsSeries};

/// Linear interpolation between the bracketing points.
pub fn fill_linear(series: &TsSeries<f64>, step_ns: i64) -> TsSeries<f64> {
    fill_with(series, step_ns, |a, b, frac| {
        a.value + frac * (b.value - a.value)
    })
}

/// Last observation carried forward: each gap point repeats the left value.
pub fn fill_locf(series: &TsSeries<f64>, step_ns: i64) -> TsSeries<f64> {
    fill_with(series, step_ns, |a, _b, _frac| a.value)
}

/// Zero fill: each gap point is 0.0 (e.g. a counter with no events).
pub fn fill_zero(series: &TsSeries<f64>, step_ns: i64) -> TsSeries<f64> {
    fill_with(series, step_ns, |_a, _b, _frac| 0.0)
}

fn fill_with(
    series: &TsSeries<f64>,
    step_ns: i64,
    value_at: impl Fn(TsPoint<f64>, TsPoint<f64>, f64) -> f64,
) -> TsSeries<f64> {
    let pts: Vec<TsPoint<f64>> = series.iter().collect();
    let mut out = TsSeries::with_capacity(pts.len());
    if pts.is_empty() {
        return out;
    }
    let _ = out.push(pts[0].ts, pts[0].value);
    for w in pts.windows(2) {
        let (a, b) = (w[0], w[1]);
        let gap = b.ts - a.ts;
        if step_ns > 0 && gap > step_ns {
            let mut t = a.ts + step_ns;
            while t < b.ts {
                let frac = (t - a.ts) as f64 / gap as f64;
                let _ = out.push(t, value_at(a, b, frac));
                t += step_ns;
            }
        }
        let _ = out.push(b.ts, b.value);
    }
    out
}

#[cfg(feature = "harness")]
pub mod recipe;
