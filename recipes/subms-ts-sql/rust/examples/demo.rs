//! A short stdout demo: build a trades frame, register it, and run a grouped
//! aggregate plus a row-wise filtered projection. Std-only - no harness.

use subms_ts::{TsColumn, TsDataFrame, TsSeries, TsValue};
use subms_ts_sql::{TsSqlCatalog, query};

fn main() {
    let rows = [
        ("AAPL", 10.0, 190.0),
        ("MSFT", 5.0, 410.0),
        ("AAPL", 7.0, 192.0),
        ("MSFT", 3.0, 408.0),
        ("AAPL", 4.0, 188.0),
    ];
    let mut symbol = TsSeries::<String>::new();
    let mut size = TsSeries::<f64>::new();
    let mut price = TsSeries::<f64>::new();
    for (i, (sym, sz, px)) in rows.into_iter().enumerate() {
        symbol.push(i as i64, sym.to_string()).unwrap();
        size.push(i as i64, sz).unwrap();
        price.push(i as i64, px).unwrap();
    }
    let frame = TsDataFrame::new()
        .with_column("symbol", TsColumn::Str(symbol))
        .with_column("size", TsColumn::F64(size))
        .with_column("price", TsColumn::F64(price));

    let mut cat = TsSqlCatalog::new();
    cat.register("trades", frame);

    let grouped = query(
        &cat,
        "SELECT symbol, SUM(size) AS total, AVG(price) AS avg_px \
         FROM trades GROUP BY symbol ORDER BY total DESC",
    )
    .unwrap();
    println!("grouped by symbol (total size desc):");
    for r in 0..grouped.column("symbol").map(|c| c.len()).unwrap_or(0) {
        let sym = cell(&grouped, "symbol", r);
        let total = cell(&grouped, "total", r);
        let avg = cell(&grouped, "avg_px", r);
        println!("  {sym:>6}  total={total:>6}  avg_px={avg}");
    }

    let notional = query(
        &cat,
        "SELECT symbol, size * price AS notional FROM trades \
         WHERE price > 189 ORDER BY notional DESC LIMIT 3",
    )
    .unwrap();
    println!("\ntop notional where price > 189:");
    for r in 0..notional.column("notional").map(|c| c.len()).unwrap_or(0) {
        println!(
            "  {:>6}  notional={}",
            cell(&notional, "symbol", r),
            cell(&notional, "notional", r)
        );
    }
}

fn cell(frame: &TsDataFrame, column: &str, row: usize) -> String {
    let col = match frame.column(column) {
        Some(c) => c,
        None => return "-".to_string(),
    };
    // The demo frames are small; the row index maps to the synthetic ts the
    // result builder assigned (0-based monotonic), so read at ts == row.
    match col.get(row as i64) {
        Some(TsValue::F64(v)) => format!("{v}"),
        Some(TsValue::I64(v)) => format!("{v}"),
        Some(TsValue::Str(s)) => s,
        Some(other) => format!("{other:?}"),
        None => "-".to_string(),
    }
}
