//! `subms-ts-asof-join` - as-of joins over two time series. For each point in
//! the left series, find the matching point in the right by timestamp:
//! backward (largest right ts <= left ts), forward (smallest right ts >=
//! left ts), or nearest within a tolerance. The join every market-data /
//! sensor-fusion pipeline needs ("what was the bid when this trade printed?").
//!
//! Backward + forward run as a single linear merge-walk over both series
//! (O(n + m), no per-point search); nearest does a bounded two-sided lookup.
//!
//! ```
//! use subms_ts::TsSeries;
//! use subms_ts_asof_join::asof_join_backward;
//!
//! let mut trades = TsSeries::<f64>::new();
//! trades.push(10, 100.0).unwrap();
//! trades.push(25, 101.0).unwrap();
//! let mut quotes = TsSeries::<f64>::new();
//! quotes.push(5, 99.5).unwrap();
//! quotes.push(20, 99.8).unwrap();
//!
//! let m = asof_join_backward(&trades, &quotes);
//! assert_eq!(m[0].right.map(|p| p.ts), Some(5));  // quote as-of trade@10
//! assert_eq!(m[1].right.map(|p| p.ts), Some(20)); // quote as-of trade@25
//! ```

use subms_ts::{TsPoint, TsSeries};

/// One joined row: a left point and its matched right point (if any).
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct TsMatch {
    pub left: TsPoint<f64>,
    pub right: Option<TsPoint<f64>>,
}

/// For each left point, the right point with the largest ts <= left.ts.
pub fn asof_join_backward(left: &TsSeries<f64>, right: &TsSeries<f64>) -> Vec<TsMatch> {
    let r: Vec<TsPoint<f64>> = right.iter().collect();
    let mut out = Vec::with_capacity(left.len());
    let mut j = 0usize; // r[..j] all have ts <= current left.ts
    let mut last: Option<TsPoint<f64>> = None;
    for lp in left.iter() {
        while j < r.len() && r[j].ts <= lp.ts {
            last = Some(r[j]);
            j += 1;
        }
        out.push(TsMatch {
            left: lp,
            right: last,
        });
    }
    out
}

/// For each left point, the right point with the smallest ts >= left.ts.
pub fn asof_join_forward(left: &TsSeries<f64>, right: &TsSeries<f64>) -> Vec<TsMatch> {
    let r: Vec<TsPoint<f64>> = right.iter().collect();
    let mut out = Vec::with_capacity(left.len());
    let mut j = 0usize;
    for lp in left.iter() {
        while j < r.len() && r[j].ts < lp.ts {
            j += 1;
        }
        out.push(TsMatch {
            left: lp,
            right: r.get(j).copied(),
        });
    }
    out
}

/// For each left point, the nearest right point by absolute ts distance,
/// only if within `tolerance_ns` (else `None`). Ties resolve to the earlier.
pub fn asof_join_nearest(
    left: &TsSeries<f64>,
    right: &TsSeries<f64>,
    tolerance_ns: i64,
) -> Vec<TsMatch> {
    let r: Vec<TsPoint<f64>> = right.iter().collect();
    let mut out = Vec::with_capacity(left.len());
    let mut j = 0usize; // index of last right with ts <= left.ts
    let mut have_back = false;
    for lp in left.iter() {
        while j < r.len() && r[j].ts <= lp.ts {
            have_back = true;
            j += 1;
        }
        // back candidate at j-1, forward candidate at j
        let back = if have_back { Some(r[j - 1]) } else { None };
        let fwd = r.get(j).copied();
        let pick = match (back, fwd) {
            (Some(b), Some(f)) => {
                if (lp.ts - b.ts) <= (f.ts - lp.ts) {
                    Some(b)
                } else {
                    Some(f)
                }
            }
            (Some(b), None) => Some(b),
            (None, Some(f)) => Some(f),
            (None, None) => None,
        };
        let within = pick.filter(|p| (p.ts - lp.ts).abs() <= tolerance_ns);
        out.push(TsMatch {
            left: lp,
            right: within,
        });
    }
    out
}

#[cfg(feature = "harness")]
pub mod recipe;
