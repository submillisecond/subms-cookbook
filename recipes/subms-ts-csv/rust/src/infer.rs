//! Per-column narrowest-fit type inference. Scan a column's non-empty cells
//! and pick the tightest element type that fits ALL of them: `I64` if every
//! cell is a valid `i64`, else `F64` if every cell is a valid `f64`, else
//! `Bool` if every cell is `true`/`false` (ASCII-case-insensitive), else
//! `Str`. An all-empty (or empty) column infers `Str`.

/// The element type the reader picks for a column. Maps onto a `TsColumn`
/// variant (`Value` is never inferred - it is the caller's escape hatch).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum TsInferredType {
    I64,
    F64,
    Bool,
    Str,
}

/// Does this cell parse as an `i64`? Mirrors `str::parse::<i64>` exactly so the
/// build-time parse cannot disagree with the inference-time check: leading
/// `+`/`-`, ASCII digits, no underscores, no whitespace, in `i64` range.
fn is_i64(cell: &str) -> bool {
    cell.parse::<i64>().is_ok()
}

/// Does this cell parse as a finite `f64`? A token that parses to an infinity
/// or NaN ("inf", "nan") is rejected here so it does not later degrade to a
/// gap - such a column falls through to `Str`, which preserves the literal.
fn is_f64(cell: &str) -> bool {
    matches!(cell.parse::<f64>(), Ok(v) if v.is_finite())
}

fn is_bool(cell: &str) -> bool {
    cell.eq_ignore_ascii_case("true") || cell.eq_ignore_ascii_case("false")
}

/// Infer the column type from an iterator over its non-empty cells. Single
/// pass: track whether the all-i64, all-f64, all-bool invariants still hold;
/// the result is the tightest one still standing. An i64 also satisfies f64,
/// so the i64 flag is the strict narrowing.
pub fn infer<'a>(cells: impl Iterator<Item = &'a str>) -> TsInferredType {
    let mut any = false;
    let mut all_i64 = true;
    let mut all_f64 = true;
    let mut all_bool = true;

    for cell in cells {
        any = true;
        if all_i64 && !is_i64(cell) {
            all_i64 = false;
        }
        if all_f64 && !is_f64(cell) {
            all_f64 = false;
        }
        if all_bool && !is_bool(cell) {
            all_bool = false;
        }
        if !all_i64 && !all_f64 && !all_bool {
            return TsInferredType::Str;
        }
    }

    if !any {
        return TsInferredType::Str;
    }
    if all_i64 {
        TsInferredType::I64
    } else if all_f64 {
        TsInferredType::F64
    } else if all_bool {
        TsInferredType::Bool
    } else {
        TsInferredType::Str
    }
}
