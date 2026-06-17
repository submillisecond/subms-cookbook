//! `subms-ts-categorical` - the optimizer that turns expensive string
//! group/join keys into cheap `u32` integer compares.
//!
//! Two standalone types, neither of which touches the subms-ts core on its
//! hot path:
//!
//! - [`TsStringInterner`] - a `&str -> u32` table. Equal strings collapse to
//!   one densely-assigned id, so a downstream comparison is an int compare
//!   instead of a byte-for-byte string compare.
//! - [`TsDictColumn`] - a dictionary-encoded string column: a `Vec<u32>` of
//!   codes plus a `Vec<String>` dictionary. A group-by / join keys on
//!   `codes()` (an int) rather than re-hashing each string, and the dictionary
//!   amortises storage for a high-duplication column (think a `symbol` column
//!   that is one of a handful of tickers).
//!
//! The bridge to the analytical layer is [`encode_str_column`], which lifts a
//! `TsColumn::Str` straight off a `TsDataFrame` into a [`TsDictColumn`] while
//! the core types stay independent.
//!
//! ```
//! use subms_ts_categorical::TsDictColumn;
//!
//! let symbols = ["AAPL", "MSFT", "AAPL", "AAPL", "MSFT"];
//! let dict = TsDictColumn::from_strs(symbols);
//! assert_eq!(dict.len(), 5);
//! assert_eq!(dict.cardinality(), 2); // two distinct tickers
//! // equal strings share a code, so a group-by keys on u32, not String.
//! assert_eq!(dict.codes()[0], dict.codes()[2]);
//! assert_eq!(dict.decode_at(1), Some("MSFT"));
//! ```

use std::collections::HashMap;

use subms_ts::{TsColumn, TsSeries};

#[cfg(feature = "harness")]
pub mod recipe;

/// A stable `&str -> u32` table. The first sight of a string assigns it the
/// next dense id (0, 1, 2, ... in first-seen order); every later sight of the
/// same string returns that id. Ids are never reused and never change, so a
/// code captured early stays valid for the interner's lifetime.
///
/// Backed by a `HashMap<String, u32>` for the forward lookup and a
/// `Vec<String>` for the reverse: `resolve(id)` is an O(1) index, `intern` is
/// an amortised O(1) hash probe.
///
/// The table is exact and unbounded - it holds one `String` per distinct value
/// for the reverse map and grows with the distinct-string count. That is the
/// right call for the bounded-alphabet case this recipe targets (a `symbol` /
/// `region` / `status` column drawn from a small fixed set). An unbounded
/// distinct stream needs a bounded / evicting variant; see the recipe writeup's
/// non-claims.
#[derive(Clone, Debug, Default)]
pub struct TsStringInterner {
    forward: HashMap<String, u32>,
    reverse: Vec<String>,
}

impl TsStringInterner {
    pub fn new() -> Self {
        Self {
            forward: HashMap::new(),
            reverse: Vec::new(),
        }
    }

    pub fn with_capacity(cap: usize) -> Self {
        Self {
            forward: HashMap::with_capacity(cap),
            reverse: Vec::with_capacity(cap),
        }
    }

    /// Return the stable id for `s`, assigning a fresh dense id on first sight.
    /// A repeat of the same string always returns the same id.
    pub fn intern(&mut self, s: &str) -> u32 {
        if let Some(&id) = self.forward.get(s) {
            return id;
        }
        let id = self.reverse.len() as u32;
        self.reverse.push(s.to_string());
        self.forward.insert(s.to_string(), id);
        id
    }

    /// The string for `id`, or `None` if no such id has been assigned.
    pub fn resolve(&self, id: u32) -> Option<&str> {
        self.reverse.get(id as usize).map(String::as_str)
    }

    /// The id for `s` without assigning one. `None` if `s` was never interned.
    pub fn get(&self, s: &str) -> Option<u32> {
        self.forward.get(s).copied()
    }

    pub fn contains(&self, s: &str) -> bool {
        self.forward.contains_key(s)
    }

    /// Distinct strings interned so far. Also the next id that `intern` would
    /// assign to a never-before-seen string.
    pub fn len(&self) -> usize {
        self.reverse.len()
    }

    pub fn is_empty(&self) -> bool {
        self.reverse.is_empty()
    }

    /// The interned strings in id order: index `i` is the string with id `i`.
    pub fn strings(&self) -> &[String] {
        &self.reverse
    }
}

/// A dictionary-encoded string column. `codes[i]` indexes into `dict`, so
/// `dict[codes[i]]` is the i-th logical string. Equal strings share one code,
/// which is the whole point: a group-by / join over [`codes`](Self::codes)
/// compares `u32`s, and a high-duplication column shrinks to one `String` per
/// distinct value plus a flat `u32` per row.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TsDictColumn {
    codes: Vec<u32>,
    dict: Vec<String>,
}

impl TsDictColumn {
    /// Encode any iterator of string-likes. Distinct values enter the
    /// dictionary in first-seen order; codes follow the input order.
    pub fn from_strs<I, S>(values: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut interner = TsStringInterner::new();
        let mut codes = Vec::new();
        for v in values {
            codes.push(interner.intern(v.as_ref()));
        }
        Self {
            codes,
            dict: interner.into_strings(),
        }
    }

    /// Encode a string time series. The ts axis is discarded - this is a value
    /// column optimizer; round-tripping via [`to_series`](Self::to_series)
    /// re-emits dense `0..len` timestamps, not the originals.
    pub fn encode(series: &TsSeries<String>) -> Self {
        Self::from_strs(series.iter().map(|p| p.value))
    }

    /// The i-th string, or `None` when `i` is out of range.
    pub fn decode_at(&self, i: usize) -> Option<&str> {
        let code = *self.codes.get(i)?;
        self.dict.get(code as usize).map(String::as_str)
    }

    /// The per-row code array. This is the key surface: a downstream operator
    /// groups / joins on these `u32`s instead of the strings.
    pub fn codes(&self) -> &[u32] {
        &self.codes
    }

    /// The dictionary: `dict[code]` is the string for `code`.
    pub fn dict(&self) -> &[String] {
        &self.dict
    }

    /// Distinct strings in the column (the dictionary size).
    pub fn cardinality(&self) -> usize {
        self.dict.len()
    }

    /// Logical row count (the code array length).
    pub fn len(&self) -> usize {
        self.codes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.codes.is_empty()
    }

    /// The string for `code` straight out of the dictionary, bypassing the
    /// per-row indirection.
    pub fn lookup(&self, code: u32) -> Option<&str> {
        self.dict.get(code as usize).map(String::as_str)
    }

    /// Decode back to a string series with dense `0..len` timestamps. The
    /// values round-trip the original column exactly; the ts axis does not (it
    /// was never stored).
    pub fn to_series(&self) -> TsSeries<String> {
        let mut s = TsSeries::with_capacity(self.codes.len());
        for (i, &code) in self.codes.iter().enumerate() {
            let v = self.dict[code as usize].clone();
            let _ = s.push(i as i64, v);
        }
        s
    }

    /// Decode the whole column into a plain `Vec<String>` in row order.
    pub fn to_vec(&self) -> Vec<String> {
        self.codes
            .iter()
            .map(|&code| self.dict[code as usize].clone())
            .collect()
    }
}

impl TsStringInterner {
    /// Consume the interner, returning the reverse table (the dictionary in id
    /// order). Used by [`TsDictColumn::from_strs`] to hand the built dictionary
    /// straight to the column without a clone.
    fn into_strings(self) -> Vec<String> {
        self.reverse
    }
}

/// Bridge to the analytical layer: dictionary-encode a `TsColumn::Str` taken
/// off a `TsDataFrame`. Returns `None` when the column is not a string column,
/// so a caller can probe a frame's column without matching the variant by hand.
pub fn encode_str_column(col: &TsColumn) -> Option<TsDictColumn> {
    col.as_str().map(TsDictColumn::encode)
}
