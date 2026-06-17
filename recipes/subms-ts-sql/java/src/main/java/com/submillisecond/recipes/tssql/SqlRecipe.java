package com.submillisecond.recipes.tssql;

import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsRecipe;
import com.submillisecond.perf.SubMsStageKind;
import com.submillisecond.perf.SubMsTimer;
import com.submillisecond.recipes.ts.TsColumn;
import com.submillisecond.recipes.ts.TsDataFrame;
import com.submillisecond.recipes.ts.TsSeries;
import com.submillisecond.recipes.ts.TsSeriesD;
import com.submillisecond.recipes.tssql.Ast.TsSqlStmt;

/**
 * Drives a representative subms-ts-sql workload. Two stages mirror the Rust
 * recipe: {@code parse} repeatedly parses a moderate grouped-aggregate query;
 * {@code query} parses + lowers + executes a GROUP BY with two aggregates and a
 * WHERE over a few-thousand-row frame. Throughput-contracted: each timed
 * {@code query} sample is the full front-to-engine path, not a single op.
 */
public final class SqlRecipe implements SubMsRecipe {

    private static final int ROWS = 4_096;
    private static final int CARDINALITY = 8;

    private static final String[] VENUES = {
        "ARCA", "BATS", "EDGX", "IEX", "NSDQ", "NYSE", "PHLX", "XCBO"
    };

    private static final String PARSE_QUERY =
            "SELECT venue, SUM(size) AS total_size, AVG(price) AS mean_price, COUNT(*) AS n "
            + "FROM trades WHERE price > 100 GROUP BY venue ORDER BY total_size DESC LIMIT 5";
    private static final String RUN_QUERY =
            "SELECT venue, SUM(size) AS total_size, COUNT(*) AS n FROM trades "
            + "WHERE size > 1 GROUP BY venue";

    // Mirrors subms::SubMsLcg (incl. the seed | 1 guard) so the workload's
    // pseudo-random draws match the Rust recipe's drive sequence.
    private static final class Lcg {
        private long state;

        Lcg(long seed) {
            this.state = seed | 1L;
        }

        long nextU32() {
            state = state * 6364136223846793005L + 1442695040888963407L;
            return (state >>> 32) & 0xffffffffL;
        }
    }

    private static TsSqlCatalog buildCatalog(long seed) {
        Lcg rng = new Lcg(seed);
        TsSeries<String> venue = TsSeries.withCapacity(ROWS);
        TsSeriesD size = TsSeriesD.withCapacity(ROWS);
        TsSeriesD price = TsSeriesD.withCapacity(ROWS);
        for (int i = 0; i < ROWS; i++) {
            String v = VENUES[(int) (rng.nextU32() % CARDINALITY)];
            double s = (double) (rng.nextU32() >>> 18) + 1.0;
            double p = (rng.nextU32() >>> 16) / 100.0;
            venue.push(i, v);
            size.push(i, s);
            price.push(i, p);
        }
        TsDataFrame frame = new TsDataFrame()
                .withColumn("venue", new TsColumn.Str(venue))
                .withColumn("size", new TsColumn.F64(size))
                .withColumn("price", new TsColumn.F64(price));
        TsSqlCatalog cat = new TsSqlCatalog();
        cat.register("trades", frame);
        return cat;
    }

    @Override
    public String name() {
        return "subms-ts-sql";
    }

    @Override
    public void run(SubMsPerfHarness h, SubMsBenchParams params) {
        int rounds = params.entries();
        long sink = 0;

        SubMsPerfHarness.Stage sParse = h.stage("parse", rounds).withKind(SubMsStageKind.HOT_PATH);
        for (int r = 0; r < rounds; r++) {
            long t0 = SubMsTimer.nanosNow();
            TsSqlStmt stmt = Parser.parse(PARSE_QUERY);
            sParse.record(SubMsTimer.nanosNow() - t0);
            sink += stmt.projection().size();
        }

        TsSqlCatalog cat = buildCatalog(params.seed());
        SubMsPerfHarness.Stage sQuery = h.stage("query", rounds).withKind(SubMsStageKind.HOT_PATH);
        for (int r = 0; r < rounds; r++) {
            long t0 = SubMsTimer.nanosNow();
            TsDataFrame out = TsSql.query(cat, RUN_QUERY);
            sQuery.record(SubMsTimer.nanosNow() - t0);
            sink += out.ncols();
        }
        BLACK_HOLE = sink;

        h.meta("frame_rows", Integer.toString(ROWS));
        h.meta("key_cardinality", Integer.toString(CARDINALITY));
        h.meta("subms.workload.feature", "sql");
    }

    static volatile long BLACK_HOLE;
}
