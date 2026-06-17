//! `chrono` conversions, behind the `datetime` feature. The core stores
//! `i64` nanoseconds; these helpers let research code read + write real
//! `DateTime<Utc>` values without the hot path paying for it.

use chrono::{DateTime, Utc};

use crate::{TsError, TsPoint, TsRange, TsSeries, TsValueKind};

impl<T> TsPoint<T> {
    pub fn at_datetime(dt: DateTime<Utc>, value: T) -> Self {
        Self {
            ts: dt.timestamp_nanos_opt().unwrap_or(0),
            value,
        }
    }

    pub fn datetime(&self) -> DateTime<Utc> {
        DateTime::from_timestamp_nanos(self.ts)
    }
}

impl<T: Clone> TsSeries<T> {
    pub fn push_datetime(&mut self, dt: DateTime<Utc>, value: T) -> Result<(), TsError>
    where
        T: TsValueKind,
    {
        self.push(dt.timestamp_nanos_opt().unwrap_or(0), value)
    }

    pub fn range_datetime(&self, lo: DateTime<Utc>, hi: DateTime<Utc>) -> TsRange<'_, T> {
        self.range(
            lo.timestamp_nanos_opt().unwrap_or(i64::MIN),
            hi.timestamp_nanos_opt().unwrap_or(i64::MAX),
        )
    }

    pub fn nearest_datetime(&self, dt: DateTime<Utc>) -> Option<TsPoint<T>> {
        self.nearest(dt.timestamp_nanos_opt().unwrap_or(0))
    }
}
