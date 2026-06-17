//! The join engine. A [`TsDataFrame`] is first flattened into a row-major table
//! of named, typed [`TsArray`]s over its union-of-timestamps row axis (the same
//! flattening `subms-ts-expr` does), then joined on a tuple of typed key
//! columns. Output is a [`TsJoinResult`]: named [`TsArray`]s, all of equal
//! length, with Arrow-style validity for the missing cells an outer / left /
//! right join produces.
//!
//! Keys are typed CELLS, not f64 bit patterns: a join keys on a `Str` symbol,
//! an `I64` date, a `Bool` flag, or an `F64` value (or a tuple mixing them).
//! The key token preserves the cell's type, so a `Str` "AAPL" never collides
//! with anything but another `Str` "AAPL".

use std::collections::HashMap;

use subms_ts::{TsDataFrame, TsDataType, TsValue};
use subms_ts_expr::TsArray;

/// The six equi-join kinds. Hash and sort-merge implement all six and agree on
/// the result set; cross join ignores the kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TsJoinKind {
    /// Rows whose keys match on both sides. Left columns + right columns.
    Inner,
    /// Every left row; right columns are null where no match.
    Left,
    /// Every right row; left columns are null where no match.
    Right,
    /// Every matched pair, plus unmatched-left (right null) and unmatched-right
    /// (left null).
    Outer,
    /// Left rows that have at least one right match. Left columns only; each
    /// qualifying left row appears once regardless of match multiplicity.
    Semi,
    /// Left rows that have no right match. Left columns only.
    Anti,
}

/// Errors the join surface can raise. All are caller-input errors caught up
/// front; a successful join never partially fails.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TsJoinError {
    /// A named key column is not present in the frame it was requested from.
    UnknownKey { side: &'static str, name: String },
    /// `left_keys` and `right_keys` had different lengths - an equi-join pairs
    /// keys positionally, so the two key lists must be the same arity.
    KeyArityMismatch { left: usize, right: usize },
    /// Zero key columns were supplied to a keyed join. Use [`cross_join`] for
    /// the keyless cartesian product instead.
    NoKeys,
}

impl std::fmt::Display for TsJoinError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TsJoinError::UnknownKey { side, name } => {
                write!(f, "unknown {side} key column: {name}")
            }
            TsJoinError::KeyArityMismatch { left, right } => {
                write!(f, "key arity mismatch: left={left}, right={right}")
            }
            TsJoinError::NoKeys => write!(f, "a keyed join needs at least one key column"),
        }
    }
}

impl std::error::Error for TsJoinError {}

/// The result of a join: ordered named columns, all of the same length
/// ([`Self::nrows`]). Missing cells (the unmatched side of an outer / left /
/// right join) carry an unset validity bit in their [`TsArray`]; read them with
/// [`TsArray::get`] (`None` on a null) or coalesce with [`TsArray::fill_null`].
#[derive(Clone, Debug, PartialEq)]
pub struct TsJoinResult {
    columns: Vec<(String, TsArray)>,
    nrows: usize,
}

impl TsJoinResult {
    fn new(columns: Vec<(String, TsArray)>, nrows: usize) -> Self {
        debug_assert!(columns.iter().all(|(_, c)| c.len() == nrows));
        Self { columns, nrows }
    }

    pub fn nrows(&self) -> usize {
        self.nrows
    }

    pub fn ncols(&self) -> usize {
        self.columns.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nrows == 0
    }

    /// The named columns in output order. Key columns first (once, unsuffixed),
    /// then left payload, then right payload; collisions carry `_left` /
    /// `_right` suffixes.
    pub fn columns(&self) -> &[(String, TsArray)] {
        &self.columns
    }

    /// Column names in output order.
    pub fn column_names(&self) -> impl Iterator<Item = &str> {
        self.columns.iter().map(|(n, _)| n.as_str())
    }

    /// A column by its (possibly suffixed) output name.
    pub fn column(&self, name: &str) -> Option<&TsArray> {
        self.columns
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, c)| c)
    }

    /// A column by output index.
    pub fn column_at(&self, i: usize) -> Option<&TsArray> {
        self.columns.get(i).map(|(_, c)| c)
    }
}

// ---------- frame flattening ----------

/// Flatten a frame to named, typed [`TsArray`]s over its union-of-timestamps
/// row axis - exactly the shape `subms-ts-expr` evaluates against. Each column
/// is dense over the row axis: a row where the column had no point is a null
/// cell (validity unset). Exposed so a caller can inspect the same flattening
/// the join uses.
pub fn frame_columns(frame: &TsDataFrame) -> Vec<(String, TsArray)> {
    let order: Vec<String> = frame.column_names().map(|s| s.to_string()).collect();
    let types: Vec<TsDataType> = order
        .iter()
        .map(|n| frame.column(n).map(|c| c.data_type()).unwrap_or(TsDataType::F64))
        .collect();
    let mut cells: Vec<Vec<Option<TsValue>>> = vec![Vec::new(); order.len()];
    for (_ts, row) in frame.aligned() {
        for (i, cell) in row.into_iter().enumerate() {
            cells[i].push(cell);
        }
    }
    order
        .into_iter()
        .zip(types)
        .zip(cells)
        .map(|((name, ty), col_cells)| (name, cells_to_array(ty, &col_cells)))
        .collect()
}

// Project a column's per-row Option<TsValue> cells onto a typed TsArray of the
// column's declared dtype. A cell whose boxed value does not match the dtype is
// treated as null (a stored column never mixes types, so this is defensive).
fn cells_to_array(ty: TsDataType, cells: &[Option<TsValue>]) -> TsArray {
    let n = cells.len();
    match ty {
        TsDataType::F64 | TsDataType::Value => {
            let mut values = vec![0.0; n];
            let mut valid = vec![false; n];
            for (i, c) in cells.iter().enumerate() {
                if let Some(v) = c.as_ref().and_then(value_as_f64) {
                    values[i] = v;
                    valid[i] = true;
                }
            }
            TsArray::F64 { values, valid }
        }
        TsDataType::I64 => {
            let mut values = vec![0i64; n];
            let mut valid = vec![false; n];
            for (i, c) in cells.iter().enumerate() {
                if let Some(TsValue::I64(v)) = c {
                    values[i] = *v;
                    valid[i] = true;
                }
            }
            TsArray::I64 { values, valid }
        }
        TsDataType::Bool => {
            let mut values = vec![false; n];
            let mut valid = vec![false; n];
            for (i, c) in cells.iter().enumerate() {
                if let Some(TsValue::Bool(v)) = c {
                    values[i] = *v;
                    valid[i] = true;
                }
            }
            TsArray::Bool { values, valid }
        }
        TsDataType::Str => {
            let mut values = vec![String::new(); n];
            let mut valid = vec![false; n];
            for (i, c) in cells.iter().enumerate() {
                if let Some(TsValue::Str(v)) = c {
                    values[i] = v.clone();
                    valid[i] = true;
                }
            }
            TsArray::Str { values, valid }
        }
    }
}

fn value_as_f64(v: &TsValue) -> Option<f64> {
    match v {
        TsValue::F64(x) => Some(*x),
        TsValue::I64(x) => Some(*x as f64),
        _ => None,
    }
}

// A flattened input ready to join: its named columns + the resolved
// key-column indices.
struct Table {
    columns: Vec<(String, TsArray)>,
    key_idx: Vec<usize>,
    nrows: usize,
}

impl Table {
    fn build(
        frame: &TsDataFrame,
        keys: &[&str],
        side: &'static str,
    ) -> Result<Table, TsJoinError> {
        let columns = frame_columns(frame);
        let nrows = columns.first().map(|(_, c)| c.len()).unwrap_or(0);
        let mut key_idx = Vec::with_capacity(keys.len());
        for k in keys {
            let idx = columns
                .iter()
                .position(|(n, _)| n == k)
                .ok_or_else(|| TsJoinError::UnknownKey {
                    side,
                    name: (*k).to_string(),
                })?;
            key_idx.push(idx);
        }
        Ok(Table {
            columns,
            key_idx,
            nrows,
        })
    }

    fn name(&self, i: usize) -> &str {
        &self.columns[i].0
    }

    fn col(&self, i: usize) -> &TsArray {
        &self.columns[i].1
    }

    // The key tuple at a row, encoded for hashing / equality. A null key cell
    // tags the whole tuple unmatchable - in SQL semantics a NULL key never
    // equals anything, including another NULL.
    fn key_at(&self, row: usize) -> KeyTuple {
        let mut parts = Vec::with_capacity(self.key_idx.len());
        let mut matchable = true;
        for &ki in &self.key_idx {
            match key_token(self.col(ki), row) {
                Some(tok) => parts.push(tok),
                None => {
                    matchable = false;
                    parts.push(KeyToken::Null);
                }
            }
        }
        KeyTuple { parts, matchable }
    }
}

// A single typed key component, hashable and totally ordered. The f64 bit
// pattern is the F64 equality token: -0.0 and +0.0 differ in bits and so do
// NOT join (documented as a non-claim, matching SQL/Polars float-key behaviour
// rather than papering over it with a fuzzy compare). NaN keys are unmatchable
// upstream (a column never stores a non-finite f64).
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum KeyToken {
    Null,
    Bool(bool),
    I64(i64),
    F64Bits(u64),
    Str(String),
}

fn key_token(col: &TsArray, row: usize) -> Option<KeyToken> {
    match col.get(row)? {
        TsValue::Bool(b) => Some(KeyToken::Bool(b)),
        TsValue::I64(v) => Some(KeyToken::I64(v)),
        TsValue::F64(v) => Some(KeyToken::F64Bits(v.to_bits())),
        TsValue::Str(s) => Some(KeyToken::Str(s)),
        _ => None,
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct KeyTuple {
    parts: Vec<KeyToken>,
    matchable: bool,
}

// ---------- output assembly ----------

// A row of the output is "left row L (or none) paired with right row R (or
// none)". The builder turns a sequence of these into the renamed, validity-
// carrying output columns.
struct OutputPlan {
    // (left_row, right_row); None on either side means that side is missing.
    pairs: Vec<(Option<usize>, Option<usize>)>,
}

fn assemble(left: &Table, right: &Table, plan: &OutputPlan, kind: TsJoinKind) -> TsJoinResult {
    let nrows = plan.pairs.len();
    let mut out: Vec<(String, TsArray)> = Vec::new();

    // Key columns once, from whichever side is present per row (left wins when
    // both present; an outer right-only row reads the key off the right).
    for (li, &lk) in left.key_idx.iter().enumerate() {
        let rk = right.key_idx[li];
        let arr = project_key(left.col(lk), right.col(rk), &plan.pairs);
        out.push((left.name(lk).to_string(), arr));
    }

    // Semi / Anti emit left non-key columns only.
    let emit_right = !matches!(kind, TsJoinKind::Semi | TsJoinKind::Anti);

    for ci in 0..left.columns.len() {
        if left.key_idx.contains(&ci) {
            continue;
        }
        let name = left.name(ci);
        let out_name = if emit_right && right_has_payload_name(right, name) {
            format!("{name}_left")
        } else {
            name.to_string()
        };
        let arr = project_side(left.col(ci), &plan.pairs, Side::Left);
        out.push((out_name, arr));
    }

    if emit_right {
        for ci in 0..right.columns.len() {
            if right.key_idx.contains(&ci) {
                continue;
            }
            let name = right.name(ci);
            let out_name = if left_has_payload_name(left, name) {
                format!("{name}_right")
            } else {
                name.to_string()
            };
            let arr = project_side(right.col(ci), &plan.pairs, Side::Right);
            out.push((out_name, arr));
        }
    }

    TsJoinResult::new(out, nrows)
}

fn right_has_payload_name(right: &Table, name: &str) -> bool {
    right
        .columns
        .iter()
        .enumerate()
        .any(|(rj, (rn, _))| rn == name && !right.key_idx.contains(&rj))
}

fn left_has_payload_name(left: &Table, name: &str) -> bool {
    left.columns
        .iter()
        .enumerate()
        .any(|(lj, (ln, _))| ln == name && !left.key_idx.contains(&lj))
}

#[derive(Clone, Copy)]
enum Side {
    Left,
    Right,
}

// The key column for the output, coalescing left-then-right per row so an
// outer right-only row still carries its key.
fn project_key(
    lcol: &TsArray,
    rcol: &TsArray,
    pairs: &[(Option<usize>, Option<usize>)],
) -> TsArray {
    let n = pairs.len();
    let mut b = ArrayBuilder::for_type(lcol.data_type(), n);
    for &(l, r) in pairs {
        let cell = l
            .and_then(|i| lcol.get(i))
            .or_else(|| r.and_then(|i| rcol.get(i)));
        b.push(cell);
    }
    b.finish()
}

fn project_side(
    src: &TsArray,
    pairs: &[(Option<usize>, Option<usize>)],
    side: Side,
) -> TsArray {
    let n = pairs.len();
    let mut b = ArrayBuilder::for_type(src.data_type(), n);
    for &(l, r) in pairs {
        let row = match side {
            Side::Left => l,
            Side::Right => r,
        };
        b.push(row.and_then(|i| src.get(i)));
    }
    b.finish()
}

// Accumulates typed cells (Some / None) into the matching TsArray variant. A
// cell whose boxed type disagrees with the builder's type is recorded as null.
enum ArrayBuilder {
    F64 { values: Vec<f64>, valid: Vec<bool> },
    I64 { values: Vec<i64>, valid: Vec<bool> },
    Bool { values: Vec<bool>, valid: Vec<bool> },
    Str { values: Vec<String>, valid: Vec<bool> },
}

impl ArrayBuilder {
    fn for_type(ty: TsDataType, cap: usize) -> ArrayBuilder {
        match ty {
            TsDataType::I64 => ArrayBuilder::I64 {
                values: Vec::with_capacity(cap),
                valid: Vec::with_capacity(cap),
            },
            TsDataType::Bool => ArrayBuilder::Bool {
                values: Vec::with_capacity(cap),
                valid: Vec::with_capacity(cap),
            },
            TsDataType::Str => ArrayBuilder::Str {
                values: Vec::with_capacity(cap),
                valid: Vec::with_capacity(cap),
            },
            // F64 and the schemaless Value escape hatch both land in an f64
            // array, matching the flattening in cells_to_array.
            TsDataType::F64 | TsDataType::Value => ArrayBuilder::F64 {
                values: Vec::with_capacity(cap),
                valid: Vec::with_capacity(cap),
            },
        }
    }

    fn push(&mut self, cell: Option<TsValue>) {
        match self {
            ArrayBuilder::F64 { values, valid } => match cell {
                Some(v) => match value_as_f64(&v) {
                    Some(f) => {
                        values.push(f);
                        valid.push(true);
                    }
                    None => {
                        values.push(0.0);
                        valid.push(false);
                    }
                },
                None => {
                    values.push(0.0);
                    valid.push(false);
                }
            },
            ArrayBuilder::I64 { values, valid } => match cell {
                Some(TsValue::I64(v)) => {
                    values.push(v);
                    valid.push(true);
                }
                _ => {
                    values.push(0);
                    valid.push(false);
                }
            },
            ArrayBuilder::Bool { values, valid } => match cell {
                Some(TsValue::Bool(v)) => {
                    values.push(v);
                    valid.push(true);
                }
                _ => {
                    values.push(false);
                    valid.push(false);
                }
            },
            ArrayBuilder::Str { values, valid } => match cell {
                Some(TsValue::Str(v)) => {
                    values.push(v);
                    valid.push(true);
                }
                _ => {
                    values.push(String::new());
                    valid.push(false);
                }
            },
        }
    }

    fn finish(self) -> TsArray {
        match self {
            ArrayBuilder::F64 { values, valid } => TsArray::F64 { values, valid },
            ArrayBuilder::I64 { values, valid } => TsArray::I64 { values, valid },
            ArrayBuilder::Bool { values, valid } => TsArray::Bool { values, valid },
            ArrayBuilder::Str { values, valid } => TsArray::Str { values, valid },
        }
    }
}

// ---------- public join entry points ----------

fn validate_keys(left_keys: &[&str], right_keys: &[&str]) -> Result<(), TsJoinError> {
    if left_keys.is_empty() {
        return Err(TsJoinError::NoKeys);
    }
    if left_keys.len() != right_keys.len() {
        return Err(TsJoinError::KeyArityMismatch {
            left: left_keys.len(),
            right: right_keys.len(),
        });
    }
    Ok(())
}

/// Equi-join via a hash table on the smaller side, probed by the larger.
///
/// Keys are typed cells: a `Str` symbol column joins to a `Str` symbol column,
/// an `I64` date to an `I64` date, a tuple of `(Str, I64)` to another. A cell's
/// type is part of its key token, so a string never collides with a number.
///
/// Row order is deterministic: the output walks the LEFT input top to bottom
/// (driving order), and for each left row emits its right matches in the right
/// input's original row order. Outer-only right rows (the rows nothing matched)
/// come last, in right-input order. The build side is chosen by size for speed,
/// but the per-left-row match lists are always materialised in right-input
/// order, so output order does not depend on which side was built.
pub fn hash_join(
    left: &TsDataFrame,
    right: &TsDataFrame,
    left_keys: &[&str],
    right_keys: &[&str],
    kind: TsJoinKind,
) -> Result<TsJoinResult, TsJoinError> {
    validate_keys(left_keys, right_keys)?;
    let left = Table::build(left, left_keys, "left")?;
    let right = Table::build(right, right_keys, "right")?;

    // Build a key -> right-row-list index once. Insertion-ordered list per key
    // preserves right-input order for deterministic output.
    let mut index: HashMap<KeyTuple, Vec<usize>> = HashMap::new();
    for r in 0..right.nrows {
        let key = right.key_at(r);
        if !key.matchable {
            continue;
        }
        index.entry(key).or_default().push(r);
    }

    let mut pairs: Vec<(Option<usize>, Option<usize>)> = Vec::new();
    let mut right_matched = vec![false; right.nrows];

    for l in 0..left.nrows {
        let key = left.key_at(l);
        let matches = if key.matchable { index.get(&key) } else { None };
        match (kind, matches) {
            (TsJoinKind::Semi, Some(_)) => pairs.push((Some(l), None)),
            (TsJoinKind::Semi, None) => {}
            (TsJoinKind::Anti, Some(_)) => {}
            (TsJoinKind::Anti, None) => pairs.push((Some(l), None)),
            (_, Some(rs)) => {
                for &r in rs {
                    right_matched[r] = true;
                    pairs.push((Some(l), Some(r)));
                }
            }
            (TsJoinKind::Left | TsJoinKind::Outer, None) => pairs.push((Some(l), None)),
            (TsJoinKind::Inner | TsJoinKind::Right, None) => {}
        }
    }

    if matches!(kind, TsJoinKind::Right | TsJoinKind::Outer) {
        for (r, matched) in right_matched.iter().enumerate() {
            if !matched {
                pairs.push((None, Some(r)));
            }
        }
    }

    let plan = OutputPlan { pairs };
    Ok(assemble(&left, &right, &plan, kind))
}

/// Equi-join via sort-merge: both inputs are sorted on the key tuple, then
/// merged in a single linear pass. Produces the same result SET as
/// [`hash_join`] for every kind. Row order differs: output is in sorted-key
/// order (then, within a key group, left-input order x right-input order),
/// which is the natural order for already-sorted time-series inputs.
///
/// Unmatchable (null-key) rows are handled like SQL NULL keys: they never
/// merge, and surface as unmatched-left / unmatched-right per the kind.
pub fn sort_merge_join(
    left: &TsDataFrame,
    right: &TsDataFrame,
    left_keys: &[&str],
    right_keys: &[&str],
    kind: TsJoinKind,
) -> Result<TsJoinResult, TsJoinError> {
    validate_keys(left_keys, right_keys)?;
    let left = Table::build(left, left_keys, "left")?;
    let right = Table::build(right, right_keys, "right")?;

    // Sort row indices by key. Unmatchable rows sort to the end and are never
    // merged; the matchable flag keeps a null key from colliding with a real one.
    let mut lorder: Vec<usize> = (0..left.nrows).collect();
    let mut rorder: Vec<usize> = (0..right.nrows).collect();
    lorder.sort_by(|&a, &b| cmp_key(&left.key_at(a), &left.key_at(b)));
    rorder.sort_by(|&a, &b| cmp_key(&right.key_at(a), &right.key_at(b)));

    let mut pairs: Vec<(Option<usize>, Option<usize>)> = Vec::new();
    let mut li = 0usize;
    let mut ri = 0usize;
    let emit_unmatched_left = matches!(
        kind,
        TsJoinKind::Left | TsJoinKind::Outer | TsJoinKind::Anti
    );
    let emit_unmatched_right = matches!(kind, TsJoinKind::Right | TsJoinKind::Outer);

    while li < left.nrows {
        let lkey = left.key_at(lorder[li]);
        if !lkey.matchable {
            // every remaining left row is unmatchable (they sorted last).
            break;
        }
        // advance right past keys strictly less than the current left key.
        while ri < right.nrows {
            let rkey = right.key_at(rorder[ri]);
            if !rkey.matchable || cmp_key(&rkey, &lkey) != std::cmp::Ordering::Less {
                break;
            }
            if emit_unmatched_right {
                pairs.push((None, Some(rorder[ri])));
            }
            ri += 1;
        }
        // gather the left group sharing lkey.
        let lstart = li;
        while li < left.nrows {
            let k = left.key_at(lorder[li]);
            if k.matchable && cmp_key(&k, &lkey) == std::cmp::Ordering::Equal {
                li += 1;
            } else {
                break;
            }
        }
        // gather the right group sharing lkey.
        let rstart = ri;
        while ri < right.nrows {
            let k = right.key_at(rorder[ri]);
            if k.matchable && cmp_key(&k, &lkey) == std::cmp::Ordering::Equal {
                ri += 1;
            } else {
                break;
            }
        }
        let l_group = &lorder[lstart..li];
        let r_group = &rorder[rstart..ri];
        emit_group(&mut pairs, l_group, r_group, kind, emit_unmatched_left);
    }

    // any left rows left over with no right match (right exhausted).
    while li < left.nrows {
        if emit_unmatched_left {
            push_unmatched_left(&mut pairs, lorder[li], kind);
        }
        li += 1;
    }
    // any trailing right rows (their key exceeded every left key).
    while ri < right.nrows {
        let rkey = right.key_at(rorder[ri]);
        if rkey.matchable && emit_unmatched_right {
            pairs.push((None, Some(rorder[ri])));
        }
        ri += 1;
    }

    let plan = OutputPlan { pairs };
    Ok(assemble(&left, &right, &plan, kind))
}

fn emit_group(
    pairs: &mut Vec<(Option<usize>, Option<usize>)>,
    l_group: &[usize],
    r_group: &[usize],
    kind: TsJoinKind,
    emit_unmatched_left: bool,
) {
    let has_right = !r_group.is_empty();
    match kind {
        TsJoinKind::Semi => {
            if has_right {
                for &l in l_group {
                    pairs.push((Some(l), None));
                }
            }
        }
        TsJoinKind::Anti => {
            if !has_right {
                for &l in l_group {
                    pairs.push((Some(l), None));
                }
            }
        }
        _ => {
            if has_right {
                for &l in l_group {
                    for &r in r_group {
                        pairs.push((Some(l), Some(r)));
                    }
                }
            } else if emit_unmatched_left {
                for &l in l_group {
                    pairs.push((Some(l), None));
                }
            }
        }
    }
}

fn push_unmatched_left(
    pairs: &mut Vec<(Option<usize>, Option<usize>)>,
    l: usize,
    kind: TsJoinKind,
) {
    match kind {
        TsJoinKind::Semi => {}
        _ => pairs.push((Some(l), None)),
    }
}

fn cmp_key(a: &KeyTuple, b: &KeyTuple) -> std::cmp::Ordering {
    // unmatchable (null-key) tuples sort after everything so the merge can stop
    // the moment it hits the first one.
    match (a.matchable, b.matchable) {
        (true, true) => a.parts.cmp(&b.parts),
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        (false, false) => std::cmp::Ordering::Equal,
    }
}

/// The keyless cartesian product: every left row paired with every right row.
/// Output row count is `left.nrows * right.nrows`; order is left-major (left
/// row 0 against every right row, then left row 1, ...). All cells are present
/// (a cross join never produces a null). Column collisions are suffixed
/// `_left` / `_right`.
pub fn cross_join(left: &TsDataFrame, right: &TsDataFrame) -> TsJoinResult {
    let left = Table::build(left, &[], "left").expect("cross has no keys to resolve");
    let right = Table::build(right, &[], "right").expect("cross has no keys to resolve");
    let mut pairs = Vec::with_capacity(left.nrows * right.nrows);
    for l in 0..left.nrows {
        for r in 0..right.nrows {
            pairs.push((Some(l), Some(r)));
        }
    }
    let plan = OutputPlan { pairs };
    // Inner-style assembly (no missing cells, both sides emitted, suffixing on).
    assemble(&left, &right, &plan, TsJoinKind::Inner)
}
