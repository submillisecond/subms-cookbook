//! The parsed-statement IR. A [`TsSqlStmt`] is the whole `SELECT` after the
//! parser has validated structure but before any lowering decision; the
//! lowerer in [`crate::lower`] walks it and decides whether the query is a
//! grouped aggregate (lower to `subms-ts-groupby`) or a row-wise projection
//! pipeline (lower to `subms-ts-lazy`). The variants are public so a caller
//! can inspect a parse without executing - `parse(sql)` returns this directly.

/// A scalar literal as written in the SQL text. The lowerer maps these onto
/// `subms-ts-expr` typed literals (`Num` -> f64, `Int` -> i64, `Str` -> string).
#[derive(Clone, Debug, PartialEq)]
pub enum SqlLiteral {
    /// An integer literal with no decimal point or exponent (`42`).
    Int(i64),
    /// A floating literal (`3.5`, `1e3`).
    Num(f64),
    /// A single-quoted string literal (`'AAPL'`).
    Str(String),
}

/// Arithmetic operators in a scalar expression.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArithOp {
    Add,
    Sub,
    Mul,
    Div,
}

/// Comparison operators in a predicate. `=`/`<>` map onto `TsExpr` eq/ne; the
/// ordered four map directly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CmpOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

/// The aggregate functions the subset supports. `Count` doubles for both
/// `COUNT(expr)` and `COUNT(*)` - the latter carries no operand.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AggFunc {
    Sum,
    Avg,
    Min,
    Max,
    Count,
}

/// A scalar / boolean expression. The same tree covers a projection scalar, a
/// `WHERE` predicate, and an aggregate operand; the lowerer enforces which
/// shapes are legal where (e.g. an aggregate may not nest inside another
/// aggregate). Comparison + boolean nodes only ever appear inside a predicate.
#[derive(Clone, Debug, PartialEq)]
pub enum SqlExpr {
    /// A column reference (`price`).
    Column(String),
    /// A literal scalar.
    Literal(SqlLiteral),
    /// Binary arithmetic over two scalar sub-expressions.
    Arith(ArithOp, Box<SqlExpr>, Box<SqlExpr>),
    /// A comparison; yields a boolean. Legal only inside a predicate.
    Compare(CmpOp, Box<SqlExpr>, Box<SqlExpr>),
    /// Boolean `AND` of two predicates.
    And(Box<SqlExpr>, Box<SqlExpr>),
    /// Boolean `OR` of two predicates.
    Or(Box<SqlExpr>, Box<SqlExpr>),
    /// Boolean `NOT` of a predicate.
    Not(Box<SqlExpr>),
    /// `CASE WHEN <pred> THEN <e> ELSE <e> END`. Lowers to `TsExpr::When`.
    Case {
        when: Box<SqlExpr>,
        then: Box<SqlExpr>,
        otherwise: Box<SqlExpr>,
    },
    /// An aggregate call. `arg` is `None` for `COUNT(*)`.
    Aggregate {
        func: AggFunc,
        arg: Option<Box<SqlExpr>>,
    },
}

impl SqlExpr {
    /// Does this expression contain an aggregate call anywhere in its tree?
    /// Drives the grouped-vs-rowwise lowering decision: a projection list with
    /// any aggregate is an aggregate query.
    pub fn contains_aggregate(&self) -> bool {
        match self {
            SqlExpr::Aggregate { .. } => true,
            SqlExpr::Column(_) | SqlExpr::Literal(_) => false,
            SqlExpr::Arith(_, a, b)
            | SqlExpr::Compare(_, a, b)
            | SqlExpr::And(a, b)
            | SqlExpr::Or(a, b) => a.contains_aggregate() || b.contains_aggregate(),
            SqlExpr::Not(e) => e.contains_aggregate(),
            SqlExpr::Case {
                when,
                then,
                otherwise,
            } => {
                when.contains_aggregate()
                    || then.contains_aggregate()
                    || otherwise.contains_aggregate()
            }
        }
    }
}

/// One item in the `SELECT` projection list.
#[derive(Clone, Debug, PartialEq)]
pub enum SelectItem {
    /// `*` - every source column, in source order.
    Star,
    /// A projected expression with its output column name. The alias is the
    /// `AS` name when present, else a derived name (the bare column name, or a
    /// synthesised `col_N` for a computed expression).
    Expr { expr: SqlExpr, alias: String },
}

/// One `ORDER BY` key.
#[derive(Clone, Debug, PartialEq)]
pub struct OrderKey {
    pub column: String,
    pub ascending: bool,
}

/// A parsed `SELECT` statement. The parser fills every clause it sees; absent
/// clauses are empty / `None`. The lowerer reads this whole struct.
#[derive(Clone, Debug, PartialEq)]
pub struct TsSqlStmt {
    pub projection: Vec<SelectItem>,
    pub table: String,
    pub filter: Option<SqlExpr>,
    pub group_by: Vec<String>,
    pub order_by: Vec<OrderKey>,
    pub limit: Option<usize>,
}
