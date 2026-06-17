//! Lowering: turn a parsed [`TsSqlStmt`] into a result [`TsDataFrame`] by
//! compiling its clauses onto the already-built operator recipes. There is no
//! execution engine of its own here - every clause becomes a call into
//! `subms-ts-expr` (scalar / predicate IR), `subms-ts-lazy` (row-wise
//! select / filter / sort / limit / with_column pipeline), or
//! `subms-ts-groupby` (the partition + per-group aggregate).
//!
//! Two shapes:
//!
//! - A query with `GROUP BY` or any aggregate in its projection is an
//!   AGGREGATE query: keys + aggregate exprs lower to `group_by(...).agg(...)`,
//!   then optional `ORDER BY` / `LIMIT` ride a lazy pipeline over the result.
//! - Everything else is a ROW-WISE pipeline: projections become `with_column`
//!   (then a final `select` to order + alias), `WHERE` a `filter`, `ORDER BY`
//!   a `sort_by` chain, `LIMIT` a `limit`.

use subms_ts::{TsColumn, TsDataFrame, TsSeries};
use subms_ts_expr::{TsArray, TsExpr, when};
use subms_ts_groupby::{TsGroupResult, group_by};
use subms_ts_lazy::{LazyTsFrame, ResultFrame};

use crate::TsSqlError;
use crate::ast::{AggFunc, ArithOp, CmpOp, SelectItem, SqlExpr, SqlLiteral, TsSqlStmt};

/// Lower + execute `stmt` against `source`, returning the result frame.
pub fn execute(stmt: &TsSqlStmt, source: &TsDataFrame) -> Result<TsDataFrame, TsSqlError> {
    let source_cols: Vec<String> = source.column_names().map(|s| s.to_string()).collect();
    validate_columns(stmt, &source_cols)?;

    let is_aggregate = !stmt.group_by.is_empty() || projection_has_aggregate(stmt);
    if is_aggregate {
        execute_aggregate(stmt, source, &source_cols)
    } else {
        execute_rowwise(stmt, source, &source_cols)
    }
}

fn projection_has_aggregate(stmt: &TsSqlStmt) -> bool {
    stmt.projection.iter().any(|item| match item {
        SelectItem::Star => false,
        SelectItem::Expr { expr, .. } => expr.contains_aggregate(),
    })
}

// Clone a source frame's columns into a fresh owned frame. The lazy planner and
// group-by both take an owned TsDataFrame; the catalog only lends a borrow.
fn clone_frame(source: &TsDataFrame, cols: &[String]) -> TsDataFrame {
    let names: Vec<&str> = cols.iter().map(|s| s.as_str()).collect();
    source.select(&names)
}

// ---------- row-wise pipeline ----------

fn execute_rowwise(
    stmt: &TsSqlStmt,
    source: &TsDataFrame,
    source_cols: &[String],
) -> Result<TsDataFrame, TsSqlError> {
    let mut lazy = LazyTsFrame::new(clone_frame(source, source_cols));

    if let Some(pred) = &stmt.filter {
        lazy = lazy.filter(lower_predicate(pred)?);
    }

    // Each non-trivial projection becomes a derived column; a bare column passes
    // through. We then `select` the alias list to fix output order + names.
    let mut out_names: Vec<String> = Vec::new();
    for item in &stmt.projection {
        match item {
            SelectItem::Star => {
                for c in source_cols {
                    if !out_names.contains(c) {
                        out_names.push(c.clone());
                    }
                }
            }
            SelectItem::Expr { expr, alias } => {
                match expr {
                    // A bare column projection needs no derived column.
                    SqlExpr::Column(name) if name == alias => {}
                    _ => {
                        lazy = lazy.with_column(alias.clone(), lower_scalar(expr)?);
                    }
                }
                if !out_names.contains(alias) {
                    out_names.push(alias.clone());
                }
            }
        }
    }

    lazy = apply_order_by(lazy, &stmt.order_by);

    let select_refs: Vec<&str> = out_names.iter().map(|s| s.as_str()).collect();
    lazy = lazy.select(&select_refs);

    if let Some(n) = stmt.limit {
        lazy = lazy.limit(n);
    }

    let result = lazy.collect().map_err(map_lazy_err)?;
    Ok(result_to_ordered_frame(&result))
}

// Rebuild a TsDataFrame from a lazy ResultFrame using ROW POSITION as the
// synthetic ts, so the pipeline's row order (post ORDER BY) is preserved. The
// lazy `into_data_frame` re-sorts rows by their original ts, which would undo
// an ORDER BY; a SQL result is positional, not temporal, so we re-key on the
// row index. Every column shares the 0..n axis, so cross-column alignment holds.
fn result_to_ordered_frame(result: &ResultFrame) -> TsDataFrame {
    let n = result.nrows();
    let mut frame = TsDataFrame::new();
    for name in result
        .column_names()
        .map(|s| s.to_string())
        .collect::<Vec<_>>()
    {
        let arr = result.column(&name).expect("listed column exists");
        frame
            .push_column(name, array_to_positional_column(arr, n))
            .expect("result column names are unique");
    }
    frame
}

// Turn a positional TsArray into a typed column at ts = row index. A null cell
// is simply not pushed (a gap), which is the frame's null model.
fn array_to_positional_column(arr: &TsArray, n: usize) -> TsColumn {
    match arr {
        TsArray::F64 { values, valid } => {
            let mut s = TsSeries::<f64>::with_capacity(n);
            for (i, (&v, &ok)) in values.iter().zip(valid).enumerate() {
                if ok {
                    let _ = s.push(i as i64, v);
                }
            }
            TsColumn::F64(s)
        }
        TsArray::I64 { values, valid } => {
            let mut s = TsSeries::<i64>::with_capacity(n);
            for (i, (&v, &ok)) in values.iter().zip(valid).enumerate() {
                if ok {
                    let _ = s.push(i as i64, v);
                }
            }
            TsColumn::I64(s)
        }
        TsArray::Bool { values, valid } => {
            let mut s = TsSeries::<bool>::with_capacity(n);
            for (i, (&v, &ok)) in values.iter().zip(valid).enumerate() {
                if ok {
                    let _ = s.push(i as i64, v);
                }
            }
            TsColumn::Bool(s)
        }
        TsArray::Str { values, valid } => {
            let mut s = TsSeries::<String>::with_capacity(n);
            for (i, (v, &ok)) in values.iter().zip(valid).enumerate() {
                if ok {
                    let _ = s.push(i as i64, v.clone());
                }
            }
            TsColumn::Str(s)
        }
    }
}

// ---------- aggregate query ----------

fn execute_aggregate(
    stmt: &TsSqlStmt,
    source: &TsDataFrame,
    source_cols: &[String],
) -> Result<TsDataFrame, TsSqlError> {
    let owned = clone_frame(source, source_cols);

    // The projection list, in order, is either a group key column or an
    // aggregate. A non-key, non-aggregate bare column would be ambiguous under
    // SQL group-by rules; reject it rather than guess.
    let mut agg_specs: Vec<(String, TsExpr)> = Vec::new();
    for item in &stmt.projection {
        match item {
            SelectItem::Star => {
                return Err(TsSqlError::ty(
                    "SELECT * is not allowed with GROUP BY / aggregates",
                ));
            }
            SelectItem::Expr { expr, alias } => {
                if expr.contains_aggregate() {
                    agg_specs.push((alias.clone(), lower_agg_expr(expr)?));
                } else if let SqlExpr::Column(name) = expr {
                    if !stmt.group_by.contains(name) {
                        return Err(TsSqlError::ty(format!(
                            "column {name} must appear in GROUP BY or an aggregate"
                        )));
                    }
                    // A key column is emitted by group_by directly; no agg spec.
                } else {
                    return Err(TsSqlError::ty(
                        "a non-aggregate projection must be a GROUP BY key column",
                    ));
                }
            }
        }
    }

    // A bare aggregate query with no GROUP BY (e.g. SELECT COUNT(*) FROM t) is a
    // single whole-frame group. We synthesise a constant key so the result is a
    // one-row frame; the key column is dropped from the output below.
    let synthetic_key = stmt.group_by.is_empty();
    let frame_with_key;
    let keys: Vec<&str> = if synthetic_key {
        frame_with_key = with_constant_key(&owned);
        vec![GROUP_ALL_KEY]
    } else {
        frame_with_key = owned;
        stmt.group_by.iter().map(|s| s.as_str()).collect()
    };

    let agg_refs: Vec<(&str, TsExpr)> = agg_specs
        .iter()
        .map(|(n, e)| (n.as_str(), e.clone()))
        .collect();
    let grouped = group_by(&frame_with_key, &keys)
        .map_err(map_groupby_err)?
        .agg(&agg_refs)
        .map_err(map_groupby_err)?;

    let mut frame = group_result_to_frame(&grouped);
    if synthetic_key {
        frame.drop(GROUP_ALL_KEY);
    }

    // ORDER BY / LIMIT over the aggregated result ride a lazy pipeline. The
    // projection order is already fixed by group_by (keys then aggs in the
    // order we pushed them), which matches the SELECT list order.
    apply_post_aggregate(stmt, frame)
}

const GROUP_ALL_KEY: &str = "__subms_sql_all";

fn with_constant_key(frame: &TsDataFrame) -> TsDataFrame {
    // Re-emit every source column plus a constant i64 key at the same row axis.
    // The aligned walk gives us the row timestamps; the key is 0 everywhere so
    // every row lands in one group.
    let names: Vec<String> = frame.column_names().map(|s| s.to_string()).collect();
    let refs: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
    let mut out = frame.select(&refs);
    let mut key = TsSeries::<i64>::new();
    for (ts, _row) in frame.aligned() {
        let _ = key.push(ts, 0);
    }
    out.push_column(GROUP_ALL_KEY, TsColumn::I64(key))
        .expect("synthetic key name is reserved");
    out
}

fn apply_post_aggregate(stmt: &TsSqlStmt, frame: TsDataFrame) -> Result<TsDataFrame, TsSqlError> {
    if stmt.order_by.is_empty() && stmt.limit.is_none() {
        return Ok(frame);
    }
    // Capture the aggregated frame's full column set so a terminal `select`
    // pins it: the lazy optimiser's projection pushdown drops any column no
    // downstream node references, and a lone `sort_by` references only its key.
    let out_names: Vec<String> = frame.column_names().map(|s| s.to_string()).collect();
    let mut lazy = LazyTsFrame::new(frame);
    lazy = apply_order_by(lazy, &stmt.order_by);
    let refs: Vec<&str> = out_names.iter().map(|s| s.as_str()).collect();
    lazy = lazy.select(&refs);
    if let Some(n) = stmt.limit {
        lazy = lazy.limit(n);
    }
    let result = lazy.collect().map_err(map_lazy_err)?;
    Ok(result_to_ordered_frame(&result))
}

// Apply a multi-key ORDER BY as a chain of stable single-column sorts. The lazy
// sort is stable, so sorting by the LEAST-significant key first and the most-
// significant key last yields the correct lexicographic multi-key order (each
// later sort preserves the relative order the earlier sorts established).
fn apply_order_by(mut lazy: LazyTsFrame, keys: &[crate::ast::OrderKey]) -> LazyTsFrame {
    for key in keys.iter().rev() {
        lazy = lazy.sort_by(key.column.clone(), key.ascending);
    }
    lazy
}

// A TsGroupResult is a set of named TsArrays. Rebuild a TsDataFrame by gathering
// each array's present cells at synthetic monotonic timestamps so the row axis
// lines up across columns (the aggregate collapsed the original time axis).
fn group_result_to_frame(result: &TsGroupResult) -> TsDataFrame {
    let mut frame = TsDataFrame::new();
    for name in result
        .column_names()
        .map(|s| s.to_string())
        .collect::<Vec<_>>()
    {
        let arr = result.column(&name).expect("listed column exists");
        let col = array_to_positional_column(arr, arr.len());
        frame
            .push_column(name, col)
            .expect("group result column names are unique");
    }
    frame
}

// ---------- expression lowering ----------

/// Lower a scalar projection expression to a `TsExpr`. Rejects an aggregate in
/// row-wise scalar position (an aggregate only appears in an aggregate query).
fn lower_scalar(expr: &SqlExpr) -> Result<TsExpr, TsSqlError> {
    match expr {
        SqlExpr::Column(name) => Ok(TsExpr::col(name.clone())),
        SqlExpr::Literal(lit) => Ok(lower_literal(lit)),
        SqlExpr::Arith(op, a, b) => {
            let l = lower_scalar(a)?;
            let r = lower_scalar(b)?;
            Ok(match op {
                ArithOp::Add => l.add(r),
                ArithOp::Sub => l.sub(r),
                ArithOp::Mul => l.mul(r),
                ArithOp::Div => l.div(r),
            })
        }
        SqlExpr::Case {
            when: cond,
            then,
            otherwise,
        } => {
            let c = lower_predicate(cond)?;
            let t = lower_scalar(then)?;
            let f = lower_scalar(otherwise)?;
            Ok(when(c, t, f))
        }
        SqlExpr::Compare(..) | SqlExpr::And(..) | SqlExpr::Or(..) | SqlExpr::Not(..) => Err(
            TsSqlError::ty("a boolean expression is not a scalar projection"),
        ),
        SqlExpr::Aggregate { .. } => Err(TsSqlError::ty(
            "an aggregate is not allowed in a row-wise projection (add GROUP BY)",
        )),
    }
}

/// Lower a predicate (boolean tree) to a `Bool`-typed `TsExpr`. Comparisons map
/// directly; `AND`/`OR` are emulated via arithmetic on the boolean masks
/// (`subms-ts-expr` has no boolean-logic op, so we compose through compare).
fn lower_predicate(expr: &SqlExpr) -> Result<TsExpr, TsSqlError> {
    match expr {
        SqlExpr::Compare(op, a, b) => {
            let l = lower_scalar(a)?;
            let r = lower_scalar(b)?;
            Ok(match op {
                CmpOp::Eq => l.eq(r),
                CmpOp::Ne => l.ne(r),
                CmpOp::Lt => l.lt(r),
                CmpOp::Le => l.le(r),
                CmpOp::Gt => l.gt(r),
                CmpOp::Ge => l.ge(r),
            })
        }
        // AND: true only where both masks are true. Map each bool to an i64
        // (1/0) via a When, multiply, and compare > 0. This keeps the result a
        // Bool TsExpr without needing a boolean-logic node in the expr IR.
        SqlExpr::And(a, b) => {
            let l = bool_to_int(lower_predicate(a)?);
            let r = bool_to_int(lower_predicate(b)?);
            Ok(l.mul(r).gt(TsExpr::lit_i64(0)))
        }
        // OR: true where either mask is true -> sum of the 1/0 masks > 0.
        SqlExpr::Or(a, b) => {
            let l = bool_to_int(lower_predicate(a)?);
            let r = bool_to_int(lower_predicate(b)?);
            Ok(l.add(r).gt(TsExpr::lit_i64(0)))
        }
        // NOT: the 1/0 mask equals 0.
        SqlExpr::Not(inner) => {
            let m = bool_to_int(lower_predicate(inner)?);
            Ok(m.eq(TsExpr::lit_i64(0)))
        }
        other => Err(TsSqlError::ty(format!(
            "expected a boolean predicate, got a scalar: {other:?}"
        ))),
    }
}

// Map a Bool TsExpr to an i64 1/0 mask so boolean logic composes via arithmetic.
fn bool_to_int(pred: TsExpr) -> TsExpr {
    when(pred, TsExpr::lit_i64(1), TsExpr::lit_i64(0))
}

/// Lower an aggregate-bearing projection expression. In the subset an aggregate
/// projection is a single top-level aggregate call (`SUM(x)`, `COUNT(*)`); a
/// scalar wrapped around an aggregate (`SUM(x) / 2`) is out of scope and a clear
/// error - the group-by surface reduces one `TsExpr::Agg` per output column.
fn lower_agg_expr(expr: &SqlExpr) -> Result<TsExpr, TsSqlError> {
    match expr {
        SqlExpr::Aggregate { func, arg } => {
            let operand = match arg {
                Some(inner) => lower_scalar(inner)?,
                None => {
                    // COUNT(*) counts rows; count over a constant 1 column is
                    // the row count regardless of nulls in any data column.
                    if *func == AggFunc::Count {
                        return Ok(TsExpr::lit_i64(1).count());
                    }
                    return Err(TsSqlError::ty("only COUNT supports a * argument"));
                }
            };
            Ok(match func {
                AggFunc::Sum => operand.sum(),
                AggFunc::Avg => operand.mean(),
                AggFunc::Min => operand.min(),
                AggFunc::Max => operand.max(),
                AggFunc::Count => operand.count(),
            })
        }
        _ => Err(TsSqlError::ty(
            "an aggregate projection must be a single aggregate call (e.g. SUM(x)); \
             arithmetic over an aggregate is out of scope",
        )),
    }
}

fn lower_literal(lit: &SqlLiteral) -> TsExpr {
    match lit {
        SqlLiteral::Int(n) => TsExpr::lit_i64(*n),
        SqlLiteral::Num(n) => TsExpr::lit_f64(*n),
        SqlLiteral::Str(s) => TsExpr::lit_str(s.clone()),
    }
}

// ---------- validation + error mapping ----------

// Up-front column existence check so an unknown column is a typed UnknownColumn
// error, not a downstream eval failure buried in a generic message.
fn validate_columns(stmt: &TsSqlStmt, source_cols: &[String]) -> Result<(), TsSqlError> {
    let known = |name: &str| source_cols.iter().any(|c| c == name);
    let mut check = |name: &str| -> Result<(), TsSqlError> {
        if known(name) {
            Ok(())
        } else {
            Err(TsSqlError::UnknownColumn(name.to_string()))
        }
    };
    for item in &stmt.projection {
        if let SelectItem::Expr { expr, .. } = item {
            check_expr_columns(expr, &mut check)?;
        }
    }
    if let Some(pred) = &stmt.filter {
        check_expr_columns(pred, &mut check)?;
    }
    for key in &stmt.group_by {
        check(key)?;
    }
    for key in &stmt.order_by {
        // An ORDER BY key may name an output alias (e.g. a computed column) as
        // well as a source column; defer the alias case to exec time. Only
        // reject a key that is neither a source column nor a projection alias.
        if !known(&key.column) && !is_output_alias(stmt, &key.column) {
            return Err(TsSqlError::UnknownColumn(key.column.clone()));
        }
    }
    Ok(())
}

fn is_output_alias(stmt: &TsSqlStmt, name: &str) -> bool {
    stmt.projection.iter().any(|item| match item {
        SelectItem::Star => false,
        SelectItem::Expr { alias, .. } => alias == name,
    })
}

fn check_expr_columns(
    expr: &SqlExpr,
    check: &mut impl FnMut(&str) -> Result<(), TsSqlError>,
) -> Result<(), TsSqlError> {
    match expr {
        SqlExpr::Column(name) => check(name),
        SqlExpr::Literal(_) => Ok(()),
        SqlExpr::Arith(_, a, b)
        | SqlExpr::Compare(_, a, b)
        | SqlExpr::And(a, b)
        | SqlExpr::Or(a, b) => {
            check_expr_columns(a, check)?;
            check_expr_columns(b, check)
        }
        SqlExpr::Not(e) => check_expr_columns(e, check),
        SqlExpr::Case {
            when,
            then,
            otherwise,
        } => {
            check_expr_columns(when, check)?;
            check_expr_columns(then, check)?;
            check_expr_columns(otherwise, check)
        }
        SqlExpr::Aggregate { arg, .. } => match arg {
            Some(inner) => check_expr_columns(inner, check),
            None => Ok(()),
        },
    }
}

fn map_lazy_err(e: subms_ts_lazy::LazyError) -> TsSqlError {
    use subms_ts_lazy::LazyError;
    match e {
        LazyError::UnknownSortColumn(c) => TsSqlError::UnknownColumn(c),
        other => TsSqlError::ty(other.to_string()),
    }
}

fn map_groupby_err(e: subms_ts_groupby::GroupByError) -> TsSqlError {
    use subms_ts_groupby::GroupByError;
    match e {
        GroupByError::UnknownColumn(c) => TsSqlError::UnknownColumn(c),
        other => TsSqlError::ty(other.to_string()),
    }
}
