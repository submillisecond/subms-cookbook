//! [`TsDataFrame`] - a heterogeneous, per-column-typed time-series container,
//! the analytical foundation the rest of the arc builds on. A [`TsPanel<T>`]
//! is homogeneous (every column shares one `T`); a frame is a bag of typed
//! columns where each column carries its own type.
//!
//! The genericity lives on the SERIES, not the cell: a [`TsColumn`] is a
//! type-erased [`TsSeries<T>`], one variant per supported `T`. Columnar scans
//! stay unboxed - an f64 column is a real `TsSeries<f64>`, not a vec of boxed
//! enums. A stored column never holds nulls (the series rejects them on push);
//! nulls are a derived surface, appearing only as `None` in the row-aligned
//! [`aligned`](TsDataFrame::aligned) view where one column has a gap.

use crate::{TsError, TsSeries, TsValue};

/// The element type of a [`TsColumn`]. `Value` is the unstructured escape
/// hatch (a column of [`TsValue`] documents).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum TsDataType {
    F64,
    I64,
    Bool,
    Str,
    Value,
}

/// A type-erased column: a typed time series. The variant carries the element
/// type, so a scan reaches the underlying homogeneous `TsSeries<T>` unboxed.
pub enum TsColumn {
    F64(TsSeries<f64>),
    I64(TsSeries<i64>),
    Bool(TsSeries<bool>),
    Str(TsSeries<String>),
    Value(TsSeries<TsValue>),
}

impl TsColumn {
    pub fn data_type(&self) -> TsDataType {
        match self {
            TsColumn::F64(_) => TsDataType::F64,
            TsColumn::I64(_) => TsDataType::I64,
            TsColumn::Bool(_) => TsDataType::Bool,
            TsColumn::Str(_) => TsDataType::Str,
            TsColumn::Value(_) => TsDataType::Value,
        }
    }

    pub fn len(&self) -> usize {
        match self {
            TsColumn::F64(s) => s.len(),
            TsColumn::I64(s) => s.len(),
            TsColumn::Bool(s) => s.len(),
            TsColumn::Str(s) => s.len(),
            TsColumn::Value(s) => s.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn as_f64(&self) -> Option<&TsSeries<f64>> {
        match self {
            TsColumn::F64(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<&TsSeries<i64>> {
        match self {
            TsColumn::I64(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<&TsSeries<bool>> {
        match self {
            TsColumn::Bool(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&TsSeries<String>> {
        match self {
            TsColumn::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_value(&self) -> Option<&TsSeries<TsValue>> {
        match self {
            TsColumn::Value(s) => Some(s),
            _ => None,
        }
    }

    /// Dynamic single-cell read at an exact ts, boxed into [`TsValue`]. Returns
    /// `None` when the column has no point at `ts`.
    pub fn get(&self, ts: i64) -> Option<TsValue> {
        match self {
            TsColumn::F64(s) => s.get_at(ts).map(|p| TsValue::F64(p.value)),
            TsColumn::I64(s) => s.get_at(ts).map(|p| TsValue::I64(p.value)),
            TsColumn::Bool(s) => s.get_at(ts).map(|p| TsValue::Bool(p.value)),
            TsColumn::Str(s) => s.get_at(ts).map(|p| TsValue::Str(p.value)),
            TsColumn::Value(s) => s.get_at(ts).map(|p| p.value),
        }
    }

    /// Iterate the column as `(ts, TsValue)` pairs in time order. Drives the
    /// frame's aligned-view merge without a per-variant code path.
    fn iter_boxed(&self) -> Box<dyn Iterator<Item = (i64, TsValue)> + '_> {
        match self {
            TsColumn::F64(s) => Box::new(s.iter().map(|p| (p.ts, TsValue::F64(p.value)))),
            TsColumn::I64(s) => Box::new(s.iter().map(|p| (p.ts, TsValue::I64(p.value)))),
            TsColumn::Bool(s) => Box::new(s.iter().map(|p| (p.ts, TsValue::Bool(p.value)))),
            TsColumn::Str(s) => Box::new(s.iter().map(|p| (p.ts, TsValue::Str(p.value)))),
            TsColumn::Value(s) => Box::new(s.iter().map(|p| (p.ts, p.value))),
        }
    }
}

/// One named, typed column slot in a [`TsFrameSchema`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TsField {
    pub name: String,
    pub data_type: TsDataType,
}

impl TsField {
    pub fn new(name: impl Into<String>, data_type: TsDataType) -> Self {
        Self {
            name: name.into(),
            data_type,
        }
    }
}

/// The ordered (name, type) shape of a [`TsDataFrame`]. Distinct from the
/// per-series `TsSchema`, which describes a single series' value layout; this
/// describes a whole frame's columns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TsFrameSchema {
    pub fields: Vec<TsField>,
}

/// A heterogeneous bag of named, typed columns. Column insertion order is the
/// frame's column order; [`schema`](TsDataFrame::schema) reports it.
#[derive(Default)]
pub struct TsDataFrame {
    names: Vec<String>,
    columns: Vec<TsColumn>,
}

impl TsDataFrame {
    pub fn new() -> Self {
        Self {
            names: Vec::new(),
            columns: Vec::new(),
        }
    }

    /// Builder add. Panics on a duplicate name - the chained builder form is
    /// for frames assembled from known-distinct columns; use
    /// [`push_column`](Self::push_column) for the fallible runtime path.
    pub fn with_column(mut self, name: impl Into<String>, col: TsColumn) -> Self {
        self.push_column(name, col)
            .expect("duplicate column name in TsDataFrame::with_column");
        self
    }

    /// Append a column. Errors on a duplicate name.
    pub fn push_column(&mut self, name: impl Into<String>, col: TsColumn) -> Result<(), TsError> {
        let name = name.into();
        if self.names.iter().any(|n| n == &name) {
            return Err(TsError::DuplicateColumn { name });
        }
        self.names.push(name);
        self.columns.push(col);
        Ok(())
    }

    fn index_of(&self, name: &str) -> Option<usize> {
        self.names.iter().position(|n| n == name)
    }

    pub fn column(&self, name: &str) -> Option<&TsColumn> {
        self.index_of(name).map(|i| &self.columns[i])
    }

    pub fn column_names(&self) -> impl Iterator<Item = &str> {
        self.names.iter().map(|n| n.as_str())
    }

    /// The frame's (name, type) shape in column order, derived from the
    /// columns themselves.
    pub fn schema(&self) -> TsFrameSchema {
        let fields = self
            .names
            .iter()
            .zip(self.columns.iter())
            .map(|(name, col)| TsField {
                name: name.clone(),
                data_type: col.data_type(),
            })
            .collect();
        TsFrameSchema { fields }
    }

    pub fn ncols(&self) -> usize {
        self.columns.len()
    }

    pub fn is_empty(&self) -> bool {
        self.columns.is_empty()
    }

    /// Projection: a new frame holding clones of the named columns, in the
    /// requested order. Unknown names are skipped.
    pub fn select(&self, names: &[&str]) -> TsDataFrame {
        let mut out = TsDataFrame::new();
        for &name in names {
            if let Some(i) = self.index_of(name) {
                out.names.push(self.names[i].clone());
                out.columns.push(clone_column(&self.columns[i]));
            }
        }
        out
    }

    /// Remove + return a column by name.
    pub fn drop(&mut self, name: &str) -> Option<TsColumn> {
        let i = self.index_of(name)?;
        self.names.remove(i);
        Some(self.columns.remove(i))
    }

    /// Rename a column in place. Returns `false` if `from` is absent or `to`
    /// already names another column.
    pub fn rename(&mut self, from: &str, to: &str) -> bool {
        let Some(i) = self.index_of(from) else {
            return false;
        };
        if from != to && self.names.iter().any(|n| n == to) {
            return false;
        }
        self.names[i] = to.to_string();
        true
    }

    /// Row-aligned view over the union of every column's ts axis. Each row is
    /// a ts plus one `Option<TsValue>` per column (in column order), `None`
    /// where that column has no point at that ts. This is the frame's null
    /// surface: the stored columns never hold nulls; the gaps are derived
    /// here by the multi-way merge.
    pub fn aligned(&self) -> impl Iterator<Item = (i64, Vec<Option<TsValue>>)> + '_ {
        let cursors: Vec<ColCursor<'_>> = self
            .columns
            .iter()
            .map(|c| c.iter_boxed().peekable())
            .collect();
        AlignedRows { cursors }
    }
}

fn clone_column(col: &TsColumn) -> TsColumn {
    match col {
        TsColumn::F64(s) => TsColumn::F64(s.clone()),
        TsColumn::I64(s) => TsColumn::I64(s.clone()),
        TsColumn::Bool(s) => TsColumn::Bool(s.clone()),
        TsColumn::Str(s) => TsColumn::Str(s.clone()),
        TsColumn::Value(s) => TsColumn::Value(s.clone()),
    }
}

type ColCursor<'a> = std::iter::Peekable<Box<dyn Iterator<Item = (i64, TsValue)> + 'a>>;

struct AlignedRows<'a> {
    cursors: Vec<ColCursor<'a>>,
}

impl Iterator for AlignedRows<'_> {
    type Item = (i64, Vec<Option<TsValue>>);

    fn next(&mut self) -> Option<Self::Item> {
        let min_ts = self
            .cursors
            .iter_mut()
            .filter_map(|c| c.peek().map(|(ts, _)| *ts))
            .min()?;
        let row = self
            .cursors
            .iter_mut()
            .map(|c| match c.peek() {
                Some((ts, _)) if *ts == min_ts => c.next().map(|(_, v)| v),
                _ => None,
            })
            .collect();
        Some((min_ts, row))
    }
}
