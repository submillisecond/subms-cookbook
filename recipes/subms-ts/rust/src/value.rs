//! Provided value types. Any of these can be the `V` in `TsSeries<V>` - the
//! value carries the shape, so a yield-curve series is `TsSeries<Curve>` and
//! a vol-surface series is `TsSeries<Surface>`, never a parallel `TsCurve` /
//! `TsSurface` type. `TsValue` is the schemaless escape hatch.

use std::collections::BTreeMap;

use crate::TsValueKind;

/// An OHLCV bar - the canonical five fields. Extra provider-specific fields
/// (adjusted close, open interest, ...) belong in a consumer's own value
/// struct; the library ships the common shape + the genericity to BYO.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Ohlc {
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

impl Ohlc {
    pub fn new(open: f64, high: f64, low: f64, close: f64, volume: f64) -> Self {
        Self {
            open,
            high,
            low,
            close,
            volume,
        }
    }
}

impl TsValueKind for Ohlc {
    fn ts_is_present(&self) -> bool {
        self.open.is_finite()
            && self.high.is_finite()
            && self.low.is_finite()
            && self.close.is_finite()
            && self.volume.is_finite()
    }
}

/// A term structure / yield curve at one instant: parallel `axis` (tenor /
/// strike) + `values` columns. A curve time series is `TsSeries<Curve>` -
/// each point is a whole curve snapshot.
#[derive(Clone, Debug, PartialEq)]
pub struct Curve {
    pub axis: Vec<f64>,
    pub values: Vec<f64>,
}

impl Curve {
    pub fn new(axis: Vec<f64>, values: Vec<f64>) -> Self {
        Self { axis, values }
    }
}

impl TsValueKind for Curve {
    fn ts_is_present(&self) -> bool {
        self.axis.iter().all(|x| x.is_finite()) && self.values.iter().all(|x| x.is_finite())
    }
}

/// A surface (e.g. an implied-vol surface) at one instant: `axis_x` x `axis_y`
/// grid of `values`. A surface time series is `TsSeries<Surface>`.
#[derive(Clone, Debug, PartialEq)]
pub struct Surface {
    pub axis_x: Vec<f64>,
    pub axis_y: Vec<f64>,
    pub values: Vec<Vec<f64>>,
}

impl Surface {
    pub fn new(axis_x: Vec<f64>, axis_y: Vec<f64>, values: Vec<Vec<f64>>) -> Self {
        Self {
            axis_x,
            axis_y,
            values,
        }
    }
}

impl TsValueKind for Surface {
    fn ts_is_present(&self) -> bool {
        self.axis_x.iter().all(|x| x.is_finite())
            && self.axis_y.iter().all(|y| y.is_finite())
            && self.values.iter().flatten().all(|v| v.is_finite())
    }
}

/// Schemaless value. The "I don't know the value type yet" path:
/// `TsSeries<TsValue>` gets the full time-query surface; the numeric
/// surface stays dark (downcast per point to aggregate). `Map` / `Array`
/// carry arbitrarily nested JSON-shaped documents.
#[derive(Clone, Debug, PartialEq)]
pub enum TsValue {
    I64(i64),
    F64(f64),
    Bool(bool),
    Str(String),
    Bytes(Vec<u8>),
    Null,
    Map(BTreeMap<String, TsValue>),
    Array(Vec<TsValue>),
}

impl TsValueKind for TsValue {
    fn ts_is_present(&self) -> bool {
        // Only a top-level Null is a missing observation. A Null nested inside
        // a Map / Array is the caller's intentional null inside a document.
        !matches!(self, TsValue::Null)
    }
}
