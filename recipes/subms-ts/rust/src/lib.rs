//! `subms-ts` - the generic time-series core of the submillisecond
//! cookbook `timeseries` arc. A [`TsSeries<T>`] is a time-ordered sequence
//! of [`TsPoint<T>`] backed by three temperature tiers: a mutable SoA head
//! chunk that absorbs `push` at tens of nanoseconds, sealing into immutable
//! warm chunks; the cold Gorilla-compressed tier plugs in behind the same
//! [`TsRange`] view via the `subms-gorilla-block` recipe.
//!
//! Timestamps are `i64` nanoseconds since the Unix epoch. The `datetime`
//! feature adds `chrono` conversions for ergonomic calendar access; the
//! default build is zero-dependency std-only.
//!
//! ```
//! use subms_ts::TsSeries;
//!
//! let mut s = TsSeries::<f64>::new();
//! s.push(1_000, 10.0).unwrap();
//! s.push(2_000, 12.5).unwrap();
//! s.push(3_000, 11.0).unwrap();
//!
//! assert_eq!(s.len(), 3);
//! assert_eq!(s.nearest_before(2_500).map(|p| p.value), Some(12.5));
//! assert_eq!(s.max(), Some(12.5));
//! assert_eq!(s.range_sum(1_000, 2_000), 22.5);
//! ```

mod codec;
mod collection;
mod dataframe;
mod meta;
mod panel;
mod point;
mod series;
mod value;

pub use codec::{TsCodec, TsCodecError, TsJsonCodec, TsTimestampStyle};
pub use collection::{TsAgg, TsCollection, TsCollectionError};
pub use dataframe::{TsColumn, TsDataFrame, TsDataType, TsField, TsFrameSchema};
pub use meta::{
    TsAttrs, TsDep, TsDepKind, TsFormat, TsNumericKind, TsSchema, TsSeriesMetadata, TsTags,
};
pub use panel::{TsPanel, TsPanelAligned, TsPanelGroup, TsPanelMetadata};
pub use point::TsPoint;
pub use series::{TsRange, TsSeries};
pub use value::{Curve, Ohlc, Surface, TsValue};

/// Convenience aliases for the provided compound value types.
pub type TsOhlc = TsSeries<Ohlc>;
pub type TsCurveSeries = TsSeries<Curve>;
pub type TsSurfaceSeries = TsSeries<Surface>;

#[cfg(feature = "datetime")]
mod datetime;

#[cfg(feature = "harness")]
pub mod recipe;

/// Errors returned by the ingest + range surface. Time-query + aggregate
/// reads never error - they return `Option` for the empty case.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TsError {
    /// `push` received a timestamp earlier than the series tail. Series are
    /// non-decreasing in `ts`; out-of-order inserts are rejected rather than
    /// silently sorted.
    NotMonotonic { last: i64, got: i64 },
    /// `push` received a null / non-finite observation. A missing
    /// observation is structurally meaningless in a series - do not insert
    /// it. (Multi-series row gaps are an alignment concern, handled by the
    /// frame layer, not by inserting nulls here.)
    NullValue { hint: &'static str },
    /// A [`TsDataFrame`] push received a column name already present. Frame
    /// column names are unique; the duplicate is rejected rather than
    /// silently shadowing the existing column.
    DuplicateColumn { name: String },
}

impl std::fmt::Display for TsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TsError::NotMonotonic { last, got } => {
                write!(f, "non-monotonic ts: tail={last}, got={got}")
            }
            TsError::NullValue { hint } => write!(f, "null value rejected: {hint}"),
            TsError::DuplicateColumn { name } => write!(f, "duplicate frame column: {name}"),
        }
    }
}

impl std::error::Error for TsError {}

/// Gate for "is this a present (non-null) observation". Implemented for every
/// value type the library ships; a custom value type implements it once (the
/// default treats every value as present). `push` rejects values that report
/// `false`.
pub trait TsValueKind {
    fn ts_is_present(&self) -> bool {
        true
    }
}

macro_rules! present_always {
    ($($t:ty),*) => { $( impl TsValueKind for $t {} )* };
}
present_always!(i64, i32, u64, u32, bool, String);

impl TsValueKind for f64 {
    fn ts_is_present(&self) -> bool {
        self.is_finite()
    }
}
impl TsValueKind for f32 {
    fn ts_is_present(&self) -> bool {
        self.is_finite()
    }
}

/// Numeric surface gate. A `TsSeries<T: TsNumeric>` lights up `min` / `max` /
/// `sum` / `mean` and their ranged variants. Non-numeric value types (Ohlc,
/// Curve, schemaless) still get the full time-query surface; extract a scalar
/// field first (`series.map(|o| o.close)`) to aggregate.
pub trait TsNumeric: Copy + PartialOrd {
    fn ts_zero() -> Self;
    fn ts_add(self, other: Self) -> Self;
    fn ts_to_f64(self) -> f64;
}

impl TsNumeric for f64 {
    fn ts_zero() -> Self {
        0.0
    }
    fn ts_add(self, other: Self) -> Self {
        self + other
    }
    fn ts_to_f64(self) -> f64 {
        self
    }
}
impl TsNumeric for i64 {
    fn ts_zero() -> Self {
        0
    }
    fn ts_add(self, other: Self) -> Self {
        self.wrapping_add(other)
    }
    fn ts_to_f64(self) -> f64 {
        self as f64
    }
}
impl TsNumeric for f32 {
    fn ts_zero() -> Self {
        0.0
    }
    fn ts_add(self, other: Self) -> Self {
        self + other
    }
    fn ts_to_f64(self) -> f64 {
        self as f64
    }
}

/// Marker for value types that get the columnar fast path (SIMD scans, Arrow
/// zero-copy, Gorilla compression) once those tiers land. Primitive scalars
/// implement it; compound + schemaless value types do not and fall back to
/// the in-memory chunk representation. The trait is intentionally minimal in
/// 0.6 - it gates which `T` the warm/cold columnar tiers accept; the encoder
/// surface arrives with `subms-gorilla-block`.
pub trait TsColumnar: Copy {}
impl TsColumnar for f64 {}
impl TsColumnar for f32 {}
impl TsColumnar for i64 {}
impl TsColumnar for i32 {}
impl TsColumnar for u64 {}
impl TsColumnar for u32 {}

/// Number of points a head chunk absorbs before sealing into a warm chunk.
/// A `(i64, f64)` head at this capacity is 1 MiB - fits L2, keeps `push` a
/// plain bounds-elided append, and amortises the seal to sub-nanosecond per
/// point.
pub(crate) const SEAL_CAP: usize = 65_536;
