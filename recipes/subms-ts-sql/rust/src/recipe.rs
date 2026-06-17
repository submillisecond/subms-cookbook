//! `SubMsRecipe` impl - two stages. `parse` repeatedly parses a moderate
//! grouped-aggregate query (the lexer + recursive-descent walk in isolation).
//! `query` parses + lowers + executes a `GROUP BY` with three aggregates and a
//! `WHERE` over a few-thousand-row frame (the full front-to-engine path). This
//! is the analytical front, so the contract is throughput, not per-op sub-ms:
//! the bench asserts a generous no-pathological-stall guard, documented in the
//! writeup.

use subms::{
    SubMsBenchParams, SubMsLcg, SubMsPerfHarness, SubMsRecipe, SubMsStageKind, SubMsTimer,
};
use subms_ts::{TsColumn, TsDataFrame, TsSeries};

use crate::{TsSqlCatalog, parser, query};

pub struct SqlRecipe;

const ROWS: usize = 4_096;
const CARDINALITY: u32 = 8;

const VENUES: [&str; 8] = [
    "ARCA", "BATS", "EDGX", "IEX", "NSDQ", "NYSE", "PHLX", "XCBO",
];

const PARSE_QUERY: &str = "SELECT venue, SUM(size) AS total_size, AVG(price) AS mean_price, \
COUNT(*) AS n FROM trades WHERE price > 100 GROUP BY venue ORDER BY total_size DESC LIMIT 5";
const RUN_QUERY: &str = "SELECT venue, SUM(size) AS total_size, COUNT(*) AS n FROM trades WHERE size > 1 GROUP BY venue";

fn build_catalog(seed: u64) -> TsSqlCatalog {
    let mut rng = SubMsLcg::new(seed);
    let mut venue = TsSeries::<String>::with_capacity(ROWS);
    let mut size = TsSeries::<f64>::with_capacity(ROWS);
    let mut price = TsSeries::<f64>::with_capacity(ROWS);
    for i in 0..ROWS {
        let v = VENUES[(rng.next_u32() % CARDINALITY) as usize];
        let s = ((rng.next_u32() >> 18) as f64) + 1.0;
        let p = ((rng.next_u32() >> 16) as f64) / 100.0;
        let _ = venue.push(i as i64, v.to_string());
        let _ = size.push(i as i64, s);
        let _ = price.push(i as i64, p);
    }
    let frame = TsDataFrame::new()
        .with_column("venue", TsColumn::Str(venue))
        .with_column("size", TsColumn::F64(size))
        .with_column("price", TsColumn::F64(price));
    let mut cat = TsSqlCatalog::new();
    cat.register("trades", frame);
    cat
}

impl SubMsRecipe for SqlRecipe {
    fn name(&self) -> &str {
        "subms-ts-sql"
    }

    fn run(&self, h: &mut SubMsPerfHarness, params: &SubMsBenchParams) {
        let rounds = params.entries;

        let s_parse = h.stage("parse", rounds).with_kind(SubMsStageKind::HotPath);
        for _ in 0..rounds {
            let t0 = SubMsTimer::tick();
            let stmt = parser::parse(PARSE_QUERY).expect("parse");
            s_parse.record(t0.elapsed_ns());
            std::hint::black_box(&stmt);
        }

        let cat = build_catalog(params.seed);
        let s_query = h.stage("query", rounds).with_kind(SubMsStageKind::HotPath);
        for _ in 0..rounds {
            let t0 = SubMsTimer::tick();
            let out = query(&cat, RUN_QUERY).expect("query");
            s_query.record(t0.elapsed_ns());
            std::hint::black_box(out.ncols());
        }

        h.add_meta("frame_rows", &ROWS.to_string());
        h.add_meta("key_cardinality", &CARDINALITY.to_string());
        h.add_meta("subms.workload.feature", "sql");
    }
}
