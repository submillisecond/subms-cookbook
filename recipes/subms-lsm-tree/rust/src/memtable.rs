use std::collections::BTreeMap;

/// In-memory buffer of pending writes, sorted by key. `None` represents a
/// tombstone - present in the map, marking the key as deleted.
pub(crate) struct Memtable {
    entries: BTreeMap<String, Option<Vec<u8>>>,
    approx_size_bytes: usize,
}

impl Memtable {
    pub(crate) fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
            approx_size_bytes: 0,
        }
    }

    pub(crate) fn put(&mut self, key: &str, value: Option<Vec<u8>>) {
        let new_val_cost = value.as_ref().map(|v| v.len()).unwrap_or(1);
        match self.entries.insert(key.to_string(), value) {
            None => self.approx_size_bytes += key.len() + new_val_cost,
            Some(prev) => {
                let prev_cost = prev.as_ref().map(|v| v.len()).unwrap_or(1);
                self.approx_size_bytes = self.approx_size_bytes + new_val_cost - prev_cost;
            }
        }
    }

    /// Returns `None` if the key is absent, `Some(None)` if tombstoned,
    /// `Some(Some(v))` if a value is present.
    pub(crate) fn get(&self, key: &str) -> Option<Option<&[u8]>> {
        self.entries.get(key).map(|opt| opt.as_deref())
    }

    pub(crate) fn approx_size_bytes(&self) -> usize {
        self.approx_size_bytes
    }

    pub(crate) fn entry_count(&self) -> usize {
        self.entries.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub(crate) fn sorted_entries(&self) -> impl Iterator<Item = (&str, Option<&[u8]>)> {
        self.entries.iter().map(|(k, v)| (k.as_str(), v.as_deref()))
    }

    /// Entries whose key is in `[lo, hi)` (either bound `None` = unbounded), in
    /// key order. Tombstones surface as `(key, None)`, resolved by the caller.
    pub(crate) fn range<'a>(
        &'a self,
        lo: Option<&str>,
        hi: Option<&str>,
    ) -> impl Iterator<Item = (&'a str, Option<&'a [u8]>)> {
        use std::ops::Bound;
        let low = lo.map(Bound::Included).unwrap_or(Bound::Unbounded);
        let high = hi.map(Bound::Excluded).unwrap_or(Bound::Unbounded);
        self.entries
            .range::<str, _>((low, high))
            .map(|(k, v)| (k.as_str(), v.as_deref()))
    }

    pub(crate) fn clear(&mut self) {
        self.entries.clear();
        self.approx_size_bytes = 0;
    }
}
