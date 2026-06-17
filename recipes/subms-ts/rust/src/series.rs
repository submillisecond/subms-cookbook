use crate::{SEAL_CAP, TsError, TsNumeric, TsPoint, TsSeriesMetadata, TsValueKind};

/// One SoA chunk: parallel timestamp + value columns. The head chunk is the
/// mutable tail; sealed chunks are the warm tier. Both share this shape so
/// the read path treats them uniformly.
#[derive(Clone, Debug)]
struct Chunk<T> {
    ts: Vec<i64>,
    val: Vec<T>,
}

impl<T: Clone> Chunk<T> {
    fn new() -> Self {
        Self {
            ts: Vec::new(),
            val: Vec::new(),
        }
    }

    fn with_capacity(cap: usize) -> Self {
        Self {
            ts: Vec::with_capacity(cap),
            val: Vec::with_capacity(cap),
        }
    }

    fn len(&self) -> usize {
        self.ts.len()
    }

    fn is_empty(&self) -> bool {
        self.ts.is_empty()
    }

    fn ts_min(&self) -> i64 {
        self.ts[0]
    }

    fn ts_max(&self) -> i64 {
        self.ts[self.ts.len() - 1]
    }

    /// First index whose ts >= target.
    fn first_ge(&self, target: i64) -> usize {
        self.ts.partition_point(|&t| t < target)
    }

    /// First index whose ts > target.
    fn first_gt(&self, target: i64) -> usize {
        self.ts.partition_point(|&t| t <= target)
    }

    fn point(&self, i: usize) -> TsPoint<T> {
        TsPoint {
            ts: self.ts[i],
            value: self.val[i].clone(),
        }
    }
}

/// A time-ordered sequence of [`TsPoint<T>`]. Non-decreasing in `ts`;
/// out-of-order or null inserts are rejected by [`TsSeries::push`].
///
/// Backed by a mutable SoA head chunk + a vec of sealed warm chunks. The
/// cold Gorilla tier plugs in behind [`TsRange`] via `subms-gorilla-block`
/// (Phase 2); a series under [`SEAL_CAP`] points lives entirely in the head.
#[derive(Clone, Debug)]
pub struct TsSeries<T> {
    warm: Vec<Chunk<T>>,
    head: Chunk<T>,
    len: usize,
    last_ts: Option<i64>,
    meta: Option<TsSeriesMetadata>,
}

impl<T: Clone> Default for TsSeries<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Clone> TsSeries<T> {
    pub fn new() -> Self {
        Self {
            warm: Vec::new(),
            head: Chunk::new(),
            len: 0,
            last_ts: None,
            meta: None,
        }
    }

    pub fn with_capacity(cap: usize) -> Self {
        Self {
            warm: Vec::new(),
            head: Chunk::with_capacity(cap.min(SEAL_CAP)),
            len: 0,
            last_ts: None,
            meta: None,
        }
    }

    /// Attach identity + schema + tags + deps. Optional - a bare series
    /// carries none and the ingest path never touches it.
    pub fn with_metadata(mut self, meta: TsSeriesMetadata) -> Self {
        self.meta = Some(meta);
        self
    }

    pub fn metadata(&self) -> Option<&TsSeriesMetadata> {
        self.meta.as_ref()
    }

    pub fn metadata_mut(&mut self) -> Option<&mut TsSeriesMetadata> {
        self.meta.as_mut()
    }

    pub fn set_metadata(&mut self, meta: TsSeriesMetadata) {
        self.meta = Some(meta);
    }

    /// Build from points already in non-decreasing ts order. Returns
    /// `NotMonotonic` / `NullValue` on the first offending point.
    pub fn from_points(points: Vec<TsPoint<T>>) -> Result<Self, TsError>
    where
        T: TsValueKind,
    {
        let mut s = Self::with_capacity(points.len());
        for p in points {
            s.push(p.ts, p.value)?;
        }
        Ok(s)
    }

    /// Append an observation. Rejects a ts earlier than the tail
    /// (`NotMonotonic`) and a null / non-finite value (`NullValue`).
    pub fn push(&mut self, ts: i64, value: T) -> Result<(), TsError>
    where
        T: TsValueKind,
    {
        if !value.ts_is_present() {
            return Err(TsError::NullValue {
                hint: "non-finite or null observation",
            });
        }
        if let Some(last) = self.last_ts
            && ts < last
        {
            return Err(TsError::NotMonotonic { last, got: ts });
        }
        self.head.ts.push(ts);
        self.head.val.push(value);
        self.len += 1;
        self.last_ts = Some(ts);
        if self.head.len() == SEAL_CAP {
            self.seal();
        }
        Ok(())
    }

    fn seal(&mut self) {
        let sealed = std::mem::replace(&mut self.head, Chunk::with_capacity(SEAL_CAP));
        self.warm.push(sealed);
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn first(&self) -> Option<TsPoint<T>> {
        if let Some(c) = self.warm.first() {
            return Some(c.point(0));
        }
        if !self.head.is_empty() {
            return Some(self.head.point(0));
        }
        None
    }

    pub fn last(&self) -> Option<TsPoint<T>> {
        if !self.head.is_empty() {
            return Some(self.head.point(self.head.len() - 1));
        }
        if let Some(c) = self.warm.last() {
            return Some(c.point(c.len() - 1));
        }
        None
    }

    /// Logical chunk list in time order: warm chunks then the head.
    fn chunks(&self) -> impl Iterator<Item = &Chunk<T>> {
        self.warm.iter().chain(std::iter::once(&self.head))
    }

    fn non_empty_chunks(&self) -> impl Iterator<Item = &Chunk<T>> {
        self.chunks().filter(|c| !c.is_empty())
    }

    /// Iterate every point in time order.
    pub fn iter(&self) -> impl Iterator<Item = TsPoint<T>> + '_ {
        self.non_empty_chunks()
            .flat_map(|c| (0..c.len()).map(move |i| c.point(i)))
    }

    // ---------- time queries ----------

    /// Exact match: the first point whose ts equals `target`.
    pub fn get_at(&self, target: i64) -> Option<TsPoint<T>> {
        for c in self.non_empty_chunks() {
            if target < c.ts_min() {
                return None;
            }
            if target > c.ts_max() {
                continue;
            }
            let i = c.first_ge(target);
            if i < c.len() && c.ts[i] == target {
                return Some(c.point(i));
            }
        }
        None
    }

    /// Largest-ts point with ts <= `target`.
    pub fn nearest_before(&self, target: i64) -> Option<TsPoint<T>> {
        let mut best: Option<&Chunk<T>> = None;
        for c in self.non_empty_chunks() {
            if c.ts_min() <= target {
                best = Some(c);
            } else {
                break;
            }
        }
        let c = best?;
        let gt = c.first_gt(target);
        if gt == 0 { None } else { Some(c.point(gt - 1)) }
    }

    /// Smallest-ts point with ts >= `target`.
    pub fn nearest_after(&self, target: i64) -> Option<TsPoint<T>> {
        for c in self.non_empty_chunks() {
            if c.ts_max() < target {
                continue;
            }
            let i = c.first_ge(target);
            if i < c.len() {
                return Some(c.point(i));
            }
        }
        None
    }

    /// Closest point by absolute ts distance (ties resolve to the earlier).
    pub fn nearest(&self, target: i64) -> Option<TsPoint<T>> {
        match (self.nearest_before(target), self.nearest_after(target)) {
            (Some(b), Some(a)) => {
                if (target - b.ts) <= (a.ts - target) {
                    Some(b)
                } else {
                    Some(a)
                }
            }
            (Some(b), None) => Some(b),
            (None, Some(a)) => Some(a),
            (None, None) => None,
        }
    }

    /// Inclusive `[lo, hi]` range as a lazy view over the underlying chunks.
    pub fn range(&self, lo: i64, hi: i64) -> TsRange<'_, T> {
        let mut spans: Vec<(&[i64], &[T])> = Vec::new();
        if lo <= hi {
            for c in self.non_empty_chunks() {
                if c.ts_max() < lo || c.ts_min() > hi {
                    continue;
                }
                let start = c.first_ge(lo);
                let end = c.first_gt(hi);
                if start < end {
                    spans.push((&c.ts[start..end], &c.val[start..end]));
                }
            }
        }
        TsRange {
            spans,
            chunk: 0,
            pos: 0,
        }
    }

    /// Contiguous value columns intersecting `[lo, hi]`, in time order. The
    /// numeric scans fold over these slices directly rather than the point
    /// iterator, which is what lets the `simd` kernels see flat `&[T]`.
    fn value_spans(&self, lo: i64, hi: i64) -> Vec<&[T]> {
        let mut spans: Vec<&[T]> = Vec::new();
        if lo <= hi {
            for c in self.non_empty_chunks() {
                if c.ts_max() < lo || c.ts_min() > hi {
                    continue;
                }
                let start = c.first_ge(lo);
                let end = c.first_gt(hi);
                if start < end {
                    spans.push(&c.val[start..end]);
                }
            }
        }
        spans
    }

    /// Every value column in time order (full-series scans).
    fn all_value_spans(&self) -> impl Iterator<Item = &[T]> {
        self.non_empty_chunks().map(|c| c.val.as_slice())
    }

    // ---------- delete surface ----------

    /// Remove + return the first point whose ts equals `target`.
    pub fn delete_at(&mut self, target: i64) -> Option<TsPoint<T>> {
        let n_warm = self.warm.len();
        for ci in 0..=n_warm {
            let c = if ci < n_warm {
                &mut self.warm[ci]
            } else {
                &mut self.head
            };
            if c.is_empty() || target < c.ts_min() || target > c.ts_max() {
                continue;
            }
            let i = c.first_ge(target);
            if i < c.len() && c.ts[i] == target {
                let ts = c.ts.remove(i);
                let value = c.val.remove(i);
                self.len -= 1;
                self.drop_empty_warm();
                self.recompute_last_ts();
                return Some(TsPoint { ts, value });
            }
        }
        None
    }

    /// Remove every point in `[lo, hi]`. Returns the count removed.
    pub fn delete_range(&mut self, lo: i64, hi: i64) -> usize {
        if lo > hi {
            return 0;
        }
        self.retain_points(|ts, _| ts < lo || ts > hi)
    }

    /// Remove every point whose value equals `target`.
    pub fn delete_by_value(&mut self, target: &T) -> usize
    where
        T: PartialEq,
    {
        self.retain_points(|_, v| v != target)
    }

    /// Remove every point whose value is in `[lo, hi]`.
    pub fn delete_value_range(&mut self, lo: &T, hi: &T) -> usize
    where
        T: PartialOrd,
    {
        self.retain_points(|_, v| v < lo || v > hi)
    }

    /// Remove every point not satisfying `keep`. Returns the count removed.
    pub fn retain(&mut self, mut keep: impl FnMut(&TsPoint<T>) -> bool) -> usize {
        self.retain_points(|ts, v| {
            keep(&TsPoint {
                ts,
                value: v.clone(),
            })
        })
    }

    /// Drop points before `cutoff` (keep ts >= cutoff). Returns count removed.
    pub fn truncate_before(&mut self, cutoff: i64) -> usize {
        self.retain_points(|ts, _| ts >= cutoff)
    }

    /// Drop points after `cutoff` (keep ts <= cutoff). Returns count removed.
    pub fn truncate_after(&mut self, cutoff: i64) -> usize {
        self.retain_points(|ts, _| ts <= cutoff)
    }

    pub fn pop_first(&mut self) -> Option<TsPoint<T>> {
        let first = self.first()?;
        self.delete_at(first.ts)
    }

    pub fn pop_last(&mut self) -> Option<TsPoint<T>> {
        let last = self.last()?;
        // Remove the last physical point, not merely the first ts-match.
        let c = if !self.head.is_empty() {
            &mut self.head
        } else {
            self.warm.last_mut()?
        };
        let i = c.len() - 1;
        let ts = c.ts.remove(i);
        let value = c.val.remove(i);
        self.len -= 1;
        self.drop_empty_warm();
        self.recompute_last_ts();
        debug_assert_eq!(ts, last.ts);
        Some(TsPoint { ts, value })
    }

    pub fn clear(&mut self) {
        self.warm.clear();
        self.head = Chunk::new();
        self.len = 0;
        self.last_ts = None;
    }

    /// Rebuild the series keeping only points satisfying `keep(ts, &val)`,
    /// preserving chunk geometry. Returns the count removed.
    fn retain_points(&mut self, mut keep: impl FnMut(i64, &T) -> bool) -> usize {
        let before = self.len;
        let mut warm: Vec<Chunk<T>> = Vec::new();
        let mut cur = Chunk::with_capacity(SEAL_CAP);
        let mut kept = 0usize;
        let old_warm = std::mem::take(&mut self.warm);
        let old_head = std::mem::replace(&mut self.head, Chunk::new());
        for c in old_warm.into_iter().chain(std::iter::once(old_head)) {
            for i in 0..c.len() {
                if keep(c.ts[i], &c.val[i]) {
                    cur.ts.push(c.ts[i]);
                    cur.val.push(c.val[i].clone());
                    kept += 1;
                    if cur.len() == SEAL_CAP {
                        warm.push(std::mem::replace(&mut cur, Chunk::with_capacity(SEAL_CAP)));
                    }
                }
            }
        }
        self.warm = warm;
        self.head = cur;
        self.len = kept;
        self.recompute_last_ts();
        before - kept
    }

    fn drop_empty_warm(&mut self) {
        self.warm.retain(|c| !c.is_empty());
    }

    fn recompute_last_ts(&mut self) {
        self.last_ts = self.last().map(|p| p.ts);
    }
}

impl<T: TsNumeric> TsSeries<T> {
    pub fn min(&self) -> Option<T> {
        fold_min(self.all_value_spans())
    }

    pub fn max(&self) -> Option<T> {
        fold_max(self.all_value_spans())
    }

    pub fn sum(&self) -> T {
        fold_sum(self.all_value_spans())
    }

    pub fn mean(&self) -> Option<f64> {
        if self.is_empty() {
            return None;
        }
        Some(self.sum().ts_to_f64() / self.len as f64)
    }

    pub fn min_point(&self) -> Option<TsPoint<T>> {
        self.iter()
            .reduce(|a, b| if b.value < a.value { b } else { a })
    }

    pub fn max_point(&self) -> Option<TsPoint<T>> {
        self.iter()
            .reduce(|a, b| if b.value > a.value { b } else { a })
    }

    pub fn range_min(&self, lo: i64, hi: i64) -> Option<T> {
        fold_min(self.value_spans(lo, hi).into_iter())
    }

    pub fn range_max(&self, lo: i64, hi: i64) -> Option<T> {
        fold_max(self.value_spans(lo, hi).into_iter())
    }

    pub fn range_sum(&self, lo: i64, hi: i64) -> T {
        fold_sum(self.value_spans(lo, hi).into_iter())
    }

    pub fn range_mean(&self, lo: i64, hi: i64) -> Option<f64> {
        let mut count = 0usize;
        let mut acc = T::ts_zero();
        for s in self.value_spans(lo, hi) {
            acc = acc.ts_add(kernels::sum_slice(s));
            count += s.len();
        }
        if count == 0 {
            None
        } else {
            Some(acc.ts_to_f64() / count as f64)
        }
    }
}

/// Lazy inclusive-range view over a [`TsSeries`]. Borrows the underlying
/// chunk columns and yields [`TsPoint<T>`] without materialising an
/// intermediate vec. The cold Gorilla tier (Phase 2) decodes into the same
/// span shape behind this view.
pub struct TsRange<'a, T> {
    spans: Vec<(&'a [i64], &'a [T])>,
    chunk: usize,
    pos: usize,
}

impl<T: Clone> Iterator for TsRange<'_, T> {
    type Item = TsPoint<T>;

    fn next(&mut self) -> Option<Self::Item> {
        while self.chunk < self.spans.len() {
            let (ts, val) = self.spans[self.chunk];
            if self.pos < ts.len() {
                let p = TsPoint {
                    ts: ts[self.pos],
                    value: val[self.pos].clone(),
                };
                self.pos += 1;
                return Some(p);
            }
            self.chunk += 1;
            self.pos = 0;
        }
        None
    }
}

// ---------- aggregate folds over contiguous value columns ----------

fn fold_sum<'a, T: TsNumeric + 'a>(spans: impl Iterator<Item = &'a [T]>) -> T {
    let mut acc = T::ts_zero();
    for s in spans {
        acc = acc.ts_add(kernels::sum_slice(s));
    }
    acc
}

fn fold_min<'a, T: TsNumeric + 'a>(spans: impl Iterator<Item = &'a [T]>) -> Option<T> {
    let mut best: Option<T> = None;
    for s in spans {
        if let Some(m) = kernels::min_slice(s) {
            best = Some(match best {
                Some(b) if b < m => b,
                _ => m,
            });
        }
    }
    best
}

fn fold_max<'a, T: TsNumeric + 'a>(spans: impl Iterator<Item = &'a [T]>) -> Option<T> {
    let mut best: Option<T> = None;
    for s in spans {
        if let Some(m) = kernels::max_slice(s) {
            best = Some(match best {
                Some(b) if b > m => b,
                _ => m,
            });
        }
    }
    best
}

/// Per-slice reduction kernels. The `simd` feature swaps the straight scalar
/// fold for an 8-lane-unrolled form: the eight independent accumulators are
/// separate dependency chains, so the optimiser is free to pack them into
/// vector registers (stable codegen, no `std::simd`, no dep). min/max stay
/// exact - the series rejects NaN on push, so reordering a comparison
/// reduction is safe. f64 sum reorders by lane, so a `simd` sum can differ
/// from the scalar sum by an ULP; that is the only observable difference.
mod kernels {
    use crate::TsNumeric;

    #[cfg(feature = "simd")]
    const LANES: usize = 8;

    #[cfg(not(feature = "simd"))]
    pub(super) fn sum_slice<T: TsNumeric>(s: &[T]) -> T {
        let mut acc = T::ts_zero();
        for &v in s {
            acc = acc.ts_add(v);
        }
        acc
    }

    #[cfg(feature = "simd")]
    pub(super) fn sum_slice<T: TsNumeric>(s: &[T]) -> T {
        let mut lanes = [T::ts_zero(); LANES];
        let mut chunks = s.chunks_exact(LANES);
        for c in &mut chunks {
            for k in 0..LANES {
                lanes[k] = lanes[k].ts_add(c[k]);
            }
        }
        let mut acc = T::ts_zero();
        for l in lanes {
            acc = acc.ts_add(l);
        }
        for &v in chunks.remainder() {
            acc = acc.ts_add(v);
        }
        acc
    }

    #[cfg(not(feature = "simd"))]
    pub(super) fn min_slice<T: TsNumeric>(s: &[T]) -> Option<T> {
        let mut it = s.iter().copied();
        let mut m = it.next()?;
        for v in it {
            if v < m {
                m = v;
            }
        }
        Some(m)
    }

    #[cfg(feature = "simd")]
    pub(super) fn min_slice<T: TsNumeric>(s: &[T]) -> Option<T> {
        if s.len() < LANES {
            let mut it = s.iter().copied();
            let mut m = it.next()?;
            for v in it {
                if v < m {
                    m = v;
                }
            }
            return Some(m);
        }
        let mut chunks = s.chunks_exact(LANES);
        let mut lanes: [T; LANES] = chunks.next().unwrap().try_into().unwrap();
        for c in &mut chunks {
            for k in 0..LANES {
                if c[k] < lanes[k] {
                    lanes[k] = c[k];
                }
            }
        }
        let mut m = lanes[0];
        for &l in &lanes[1..] {
            if l < m {
                m = l;
            }
        }
        for &v in chunks.remainder() {
            if v < m {
                m = v;
            }
        }
        Some(m)
    }

    #[cfg(not(feature = "simd"))]
    pub(super) fn max_slice<T: TsNumeric>(s: &[T]) -> Option<T> {
        let mut it = s.iter().copied();
        let mut m = it.next()?;
        for v in it {
            if v > m {
                m = v;
            }
        }
        Some(m)
    }

    #[cfg(feature = "simd")]
    pub(super) fn max_slice<T: TsNumeric>(s: &[T]) -> Option<T> {
        if s.len() < LANES {
            let mut it = s.iter().copied();
            let mut m = it.next()?;
            for v in it {
                if v > m {
                    m = v;
                }
            }
            return Some(m);
        }
        let mut chunks = s.chunks_exact(LANES);
        let mut lanes: [T; LANES] = chunks.next().unwrap().try_into().unwrap();
        for c in &mut chunks {
            for k in 0..LANES {
                if c[k] > lanes[k] {
                    lanes[k] = c[k];
                }
            }
        }
        let mut m = lanes[0];
        for &l in &lanes[1..] {
            if l > m {
                m = l;
            }
        }
        for &v in chunks.remainder() {
            if v > m {
                m = v;
            }
        }
        Some(m)
    }
}
