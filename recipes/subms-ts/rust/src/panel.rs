//! [`TsPanel`] - a homogeneous, single-`T`, ts-aligned multi-series container
//! (an OHLCV bar set, an order book, a multi-symbol basket). Series live in
//! named slots; [`TsPanel::aligned`] walks every slot in lock-step by ts.
//! The same-type time panel: every column is a `TsSeries<T>`. For a
//! heterogeneous, per-column-typed container see [`crate::TsDataFrame`].

use std::iter::Peekable;

use crate::{TsAttrs, TsDep, TsPoint, TsSeries, TsTags};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TsPanelMetadata {
    pub name: String,
    pub tags: TsTags,
    pub attributes: TsAttrs,
    pub dependencies: Vec<TsDep>,
}

impl TsPanelMetadata {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Default::default()
        }
    }
}

/// A named subset of a panel's slots (e.g. `price` = open/high/low/close).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TsPanelGroup {
    pub name: String,
    pub series_names: Vec<String>,
}

struct Slot<T> {
    name: String,
    series: TsSeries<T>,
}

#[derive(Default)]
pub struct TsPanel<T> {
    meta: TsPanelMetadata,
    slots: Vec<Slot<T>>,
    groups: Vec<TsPanelGroup>,
}

impl<T: Clone> TsPanel<T> {
    pub fn new(meta: TsPanelMetadata) -> Self {
        Self {
            meta,
            slots: Vec::new(),
            groups: Vec::new(),
        }
    }

    pub fn metadata(&self) -> &TsPanelMetadata {
        &self.meta
    }

    /// Add or replace a slot. Insertion order is preserved + defines the
    /// column order of [`aligned`](Self::aligned) rows.
    pub fn add_series(&mut self, slot_name: impl Into<String>, series: TsSeries<T>) {
        let name = slot_name.into();
        if let Some(slot) = self.slots.iter_mut().find(|s| s.name == name) {
            slot.series = series;
        } else {
            self.slots.push(Slot { name, series });
        }
    }

    pub fn series(&self, slot_name: &str) -> Option<&TsSeries<T>> {
        self.slots
            .iter()
            .find(|s| s.name == slot_name)
            .map(|s| &s.series)
    }

    pub fn series_mut(&mut self, slot_name: &str) -> Option<&mut TsSeries<T>> {
        self.slots
            .iter_mut()
            .find(|s| s.name == slot_name)
            .map(|s| &mut s.series)
    }

    pub fn slot_names(&self) -> impl Iterator<Item = &str> {
        self.slots.iter().map(|s| s.name.as_str())
    }

    pub fn len(&self) -> usize {
        self.slots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    pub fn add_group(&mut self, group: TsPanelGroup) {
        self.groups.push(group);
    }

    pub fn group(&self, group_name: &str) -> Option<&TsPanelGroup> {
        self.groups.iter().find(|g| g.name == group_name)
    }

    pub fn series_in_group<'a>(
        &'a self,
        group_name: &'a str,
    ) -> impl Iterator<Item = (&'a str, &'a TsSeries<T>)> {
        let members = self
            .group(group_name)
            .map(|g| g.series_names.as_slice())
            .unwrap_or(&[]);
        self.slots
            .iter()
            .filter(move |s| members.iter().any(|m| m == &s.name))
            .map(|s| (s.name.as_str(), &s.series))
    }

    /// Lock-step view over every slot by ts. Each row is `(ts, values)` where
    /// `values[i]` is `Some` if slot `i` had a point at that ts, else `None`.
    pub fn aligned(&self) -> TsPanelAligned<'_, T> {
        let cursors: Vec<Peekable<Box<dyn Iterator<Item = TsPoint<T>> + '_>>> = self
            .slots
            .iter()
            .map(|s| {
                let it: Box<dyn Iterator<Item = TsPoint<T>>> = Box::new(s.series.iter());
                it.peekable()
            })
            .collect();
        TsPanelAligned { cursors }
    }

    // ---------- delete surface ----------

    pub fn drop(&mut self, slot_name: &str) -> Option<TsSeries<T>> {
        let idx = self.slots.iter().position(|s| s.name == slot_name)?;
        Some(self.slots.remove(idx).series)
    }

    pub fn remove_group(&mut self, group_name: &str) -> Option<TsPanelGroup> {
        let idx = self.groups.iter().position(|g| g.name == group_name)?;
        Some(self.groups.remove(idx))
    }

    pub fn delete_at(&mut self, slot_name: &str, ts: i64) -> Option<TsPoint<T>> {
        self.series_mut(slot_name).and_then(|s| s.delete_at(ts))
    }

    pub fn delete_range(&mut self, slot_name: &str, lo: i64, hi: i64) -> usize {
        self.series_mut(slot_name)
            .map(|s| s.delete_range(lo, hi))
            .unwrap_or(0)
    }

    pub fn truncate_before(&mut self, cutoff: i64) -> usize {
        self.slots
            .iter_mut()
            .map(|s| s.series.truncate_before(cutoff))
            .sum()
    }

    pub fn truncate_after(&mut self, cutoff: i64) -> usize {
        self.slots
            .iter_mut()
            .map(|s| s.series.truncate_after(cutoff))
            .sum()
    }

    pub fn retain_slots(&mut self, keep: impl Fn(&str, &TsSeries<T>) -> bool) -> usize {
        let before = self.slots.len();
        self.slots.retain(|s| keep(&s.name, &s.series));
        before - self.slots.len()
    }

    pub fn clear(&mut self) {
        self.slots.clear();
        self.groups.clear();
    }
}

/// Multi-way merge over a panel's slots, yielding aligned rows in ts order.
pub struct TsPanelAligned<'a, T> {
    cursors: Vec<Peekable<Box<dyn Iterator<Item = TsPoint<T>> + 'a>>>,
}

impl<T: Clone> Iterator for TsPanelAligned<'_, T> {
    type Item = (i64, Vec<Option<T>>);

    fn next(&mut self) -> Option<Self::Item> {
        let min_ts = self
            .cursors
            .iter_mut()
            .filter_map(|c| c.peek().map(|p| p.ts))
            .min()?;
        let row = self
            .cursors
            .iter_mut()
            .map(|c| match c.peek() {
                Some(p) if p.ts == min_ts => c.next().map(|p| p.value),
                _ => None,
            })
            .collect();
        Some((min_ts, row))
    }
}
