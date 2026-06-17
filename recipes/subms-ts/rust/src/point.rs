/// A single observation: an `i64`-nanosecond timestamp paired with a value.
///
/// The value type is generic - `f64` for the scalar fast path, `Ohlc` /
/// `Curve` / `Surface` for compound shapes, or any custom type. `TsPoint` is
/// the iteration item + the input to `push`; it is not the storage layout
/// (the series stores SoA columns internally).
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct TsPoint<T> {
    pub ts: i64,
    pub value: T,
}

impl<T> TsPoint<T> {
    pub fn new(ts: i64, value: T) -> Self {
        Self { ts, value }
    }
}
