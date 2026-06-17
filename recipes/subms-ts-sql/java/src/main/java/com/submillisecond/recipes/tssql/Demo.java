package com.submillisecond.recipes.tssql;

import com.submillisecond.recipes.ts.TsColumn;
import com.submillisecond.recipes.ts.TsDataFrame;
import com.submillisecond.recipes.ts.TsSeries;
import com.submillisecond.recipes.ts.TsSeriesD;
import com.submillisecond.recipes.ts.TsValue;

/**
 * A short stdout demo: build a trades frame, register it, and run a grouped
 * aggregate plus a row-wise filtered projection. Std/JDK-only - no harness.
 */
public final class Demo {

    private Demo() {}

    public static void main(String[] args) {
        Object[][] rows = {
            {"AAPL", 10.0, 190.0},
            {"MSFT", 5.0, 410.0},
            {"AAPL", 7.0, 192.0},
            {"MSFT", 3.0, 408.0},
            {"AAPL", 4.0, 188.0},
        };
        TsSeries<String> symbol = new TsSeries<>();
        TsSeriesD size = new TsSeriesD();
        TsSeriesD price = new TsSeriesD();
        for (int i = 0; i < rows.length; i++) {
            symbol.push(i, (String) rows[i][0]);
            size.push(i, (Double) rows[i][1]);
            price.push(i, (Double) rows[i][2]);
        }
        TsDataFrame frame = new TsDataFrame()
                .withColumn("symbol", new TsColumn.Str(symbol))
                .withColumn("size", new TsColumn.F64(size))
                .withColumn("price", new TsColumn.F64(price));

        TsSqlCatalog cat = new TsSqlCatalog();
        cat.register("trades", frame);

        TsDataFrame grouped = TsSql.query(cat,
                "SELECT symbol, SUM(size) AS total, AVG(price) AS avg_px "
                + "FROM trades GROUP BY symbol ORDER BY total DESC");
        System.out.println("grouped by symbol (total size desc):");
        int g = grouped.column("symbol").map(TsColumn::len).orElse(0);
        for (int r = 0; r < g; r++) {
            System.out.printf("  %6s  total=%6s  avg_px=%s%n",
                    cell(grouped, "symbol", r), cell(grouped, "total", r),
                    cell(grouped, "avg_px", r));
        }

        TsDataFrame notional = TsSql.query(cat,
                "SELECT symbol, size * price AS notional FROM trades "
                + "WHERE price > 189 ORDER BY notional DESC LIMIT 3");
        System.out.println("\ntop notional where price > 189:");
        int n = notional.column("notional").map(TsColumn::len).orElse(0);
        for (int r = 0; r < n; r++) {
            System.out.printf("  %6s  notional=%s%n",
                    cell(notional, "symbol", r), cell(notional, "notional", r));
        }
    }

    private static String cell(TsDataFrame frame, String column, int row) {
        return frame.column(column)
                .flatMap(c -> c.get(row))
                .map(Demo::render)
                .orElse("-");
    }

    private static String render(TsValue v) {
        if (v instanceof TsValue.F64 d) {
            return Double.toString(d.value());
        }
        if (v instanceof TsValue.I64 l) {
            return Long.toString(l.value());
        }
        if (v instanceof TsValue.Str s) {
            return s.value();
        }
        return v.toString();
    }
}
