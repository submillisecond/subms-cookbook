//! `subms-zone-map` - a per-block min/max/count index that lets a query
//! planner skip blocks it cannot touch. For a predicate like
//! `(ts in [t1, t2] AND value > X)`, a block whose `[ts_min, ts_max]` misses
//! the window or whose `[value_min, value_max]` cannot satisfy the value
//! test is pruned without reading its body - the predicate-pushdown step
//! that lets a TSDB scan terabytes at memory-bandwidth rates.
//!
//! ```
//! use subms_zone_map::{TsZoneMap, TsValuePredicate, TsValueOp};
//! use subms_gorilla_block::TsGorillaBlock;
//!
//! let mut z = TsZoneMap::new();
//! let mut b = TsGorillaBlock::new();
//! for i in 0..100 { b.append(1_000 + i, i as f64); }
//! z.observe(7, &b);
//!
//! // window misses the block -> pruned
//! assert!(z.candidates(50_000, 60_000, None).is_empty());
//! // value predicate value > 200 cannot hold (max is 99) -> pruned
//! let pred = TsValuePredicate::new(TsValueOp::Gt, 200.0);
//! assert!(z.candidates(1_000, 1_099, Some(pred)).is_empty());
//! // satisfiable -> candidate
//! assert_eq!(z.candidates(1_000, 1_099, None), vec![7]);
//! ```

use subms_gorilla_block::TsGorillaBlock;

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum TsValueOp {
    Lt,
    Le,
    Eq,
    Ge,
    Gt,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct TsValuePredicate {
    pub op: TsValueOp,
    pub rhs: f64,
}

impl TsValuePredicate {
    pub fn new(op: TsValueOp, rhs: f64) -> Self {
        Self { op, rhs }
    }

    /// Could ANY value in `[value_min, value_max]` satisfy the predicate?
    /// Pruning is conservative: a `false` means "definitely cannot", so the
    /// block is safe to skip; a `true` means "maybe", so the block is read.
    fn satisfiable(&self, value_min: f64, value_max: f64) -> bool {
        match self.op {
            TsValueOp::Lt => value_min < self.rhs,
            TsValueOp::Le => value_min <= self.rhs,
            TsValueOp::Gt => value_max > self.rhs,
            TsValueOp::Ge => value_max >= self.rhs,
            TsValueOp::Eq => value_min <= self.rhs && self.rhs <= value_max,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct TsZone {
    pub block_id: u64,
    pub ts_min: i64,
    pub ts_max: i64,
    pub value_min: f64,
    pub value_max: f64,
    pub count: u32,
}

/// Per-block summary index. `observe` records one zone per block; `candidates`
/// returns the block ids a query must actually read.
#[derive(Clone, Debug, Default)]
pub struct TsZoneMap {
    zones: Vec<TsZone>,
}

impl TsZoneMap {
    pub fn new() -> Self {
        Self { zones: Vec::new() }
    }

    pub fn with_capacity(cap: usize) -> Self {
        Self {
            zones: Vec::with_capacity(cap),
        }
    }

    /// Record the zone for a Gorilla block (reads only its `stats`, never the
    /// body). Empty blocks are skipped.
    pub fn observe(&mut self, block_id: u64, block: &TsGorillaBlock) {
        if block.is_empty() {
            return;
        }
        let s = block.stats();
        self.zones.push(TsZone {
            block_id,
            ts_min: s.ts_min,
            ts_max: s.ts_max,
            value_min: s.value_min,
            value_max: s.value_max,
            count: s.count,
        });
    }

    /// Record a zone directly (when the block lives elsewhere or stats are
    /// already known).
    pub fn observe_zone(&mut self, zone: TsZone) {
        self.zones.push(zone);
    }

    /// Block ids whose `[ts_min, ts_max]` overlaps `[ts_lo, ts_hi]` and, if a
    /// value predicate is given, whose value range could satisfy it. Order
    /// preserved (observation order).
    pub fn candidates(
        &self,
        ts_lo: i64,
        ts_hi: i64,
        value_pred: Option<TsValuePredicate>,
    ) -> Vec<u64> {
        if ts_lo > ts_hi {
            return Vec::new();
        }
        self.zones
            .iter()
            .filter(|z| z.ts_max >= ts_lo && z.ts_min <= ts_hi)
            .filter(|z| match value_pred {
                None => true,
                Some(p) => p.satisfiable(z.value_min, z.value_max),
            })
            .map(|z| z.block_id)
            .collect()
    }

    pub fn zones(&self) -> &[TsZone] {
        &self.zones
    }

    pub fn len(&self) -> usize {
        self.zones.len()
    }

    pub fn is_empty(&self) -> bool {
        self.zones.is_empty()
    }

    pub fn clear(&mut self) {
        self.zones.clear();
    }
}

#[cfg(feature = "harness")]
pub mod recipe;
