//! `subms-ts-sql` - a zero-dependency, hand-rolled SQL-subset parser and
//! executor over the typed [`TsDataFrame`](subms_ts::TsDataFrame) engine. It
//! parses a `SELECT` against a named catalog of frames and runs it by LOWERING
//! each clause onto the operator recipes already in the arc: row-wise
//! projection / filter / sort / limit ride a `subms-ts-lazy` pipeline,
//! `GROUP BY` + aggregates lower to `subms-ts-groupby`, and every scalar /
//! predicate / aggregate is a `subms-ts-expr` IR tree. There is no execution
//! engine of its own - SQL is a front-end syntax over the analytical layer.
//!
//! The whole thing is std-only: a byte-cursor lexer, a recursive-descent
//! parser, and a clause lowerer. It is the SQL analogue of the `subms-ts-promql`
//! recipe (a hand-rolled query-language front-end with zero external deps).
//!
//! Supported subset:
//!
//! ```text
//! SELECT <proj> FROM <table>
//!   [WHERE <predicate>]
//!   [GROUP BY <col, ...>]
//!   [ORDER BY <col> [ASC|DESC], ...]
//!   [LIMIT <n>]
//! ```
//!
//! `<proj>` items: `*`, a column, an arithmetic expression, `expr AS alias`,
//! `CASE WHEN <pred> THEN <e> ELSE <e> END`, and the aggregates
//! `SUM/AVG/MIN/MAX/COUNT(expr)` / `COUNT(*)`. Predicates compose
//! `= <> < <= > >=` with `AND`/`OR`/`NOT` and parentheses. Keywords are
//! case-insensitive; identifiers keep their case; string literals are
//! single-quoted; `--` starts a line comment.
//!
//! ```
//! use subms_ts::{TsColumn, TsDataFrame, TsSeries, TsValue};
//! use subms_ts_sql::{query, TsSqlCatalog};
//!
//! let mut symbol = TsSeries::<String>::new();
//! let mut size = TsSeries::<f64>::new();
//! for (i, (s, sz)) in [("AAPL", 10.0), ("MSFT", 5.0), ("AAPL", 7.0)].into_iter().enumerate() {
//!     symbol.push(i as i64, s.to_string()).unwrap();
//!     size.push(i as i64, sz).unwrap();
//! }
//! let frame = TsDataFrame::new()
//!     .with_column("symbol", TsColumn::Str(symbol))
//!     .with_column("size", TsColumn::F64(size));
//!
//! let mut cat = TsSqlCatalog::new();
//! cat.register("trades", frame);
//!
//! let out = query(&cat, "SELECT symbol, SUM(size) AS total FROM trades GROUP BY symbol").unwrap();
//! // one row per symbol, key-sorted: AAPL -> 17, MSFT -> 5.
//! assert_eq!(out.column("total").unwrap().get(0), Some(TsValue::F64(17.0)));
//! assert_eq!(out.column("total").unwrap().get(1), Some(TsValue::F64(5.0)));
//! ```
//!
//! # Non-claims
//!
//! This is a single-table analytical subset, not a SQL engine. Out of scope:
//! `JOIN`, `HAVING`, subqueries, window functions, set operations, DDL/DML, and
//! arithmetic wrapped around an aggregate (`SUM(x) / 2`). Each is rejected with
//! an explicit [`TsSqlError::Unsupported`] / [`TsSqlError::Type`] naming the
//! gap rather than silently ignored.

use std::collections::BTreeMap;

use subms_ts::TsDataFrame;

pub mod ast;
mod lower;
pub mod parser;

#[cfg(feature = "harness")]
pub mod recipe;

pub use ast::TsSqlStmt;
pub use parser::parse;

/// What can go wrong running a query. A malformed query is a
/// [`TsSqlError::Parse`]; a clause outside the subset is
/// [`TsSqlError::Unsupported`] (naming the clause); an unknown table / column
/// is the matching variant; a type / shape error the lowerer cannot reconcile
/// is [`TsSqlError::Type`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TsSqlError {
    Parse(String),
    UnknownTable(String),
    UnknownColumn(String),
    Unsupported(String),
    Type(String),
}

impl TsSqlError {
    pub(crate) fn parse(msg: impl Into<String>) -> Self {
        TsSqlError::Parse(msg.into())
    }

    pub(crate) fn unsupported(clause: impl Into<String>) -> Self {
        TsSqlError::Unsupported(clause.into())
    }

    pub(crate) fn ty(msg: impl Into<String>) -> Self {
        TsSqlError::Type(msg.into())
    }
}

impl std::fmt::Display for TsSqlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TsSqlError::Parse(m) => write!(f, "sql parse error: {m}"),
            TsSqlError::UnknownTable(t) => write!(f, "unknown table: {t}"),
            TsSqlError::UnknownColumn(c) => write!(f, "unknown column: {c}"),
            TsSqlError::Unsupported(c) => write!(f, "unsupported clause: {c}"),
            TsSqlError::Type(m) => write!(f, "type error: {m}"),
        }
    }
}

impl std::error::Error for TsSqlError {}

/// A name -> [`TsDataFrame`] registry. A query's `FROM <name>` resolves against
/// this. The catalog owns its frames so a query borrows them read-only; the
/// same catalog can back many queries.
#[derive(Default)]
pub struct TsSqlCatalog {
    tables: BTreeMap<String, TsDataFrame>,
}

impl TsSqlCatalog {
    pub fn new() -> Self {
        Self {
            tables: BTreeMap::new(),
        }
    }

    /// Register `frame` under `name`, replacing any frame already bound there.
    pub fn register(&mut self, name: impl Into<String>, frame: TsDataFrame) {
        self.tables.insert(name.into(), frame);
    }

    /// The frame bound to `name`, if any.
    pub fn table(&self, name: &str) -> Option<&TsDataFrame> {
        self.tables.get(name)
    }

    /// The registered table names, in sorted order.
    pub fn table_names(&self) -> impl Iterator<Item = &str> {
        self.tables.keys().map(|s| s.as_str())
    }
}

/// Parse + execute `sql` against `catalog`, returning the result as a
/// [`TsDataFrame`]. The result frame's columns are the projection's output
/// names, in `SELECT`-list order; an aggregate query emits one row per group
/// (key columns first, then the aggregate columns).
pub fn query(catalog: &TsSqlCatalog, sql: &str) -> Result<TsDataFrame, TsSqlError> {
    let stmt = parse(sql)?;
    let source = catalog
        .table(&stmt.table)
        .ok_or_else(|| TsSqlError::UnknownTable(stmt.table.clone()))?;
    lower::execute(&stmt, source)
}
