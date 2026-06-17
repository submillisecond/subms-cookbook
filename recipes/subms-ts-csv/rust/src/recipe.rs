//! `SubMsRecipe` impl - parse a fixed CSV block into a [`TsDataFrame`]
//! (`read`) and emit it back to text (`write`). The block is a few typed
//! columns over a few thousand rows, regenerated per round from the seed so
//! the parse path sees fresh bytes each iteration.

use subms::{
    SubMsBenchParams, SubMsLcg, SubMsPerfHarness, SubMsRecipe, SubMsStageKind, SubMsTimer,
};

use crate::{TsCsvOptions, read_csv, write_csv};

pub struct CsvRecipe;

const ROWS: usize = 4_096;

/// Build a deterministic CSV block: an i64 ts column, an i64 counter, an f64
/// price, a bool flag, and a short str tag. Mirrors the Java recipe's columns
/// so the two read/write the same shape.
fn build_csv(seed: u64) -> String {
    let mut rng = SubMsLcg::new(seed);
    let mut s = String::with_capacity(ROWS * 24);
    s.push_str("t,count,price,ok,tag\n");
    for i in 0..ROWS {
        let count = (rng.next_u32() >> 16) as u64;
        let price = (rng.next_u32() >> 8) as f64 / 256.0;
        let ok = (rng.next_u32() & 1) == 1;
        let tag = if ok { "up" } else { "dn" };
        s.push_str(&i.to_string());
        s.push(',');
        s.push_str(&count.to_string());
        s.push(',');
        s.push_str(&format!("{price:.3}"));
        s.push(',');
        s.push_str(if ok { "true" } else { "false" });
        s.push(',');
        s.push_str(tag);
        s.push('\n');
    }
    s
}

impl SubMsRecipe for CsvRecipe {
    fn name(&self) -> &str {
        "subms-ts-csv"
    }

    fn run(&self, h: &mut SubMsPerfHarness, params: &SubMsBenchParams) {
        let rounds = params.entries;
        let text = build_csv(params.seed);
        let opts = TsCsvOptions::new().ts_column("t");

        let s_read = h.stage("read", rounds).with_kind(SubMsStageKind::HotPath);
        for _ in 0..rounds {
            let t0 = SubMsTimer::tick();
            let df = read_csv(&text, &opts).expect("parse");
            s_read.record(t0.elapsed_ns());
            std::hint::black_box(df.ncols());
        }

        // a frame to write back: the row-index axis path (no ts column), so the
        // aligned view walks one row per index.
        let df = read_csv(&text, &TsCsvOptions::new()).expect("parse");
        let s_write = h.stage("write", rounds).with_kind(SubMsStageKind::HotPath);
        for _ in 0..rounds {
            let t0 = SubMsTimer::tick();
            let out = write_csv(&df);
            s_write.record(t0.elapsed_ns());
            std::hint::black_box(out.len());
        }

        h.add_meta("rows", &ROWS.to_string());
        h.add_meta("subms.workload.feature", "csv");
    }
}
