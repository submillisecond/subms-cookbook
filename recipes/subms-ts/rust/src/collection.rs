//! [`TsCollection`] - a flat by-id registry of many series. The multi-tenant
//! scratch space: fast lookup by id / name / tag, bulk delete by tag /
//! predicate. For a coherent multi-series concept (OHLCV bars, an order
//! book) use [`crate::TsPanel`] instead.

use std::collections::HashMap;

use crate::{TsError, TsNumeric, TsSeries, TsSeriesMetadata, TsValueKind};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TsCollectionError {
    DuplicateId(u64),
    DuplicateName(String),
    UnknownId(u64),
    Ingest(TsError),
}

impl std::fmt::Display for TsCollectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TsCollectionError::DuplicateId(id) => write!(f, "duplicate series id {id}"),
            TsCollectionError::DuplicateName(n) => write!(f, "duplicate series name {n}"),
            TsCollectionError::UnknownId(id) => write!(f, "unknown series id {id}"),
            TsCollectionError::Ingest(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for TsCollectionError {}

/// Cross-series aggregation selector.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum TsAgg {
    Sum,
    Min,
    Max,
    Mean,
    Count,
}

#[derive(Clone, Debug, Default)]
pub struct TsCollection<T> {
    series: HashMap<u64, TsSeries<T>>,
    names: HashMap<String, u64>,
}

impl<T: Clone> TsCollection<T> {
    pub fn new() -> Self {
        Self {
            series: HashMap::new(),
            names: HashMap::new(),
        }
    }

    /// Register an empty series under its metadata id + name. Returns the id.
    pub fn register(&mut self, meta: TsSeriesMetadata) -> Result<u64, TsCollectionError> {
        let id = meta.id;
        if self.series.contains_key(&id) {
            return Err(TsCollectionError::DuplicateId(id));
        }
        if !meta.name.is_empty() && self.names.contains_key(&meta.name) {
            return Err(TsCollectionError::DuplicateName(meta.name));
        }
        if !meta.name.is_empty() {
            self.names.insert(meta.name.clone(), id);
        }
        self.series.insert(id, TsSeries::new().with_metadata(meta));
        Ok(id)
    }

    pub fn push(&mut self, id: u64, ts: i64, value: T) -> Result<(), TsCollectionError>
    where
        T: TsValueKind,
    {
        let s = self
            .series
            .get_mut(&id)
            .ok_or(TsCollectionError::UnknownId(id))?;
        s.push(ts, value).map_err(TsCollectionError::Ingest)
    }

    pub fn get(&self, id: u64) -> Option<&TsSeries<T>> {
        self.series.get(&id)
    }

    pub fn get_mut(&mut self, id: u64) -> Option<&mut TsSeries<T>> {
        self.series.get_mut(&id)
    }

    pub fn by_name(&self, name: &str) -> Option<&TsSeries<T>> {
        self.names.get(name).and_then(|id| self.series.get(id))
    }

    pub fn by_tag<'a>(
        &'a self,
        key: &'a str,
        value: &'a str,
    ) -> impl Iterator<Item = &'a TsSeries<T>> {
        self.series.values().filter(move |s| {
            s.metadata()
                .map(|m| m.tags.get(key).map(|v| v == value).unwrap_or(false))
                .unwrap_or(false)
        })
    }

    pub fn ids(&self) -> impl Iterator<Item = u64> + '_ {
        self.series.keys().copied()
    }

    pub fn series(&self) -> impl Iterator<Item = &TsSeries<T>> {
        self.series.values()
    }

    pub fn len(&self) -> usize {
        self.series.len()
    }

    pub fn is_empty(&self) -> bool {
        self.series.is_empty()
    }

    pub fn contains(&self, id: u64) -> bool {
        self.series.contains_key(&id)
    }

    // ---------- delete surface ----------

    pub fn deregister(&mut self, id: u64) -> Option<TsSeries<T>> {
        let s = self.series.remove(&id)?;
        if let Some(m) = s.metadata() {
            self.names.remove(&m.name);
        }
        Some(s)
    }

    pub fn delete_at(&mut self, id: u64, ts: i64) -> Option<crate::TsPoint<T>> {
        self.series.get_mut(&id).and_then(|s| s.delete_at(ts))
    }

    pub fn delete_range(&mut self, id: u64, lo: i64, hi: i64) -> usize {
        self.series
            .get_mut(&id)
            .map(|s| s.delete_range(lo, hi))
            .unwrap_or(0)
    }

    pub fn truncate_before(&mut self, cutoff: i64) -> usize {
        self.series
            .values_mut()
            .map(|s| s.truncate_before(cutoff))
            .sum()
    }

    pub fn truncate_after(&mut self, cutoff: i64) -> usize {
        self.series
            .values_mut()
            .map(|s| s.truncate_after(cutoff))
            .sum()
    }

    /// Drop a point at `ts` from every series matching `key=value`.
    pub fn delete_at_by_tag(&mut self, key: &str, value: &str, ts: i64) -> usize {
        let ids = self.matching_ids(key, value);
        ids.into_iter()
            .filter(|id| {
                self.series
                    .get_mut(id)
                    .and_then(|s| s.delete_at(ts))
                    .is_some()
            })
            .count()
    }

    /// Drop the `[lo, hi]` range from every series matching `key=value`.
    pub fn delete_range_by_tag(&mut self, key: &str, value: &str, lo: i64, hi: i64) -> usize {
        let ids = self.matching_ids(key, value);
        ids.into_iter()
            .map(|id| {
                self.series
                    .get_mut(&id)
                    .map(|s| s.delete_range(lo, hi))
                    .unwrap_or(0)
            })
            .sum()
    }

    /// Deregister + return every series matching `key=value`.
    pub fn evict_by_tag(&mut self, key: &str, value: &str) -> Vec<TsSeries<T>> {
        self.matching_ids(key, value)
            .into_iter()
            .filter_map(|id| self.deregister(id))
            .collect()
    }

    /// Deregister + return every series whose metadata satisfies `pred`.
    pub fn evict_where(&mut self, pred: impl Fn(&TsSeriesMetadata) -> bool) -> Vec<TsSeries<T>> {
        let ids: Vec<u64> = self
            .series
            .iter()
            .filter(|(_, s)| s.metadata().map(&pred).unwrap_or(false))
            .map(|(id, _)| *id)
            .collect();
        ids.into_iter()
            .filter_map(|id| self.deregister(id))
            .collect()
    }

    pub fn clear(&mut self) {
        self.series.clear();
        self.names.clear();
    }

    fn matching_ids(&self, key: &str, value: &str) -> Vec<u64> {
        self.series
            .iter()
            .filter(|(_, s)| {
                s.metadata()
                    .map(|m| m.tags.get(key).map(|v| v == value).unwrap_or(false))
                    .unwrap_or(false)
            })
            .map(|(id, _)| *id)
            .collect()
    }
}

impl<T: TsNumeric> TsCollection<T> {
    /// Aggregate the latest value as-of `ts` (each series' `nearest_before`)
    /// across the whole collection.
    pub fn aggregate_at(&self, ts: i64, agg: TsAgg) -> Option<f64> {
        self.fold_values(
            agg,
            self.series
                .values()
                .filter_map(|s| s.nearest_before(ts))
                .map(|p| p.value),
        )
    }

    /// Same, restricted to series matching `key=value`.
    pub fn aggregate_at_by_tag(&self, key: &str, value: &str, ts: i64, agg: TsAgg) -> Option<f64> {
        let vals = self
            .by_tag(key, value)
            .filter_map(|s| s.nearest_before(ts))
            .map(|p| p.value);
        self.fold_values(agg, vals)
    }

    fn fold_values(&self, agg: TsAgg, vals: impl Iterator<Item = T>) -> Option<f64> {
        let mut count = 0usize;
        let mut acc = T::ts_zero();
        let mut min: Option<T> = None;
        let mut max: Option<T> = None;
        for v in vals {
            count += 1;
            acc = acc.ts_add(v);
            min = Some(match min {
                Some(m) if m < v => m,
                _ => v,
            });
            max = Some(match max {
                Some(m) if m > v => m,
                _ => v,
            });
        }
        if count == 0 {
            return None;
        }
        Some(match agg {
            TsAgg::Sum => acc.ts_to_f64(),
            TsAgg::Min => min.unwrap().ts_to_f64(),
            TsAgg::Max => max.unwrap().ts_to_f64(),
            TsAgg::Mean => acc.ts_to_f64() / count as f64,
            TsAgg::Count => count as f64,
        })
    }
}
