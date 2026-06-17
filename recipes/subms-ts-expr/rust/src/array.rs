//! [`TsArray`] - the typed, nullable result of evaluating a [`TsExpr`](crate::TsExpr).
//! One variant per supported element type ([`TsDataType`]), each a dense value
//! buffer paired with an Arrow-style validity bitmap. The two are the same
//! length, indexed by the frame's union-of-timestamps row axis: a cell is
//! meaningful only where `valid[i]` is set, and the value under an invalid cell
//! is unspecified (we keep a type default there but never rely on it).
//!
//! This is the computed-column primitive the rest of the analytical arc rebuilds
//! on - groupby aggregates, join keys, window frames, and reshape pivots all
//! land in a `TsArray`. The validity model is for DERIVED nulls: a `Col` over a
//! row where the column has a gap, a divide-by-zero, a null propagated through a
//! binary op. It stays distinct from `TsSeries`' no-null-on-ingest invariant - a
//! series never stores a null, but aligning several series onto a shared row
//! axis legitimately produces missing cells.

use subms_ts::{TsDataType, TsValue};

/// A typed, nullable, positional array: a dense value buffer plus a parallel
/// validity bitmap. `values` and `valid` are always the same length; a cell at
/// `i` is present only where `valid[i]` is `true`.
#[derive(Clone, Debug, PartialEq)]
pub enum TsArray {
    F64 { values: Vec<f64>, valid: Vec<bool> },
    I64 { values: Vec<i64>, valid: Vec<bool> },
    Bool { values: Vec<bool>, valid: Vec<bool> },
    Str { values: Vec<String>, valid: Vec<bool> },
}

impl TsArray {
    /// The element type of this array.
    pub fn data_type(&self) -> TsDataType {
        match self {
            TsArray::F64 { .. } => TsDataType::F64,
            TsArray::I64 { .. } => TsDataType::I64,
            TsArray::Bool { .. } => TsDataType::Bool,
            TsArray::Str { .. } => TsDataType::Str,
        }
    }

    pub fn len(&self) -> usize {
        match self {
            TsArray::F64 { values, .. } => values.len(),
            TsArray::I64 { values, .. } => values.len(),
            TsArray::Bool { values, .. } => values.len(),
            TsArray::Str { values, .. } => values.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The validity bitmap. `valid[i] == false` marks a null at row `i`.
    pub fn valid(&self) -> &[bool] {
        match self {
            TsArray::F64 { valid, .. } => valid,
            TsArray::I64 { valid, .. } => valid,
            TsArray::Bool { valid, .. } => valid,
            TsArray::Str { valid, .. } => valid,
        }
    }

    /// Count of present (non-null) cells.
    pub fn valid_count(&self) -> usize {
        self.valid().iter().filter(|&&v| v).count()
    }

    /// The boxed value at `i`, or `None` when that cell is null.
    pub fn get(&self, i: usize) -> Option<TsValue> {
        match self {
            TsArray::F64 { values, valid } => valid[i].then(|| TsValue::F64(values[i])),
            TsArray::I64 { values, valid } => valid[i].then(|| TsValue::I64(values[i])),
            TsArray::Bool { values, valid } => valid[i].then(|| TsValue::Bool(values[i])),
            TsArray::Str { values, valid } => valid[i].then(|| TsValue::Str(values[i].clone())),
        }
    }

    /// A view of an `F64` array's parallel buffers, or `None` for other types.
    pub fn as_f64(&self) -> Option<(&[f64], &[bool])> {
        match self {
            TsArray::F64 { values, valid } => Some((values, valid)),
            _ => None,
        }
    }

    /// A view of an `I64` array's parallel buffers, or `None` for other types.
    pub fn as_i64(&self) -> Option<(&[i64], &[bool])> {
        match self {
            TsArray::I64 { values, valid } => Some((values, valid)),
            _ => None,
        }
    }

    /// A view of a `Bool` array's parallel buffers, or `None` for other types.
    pub fn as_bool(&self) -> Option<(&[bool], &[bool])> {
        match self {
            TsArray::Bool { values, valid } => Some((values, valid)),
            _ => None,
        }
    }

    /// A view of a `Str` array's parallel buffers, or `None` for other types.
    pub fn as_str(&self) -> Option<(&[String], &[bool])> {
        match self {
            TsArray::Str { values, valid } => Some((values, valid)),
            _ => None,
        }
    }

    /// Replace every null cell with `fill`, returning a fully-valid array of the
    /// same type. A `fill` whose type does not match the array's element type is
    /// ignored for that cell (the cell stays at its slot default but is marked
    /// valid); callers pass a matching-typed fill - the analytical "coalesce
    /// missing to a default" operation.
    pub fn fill_null(&self, fill: TsValue) -> TsArray {
        match self {
            TsArray::F64 { values, valid } => {
                let f = match &fill {
                    TsValue::F64(v) => *v,
                    TsValue::I64(v) => *v as f64,
                    _ => f64::NAN,
                };
                TsArray::F64 {
                    values: coalesce(values, valid, f),
                    valid: vec![true; values.len()],
                }
            }
            TsArray::I64 { values, valid } => {
                let f = match &fill {
                    TsValue::I64(v) => *v,
                    _ => 0,
                };
                TsArray::I64 {
                    values: coalesce(values, valid, f),
                    valid: vec![true; values.len()],
                }
            }
            TsArray::Bool { values, valid } => {
                let f = matches!(&fill, TsValue::Bool(true));
                TsArray::Bool {
                    values: coalesce(values, valid, f),
                    valid: vec![true; values.len()],
                }
            }
            TsArray::Str { values, valid } => {
                let f = match &fill {
                    TsValue::Str(s) => s.clone(),
                    _ => String::new(),
                };
                let out = values
                    .iter()
                    .zip(valid)
                    .map(|(v, &ok)| if ok { v.clone() } else { f.clone() })
                    .collect();
                TsArray::Str {
                    values: out,
                    valid: vec![true; values.len()],
                }
            }
        }
    }

    /// Compact out every null cell, returning a shorter fully-valid array of just
    /// the present values. The row axis is not preserved - the "compact to
    /// present observations" operation, used before feeding a dense consumer.
    pub fn drop_nulls(&self) -> TsArray {
        match self {
            TsArray::F64 { values, valid } => {
                let kept = retain(values, valid);
                TsArray::F64 {
                    valid: vec![true; kept.len()],
                    values: kept,
                }
            }
            TsArray::I64 { values, valid } => {
                let kept = retain(values, valid);
                TsArray::I64 {
                    valid: vec![true; kept.len()],
                    values: kept,
                }
            }
            TsArray::Bool { values, valid } => {
                let kept = retain(values, valid);
                TsArray::Bool {
                    valid: vec![true; kept.len()],
                    values: kept,
                }
            }
            TsArray::Str { values, valid } => {
                let kept: Vec<String> = values
                    .iter()
                    .zip(valid)
                    .filter(|&(_, &ok)| ok)
                    .map(|(v, _)| v.clone())
                    .collect();
                TsArray::Str {
                    valid: vec![true; kept.len()],
                    values: kept,
                }
            }
        }
    }
}

fn coalesce<T: Copy>(values: &[T], valid: &[bool], fill: T) -> Vec<T> {
    values
        .iter()
        .zip(valid)
        .map(|(&v, &ok)| if ok { v } else { fill })
        .collect()
}

fn retain<T: Copy>(values: &[T], valid: &[bool]) -> Vec<T> {
    values
        .iter()
        .zip(valid)
        .filter_map(|(&v, &ok)| ok.then_some(v))
        .collect()
}
