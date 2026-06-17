//! Correctness tests for `subms-ts-sql`: the parser (each clause, comments,
//! case-insensitivity, errors) and the executor (SELECT / WHERE / GROUP BY /
//! ORDER BY / LIMIT, CASE, aggregates), plus a test that the GROUP BY lowering
//! matches calling `subms-ts-groupby` directly. Std-only; not harness-gated.

use subms_ts::{TsColumn, TsDataFrame, TsSeries, TsValue};
use subms_ts_expr::TsExpr;
use subms_ts_groupby::group_by;
use subms_ts_sql::ast::{AggFunc, SelectItem, SqlExpr, SqlLiteral, TsSqlStmt};
use subms_ts_sql::{TsSqlCatalog, TsSqlError, parse, query};

// ---------- fixtures ----------

fn trades_frame() -> TsDataFrame {
    let rows = [
        ("AAPL", 10.0, 190.0),
        ("MSFT", 5.0, 410.0),
        ("AAPL", 7.0, 192.0),
        ("MSFT", 3.0, 408.0),
        ("AAPL", 4.0, 188.0),
        ("GOOG", 2.0, 140.0),
    ];
    let mut symbol = TsSeries::<String>::new();
    let mut size = TsSeries::<f64>::new();
    let mut price = TsSeries::<f64>::new();
    for (i, (s, sz, px)) in rows.into_iter().enumerate() {
        symbol.push(i as i64, s.to_string()).unwrap();
        size.push(i as i64, sz).unwrap();
        price.push(i as i64, px).unwrap();
    }
    TsDataFrame::new()
        .with_column("symbol", TsColumn::Str(symbol))
        .with_column("size", TsColumn::F64(size))
        .with_column("price", TsColumn::F64(price))
}

fn catalog() -> TsSqlCatalog {
    let mut cat = TsSqlCatalog::new();
    cat.register("trades", trades_frame());
    cat
}

// Read a result cell by row position (the result's positional ts axis).
fn cell(frame: &TsDataFrame, col: &str, row: i64) -> Option<TsValue> {
    frame.column(col).and_then(|c| c.get(row))
}

fn f64_at(frame: &TsDataFrame, col: &str, row: i64) -> Option<f64> {
    match cell(frame, col, row) {
        Some(TsValue::F64(v)) => Some(v),
        _ => None,
    }
}

fn str_at(frame: &TsDataFrame, col: &str, row: i64) -> Option<String> {
    match cell(frame, col, row) {
        Some(TsValue::Str(v)) => Some(v),
        _ => None,
    }
}

fn nrows(frame: &TsDataFrame, col: &str) -> usize {
    frame.column(col).map(|c| c.len()).unwrap_or(0)
}

// ---------- parser tests ----------

#[test]
fn parse_minimal_select() {
    let stmt = parse("SELECT symbol FROM trades").unwrap();
    assert_eq!(stmt.table, "trades");
    assert_eq!(stmt.projection.len(), 1);
    assert!(stmt.filter.is_none());
    assert!(stmt.group_by.is_empty());
    assert!(stmt.order_by.is_empty());
    assert!(stmt.limit.is_none());
}

#[test]
fn parse_is_case_insensitive_on_keywords_and_preserves_identifiers() {
    // lowercase keywords, mixed-case again - identifiers keep their case.
    let stmt =
        parse("select Symbol from trades where price > 100 order by price desc limit 3").unwrap();
    match &stmt.projection[0] {
        SelectItem::Expr { expr, alias } => {
            assert_eq!(*expr, SqlExpr::Column("Symbol".to_string()));
            assert_eq!(alias, "Symbol");
        }
        _ => panic!("expected expr item"),
    }
    assert!(stmt.filter.is_some());
    assert_eq!(stmt.order_by.len(), 1);
    assert!(!stmt.order_by[0].ascending);
    assert_eq!(stmt.limit, Some(3));
}

#[test]
fn parse_handles_line_comments_and_string_literals() {
    let sql = "SELECT symbol -- the ticker\nFROM trades\nWHERE symbol = 'AA''PL' -- embedded quote";
    let stmt = parse(sql).unwrap();
    let SqlExpr::Compare(_, _, rhs) = stmt.filter.as_ref().unwrap() else {
        panic!("expected compare");
    };
    assert_eq!(
        **rhs,
        SqlExpr::Literal(SqlLiteral::Str("AA'PL".to_string()))
    );
}

#[test]
fn parse_aggregates_and_star_count() {
    let stmt =
        parse("SELECT symbol, SUM(size) AS s, COUNT(*) AS n FROM trades GROUP BY symbol").unwrap();
    assert_eq!(stmt.group_by, vec!["symbol".to_string()]);
    let aggs: Vec<&SelectItem> = stmt.projection.iter().collect();
    match aggs[1] {
        SelectItem::Expr { expr, alias } => {
            assert_eq!(alias, "s");
            assert!(matches!(
                expr,
                SqlExpr::Aggregate {
                    func: AggFunc::Sum,
                    arg: Some(_)
                }
            ));
        }
        _ => panic!(),
    }
    match aggs[2] {
        SelectItem::Expr { expr, .. } => assert!(matches!(
            expr,
            SqlExpr::Aggregate {
                func: AggFunc::Count,
                arg: None
            }
        )),
        _ => panic!(),
    }
}

#[test]
fn parse_case_and_boolean_predicate() {
    let stmt = parse(
        "SELECT CASE WHEN price > 200 THEN 1 ELSE 0 END AS hi FROM trades \
         WHERE price > 100 AND NOT symbol = 'GOOG'",
    )
    .unwrap();
    assert!(matches!(
        stmt.projection[0],
        SelectItem::Expr {
            expr: SqlExpr::Case { .. },
            ..
        }
    ));
    assert!(matches!(stmt.filter, Some(SqlExpr::And(_, _))));
}

#[test]
fn parse_rejects_garbage_and_trailing_tokens() {
    assert!(matches!(parse("SELECT"), Err(TsSqlError::Parse(_))));
    assert!(matches!(
        parse("SELECT FROM trades"),
        Err(TsSqlError::Parse(_))
    ));
    assert!(matches!(
        parse("SELECT a FROM trades EXTRA"),
        Err(TsSqlError::Parse(_))
    ));
    assert!(matches!(parse(""), Err(TsSqlError::Parse(_))));
    assert!(matches!(
        parse("SELECT a FROM trades WHERE 'unterminated"),
        Err(TsSqlError::Parse(_))
    ));
}

#[test]
fn parse_rejects_unsupported_clauses_by_name() {
    assert_eq!(
        parse("SELECT a FROM t JOIN u"),
        Err(TsSqlError::Unsupported("JOIN".to_string()))
    );
    assert_eq!(
        parse("SELECT a FROM t GROUP BY a HAVING COUNT(*) > 1"),
        Err(TsSqlError::Unsupported("HAVING".to_string()))
    );
    assert_eq!(
        parse("SELECT a FROM t LEFT JOIN u"),
        Err(TsSqlError::Unsupported("LEFT".to_string()))
    );
}

// ---------- executor tests ----------

#[test]
fn select_star_returns_every_column() {
    let out = query(&catalog(), "SELECT * FROM trades").unwrap();
    let names: Vec<&str> = out.column_names().collect();
    assert_eq!(names, vec!["symbol", "size", "price"]);
    assert_eq!(nrows(&out, "symbol"), 6);
}

#[test]
fn where_filters_rows() {
    let out = query(
        &catalog(),
        "SELECT symbol, price FROM trades WHERE price > 200",
    )
    .unwrap();
    // Only the two MSFT rows clear 200.
    assert_eq!(nrows(&out, "symbol"), 2);
    assert!((0..2).all(|r| str_at(&out, "symbol", r).as_deref() == Some("MSFT")));
}

#[test]
fn where_compound_and_or_not() {
    // price > 189 AND NOT symbol = 'AAPL'  -> MSFT rows only (both > 189).
    let out = query(
        &catalog(),
        "SELECT symbol FROM trades WHERE price > 189 AND NOT symbol = 'AAPL'",
    )
    .unwrap();
    assert_eq!(nrows(&out, "symbol"), 2);

    // symbol = 'GOOG' OR price > 409 -> GOOG + the 410 MSFT row = 2.
    let out2 = query(
        &catalog(),
        "SELECT symbol FROM trades WHERE symbol = 'GOOG' OR price > 409",
    )
    .unwrap();
    assert_eq!(nrows(&out2, "symbol"), 2);
}

#[test]
fn projection_arithmetic_and_alias() {
    let out = query(
        &catalog(),
        "SELECT symbol, size * price AS notional FROM trades WHERE symbol = 'GOOG'",
    )
    .unwrap();
    assert_eq!(nrows(&out, "notional"), 1);
    assert_eq!(f64_at(&out, "notional", 0), Some(2.0 * 140.0));
}

#[test]
fn case_when_lowers_to_conditional() {
    let out = query(
        &catalog(),
        "SELECT symbol, CASE WHEN price > 200 THEN 1 ELSE 0 END AS hi FROM trades",
    )
    .unwrap();
    // Two rows (the MSFT pair) are over 200.
    let hi_sum: i64 = (0..6)
        .filter_map(|r| match cell(&out, "hi", r) {
            Some(TsValue::I64(v)) => Some(v),
            _ => None,
        })
        .sum();
    assert_eq!(hi_sum, 2);
}

#[test]
fn order_by_and_limit_preserve_sort_order() {
    let out = query(
        &catalog(),
        "SELECT symbol, price FROM trades ORDER BY price DESC LIMIT 3",
    )
    .unwrap();
    assert_eq!(nrows(&out, "price"), 3);
    // Descending: 410, 408, 192.
    assert_eq!(f64_at(&out, "price", 0), Some(410.0));
    assert_eq!(f64_at(&out, "price", 1), Some(408.0));
    assert_eq!(f64_at(&out, "price", 2), Some(192.0));
}

#[test]
fn multi_key_order_by_is_stable_lexicographic() {
    // ORDER BY symbol ASC, price DESC: within each symbol, highest price first.
    let out = query(
        &catalog(),
        "SELECT symbol, price FROM trades ORDER BY symbol ASC, price DESC",
    )
    .unwrap();
    assert_eq!(str_at(&out, "symbol", 0).as_deref(), Some("AAPL"));
    assert_eq!(f64_at(&out, "price", 0), Some(192.0));
    assert_eq!(str_at(&out, "symbol", 3).as_deref(), Some("GOOG"));
    assert_eq!(str_at(&out, "symbol", 4).as_deref(), Some("MSFT"));
    assert_eq!(f64_at(&out, "price", 4), Some(410.0));
}

#[test]
fn group_by_aggregates_sum_avg_count() {
    let out = query(
        &catalog(),
        "SELECT symbol, SUM(size) AS total, AVG(price) AS avg_px, COUNT(*) AS n \
         FROM trades GROUP BY symbol",
    )
    .unwrap();
    // Key-sorted: AAPL, GOOG, MSFT.
    assert_eq!(str_at(&out, "symbol", 0).as_deref(), Some("AAPL"));
    assert_eq!(f64_at(&out, "total", 0), Some(21.0));
    assert_eq!(
        f64_at(&out, "avg_px", 0),
        Some((190.0 + 192.0 + 188.0) / 3.0)
    );
    assert_eq!(cell(&out, "n", 0), Some(TsValue::I64(3)));
    assert_eq!(str_at(&out, "symbol", 2).as_deref(), Some("MSFT"));
    assert_eq!(f64_at(&out, "total", 2), Some(8.0));
}

#[test]
fn group_by_min_max() {
    let out = query(
        &catalog(),
        "SELECT symbol, MIN(price) AS lo, MAX(price) AS hi FROM trades GROUP BY symbol",
    )
    .unwrap();
    // AAPL row.
    assert_eq!(f64_at(&out, "lo", 0), Some(188.0));
    assert_eq!(f64_at(&out, "hi", 0), Some(192.0));
}

#[test]
fn aggregate_without_group_by_is_whole_frame() {
    let out = query(
        &catalog(),
        "SELECT COUNT(*) AS n, SUM(size) AS total FROM trades",
    )
    .unwrap();
    assert_eq!(nrows(&out, "n"), 1);
    assert_eq!(cell(&out, "n", 0), Some(TsValue::I64(6)));
    assert_eq!(f64_at(&out, "total", 0), Some(31.0));
    // The synthetic all-key must not surface in the output.
    let names: Vec<&str> = out.column_names().collect();
    assert_eq!(names, vec!["n", "total"]);
}

#[test]
fn group_by_with_where_order_limit_composes() {
    let out = query(
        &catalog(),
        "SELECT symbol, SUM(size) AS total FROM trades WHERE price > 150 \
         GROUP BY symbol ORDER BY total DESC LIMIT 1",
    )
    .unwrap();
    assert_eq!(nrows(&out, "total"), 1);
    // price>150 keeps all AAPL (21) + all MSFT (8); top-by-total is AAPL=21.
    assert_eq!(str_at(&out, "symbol", 0).as_deref(), Some("AAPL"));
    assert_eq!(f64_at(&out, "total", 0), Some(21.0));
}

#[test]
fn group_by_lowering_matches_groupby_directly() {
    // The SQL GROUP BY must produce exactly what calling subms-ts-groupby with
    // the equivalent TsExpr aggregations produces - this pins the lowering.
    let frame = trades_frame();
    let direct = group_by(&frame, &["symbol"])
        .unwrap()
        .agg(&[
            ("total", TsExpr::col("size").sum()),
            ("n", TsExpr::lit_i64(1).count()),
        ])
        .unwrap();

    let cat = catalog();
    let via_sql = query(
        &cat,
        "SELECT symbol, SUM(size) AS total, COUNT(*) AS n FROM trades GROUP BY symbol",
    )
    .unwrap();

    assert_eq!(via_sql.column("symbol").unwrap().len(), direct.nrows());
    for g in 0..direct.nrows() {
        assert_eq!(
            cell(&via_sql, "symbol", g as i64),
            direct.value("symbol", g)
        );
        assert_eq!(cell(&via_sql, "total", g as i64), direct.value("total", g));
        assert_eq!(cell(&via_sql, "n", g as i64), direct.value("n", g));
    }
}

#[test]
fn string_key_group_by_works() {
    // A pure string-keyed group with a string symbol column.
    let out = query(
        &catalog(),
        "SELECT symbol, COUNT(*) AS n FROM trades GROUP BY symbol",
    )
    .unwrap();
    assert_eq!(nrows(&out, "symbol"), 3); // AAPL, GOOG, MSFT
    assert_eq!(str_at(&out, "symbol", 0).as_deref(), Some("AAPL"));
    assert_eq!(cell(&out, "n", 0), Some(TsValue::I64(3)));
    assert_eq!(str_at(&out, "symbol", 1).as_deref(), Some("GOOG"));
    assert_eq!(cell(&out, "n", 1), Some(TsValue::I64(1)));
}

// ---------- negative / error tests ----------

fn query_err(cat: &TsSqlCatalog, sql: &str) -> TsSqlError {
    match query(cat, sql) {
        Ok(_) => panic!("expected an error for: {sql}"),
        Err(e) => e,
    }
}

#[test]
fn unknown_table_is_typed_error() {
    assert_eq!(
        query_err(&catalog(), "SELECT a FROM nope"),
        TsSqlError::UnknownTable("nope".to_string())
    );
}

#[test]
fn unknown_column_is_typed_error() {
    assert_eq!(
        query_err(&catalog(), "SELECT missing FROM trades"),
        TsSqlError::UnknownColumn("missing".to_string())
    );
    assert_eq!(
        query_err(&catalog(), "SELECT symbol FROM trades WHERE ghost > 1"),
        TsSqlError::UnknownColumn("ghost".to_string())
    );
}

#[test]
fn non_key_non_agg_projection_with_group_by_is_rejected() {
    // `price` is neither a key nor wrapped in an aggregate -> type error.
    assert!(matches!(
        query_err(
            &catalog(),
            "SELECT symbol, price FROM trades GROUP BY symbol"
        ),
        TsSqlError::Type(_)
    ));
}

#[test]
fn star_with_group_by_is_rejected() {
    assert!(matches!(
        query_err(&catalog(), "SELECT * FROM trades GROUP BY symbol"),
        TsSqlError::Type(_)
    ));
}

#[test]
fn empty_catalog_and_table_names() {
    let cat = catalog();
    let names: Vec<&str> = cat.table_names().collect();
    assert_eq!(names, vec!["trades"]);
    assert!(cat.table("trades").is_some());
    assert!(cat.table("other").is_none());
}

#[test]
fn parse_is_exposed_for_independent_testing() {
    // parse() returns the AST without touching a catalog.
    let stmt: TsSqlStmt = parse("SELECT a, b FROM t LIMIT 10").unwrap();
    assert_eq!(stmt.limit, Some(10));
    assert_eq!(stmt.projection.len(), 2);
}
