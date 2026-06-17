//! The expression IR. [`TsExpr`] is an immutable tree the evaluator walks
//! column-at-a-time over a [`TsDataFrame`](subms_ts::TsDataFrame). The builders
//! (`col`, the typed `lit_*` family, the fluent `.add` / `.gt` / `.mean` family,
//! the free `when`) are the ergonomic way most callers construct it; the
//! variants are public so downstream recipes (the lazy planner, groupby, window)
//! can pattern-match and rewrite.
//!
//! A node's element type is inferred at eval time from the frame's column types
//! and the literal types, not declared here - the IR is a type-erased tree, and
//! [`eval`](crate::eval) is where the type rules live.

use subms_ts::TsValue;

/// Elementwise unary operators. Numeric only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TsUnaryOp {
    Neg,
    Abs,
}

/// Elementwise binary arithmetic operators. Numeric only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TsBinaryOp {
    Add,
    Sub,
    Mul,
    Div,
}

/// Elementwise comparison operators. Each produces a `Bool` array.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TsCmpOp {
    Lt,
    Le,
    Eq,
    Ne,
    Ge,
    Gt,
}

/// Reductions over the valid cells of an operand. Each broadcasts its scalar
/// result back to every row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TsAggOp {
    Sum,
    Min,
    Max,
    Mean,
    Count,
}

/// An expression node. Construct via the builders; downstream recipes may match
/// on the variants directly to rewrite or compile the tree.
#[derive(Clone, Debug, PartialEq)]
pub enum TsExpr {
    /// Reference a frame column by name. Its type is the column's `TsDataType`.
    Col(String),
    /// A typed scalar literal (`F64` / `I64` / `Bool` / `Str`), broadcast to
    /// every row.
    Lit(TsValue),
    /// Elementwise unary op over its operand.
    Unary(TsUnaryOp, Box<TsExpr>),
    /// Elementwise binary op over its two operands.
    Binary(TsBinaryOp, Box<TsExpr>, Box<TsExpr>),
    /// Elementwise comparison; yields a `Bool` array.
    Compare(TsCmpOp, Box<TsExpr>, Box<TsExpr>),
    /// Elementwise select: where `cond` is `true` take `then`, else `otherwise`.
    When {
        cond: Box<TsExpr>,
        then: Box<TsExpr>,
        otherwise: Box<TsExpr>,
    },
    /// Reduce the operand to a scalar, then broadcast it to every row.
    Agg(TsAggOp, Box<TsExpr>),
}

// The builder names (add / sub / mul / div / eq) intentionally mirror the
// Polars / pandas expression vocabulary; the clippy lint that flags them as
// confusable with std::ops is the wrong call for a deliberate DSL surface.
#[allow(clippy::should_implement_trait)]
impl TsExpr {
    /// Reference a frame column by name.
    pub fn col(name: impl Into<String>) -> TsExpr {
        TsExpr::Col(name.into())
    }

    /// An `f64` literal.
    pub fn lit_f64(value: f64) -> TsExpr {
        TsExpr::Lit(TsValue::F64(value))
    }

    /// An `i64` literal.
    pub fn lit_i64(value: i64) -> TsExpr {
        TsExpr::Lit(TsValue::I64(value))
    }

    /// A `bool` literal.
    pub fn lit_bool(value: bool) -> TsExpr {
        TsExpr::Lit(TsValue::Bool(value))
    }

    /// A string literal.
    pub fn lit_str(value: impl Into<String>) -> TsExpr {
        TsExpr::Lit(TsValue::Str(value.into()))
    }

    pub fn neg(self) -> TsExpr {
        TsExpr::Unary(TsUnaryOp::Neg, Box::new(self))
    }

    pub fn abs(self) -> TsExpr {
        TsExpr::Unary(TsUnaryOp::Abs, Box::new(self))
    }

    pub fn add(self, rhs: TsExpr) -> TsExpr {
        TsExpr::Binary(TsBinaryOp::Add, Box::new(self), Box::new(rhs))
    }

    pub fn sub(self, rhs: TsExpr) -> TsExpr {
        TsExpr::Binary(TsBinaryOp::Sub, Box::new(self), Box::new(rhs))
    }

    pub fn mul(self, rhs: TsExpr) -> TsExpr {
        TsExpr::Binary(TsBinaryOp::Mul, Box::new(self), Box::new(rhs))
    }

    pub fn div(self, rhs: TsExpr) -> TsExpr {
        TsExpr::Binary(TsBinaryOp::Div, Box::new(self), Box::new(rhs))
    }

    pub fn lt(self, rhs: TsExpr) -> TsExpr {
        TsExpr::Compare(TsCmpOp::Lt, Box::new(self), Box::new(rhs))
    }

    pub fn le(self, rhs: TsExpr) -> TsExpr {
        TsExpr::Compare(TsCmpOp::Le, Box::new(self), Box::new(rhs))
    }

    pub fn eq(self, rhs: TsExpr) -> TsExpr {
        TsExpr::Compare(TsCmpOp::Eq, Box::new(self), Box::new(rhs))
    }

    pub fn ne(self, rhs: TsExpr) -> TsExpr {
        TsExpr::Compare(TsCmpOp::Ne, Box::new(self), Box::new(rhs))
    }

    pub fn ge(self, rhs: TsExpr) -> TsExpr {
        TsExpr::Compare(TsCmpOp::Ge, Box::new(self), Box::new(rhs))
    }

    pub fn gt(self, rhs: TsExpr) -> TsExpr {
        TsExpr::Compare(TsCmpOp::Gt, Box::new(self), Box::new(rhs))
    }

    pub fn sum(self) -> TsExpr {
        TsExpr::Agg(TsAggOp::Sum, Box::new(self))
    }

    pub fn min(self) -> TsExpr {
        TsExpr::Agg(TsAggOp::Min, Box::new(self))
    }

    pub fn max(self) -> TsExpr {
        TsExpr::Agg(TsAggOp::Max, Box::new(self))
    }

    pub fn mean(self) -> TsExpr {
        TsExpr::Agg(TsAggOp::Mean, Box::new(self))
    }

    pub fn count(self) -> TsExpr {
        TsExpr::Agg(TsAggOp::Count, Box::new(self))
    }
}

/// Elementwise select. Where `cond` is `true` take `then`, else `otherwise`;
/// `then` and `otherwise` must share a type. The free function reads closer to
/// the SQL / Polars `when(...).then(...).otherwise(...)` it mirrors than a
/// method chain would.
pub fn when(cond: TsExpr, then: TsExpr, otherwise: TsExpr) -> TsExpr {
    TsExpr::When {
        cond: Box::new(cond),
        then: Box::new(then),
        otherwise: Box::new(otherwise),
    }
}
