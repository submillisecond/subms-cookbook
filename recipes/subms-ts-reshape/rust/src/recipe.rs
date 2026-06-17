//! `SubMsRecipe` impl - reshape a 4,096-row frame two ways. `pivot` runs a
//! long-to-wide pivot of an (index, STRING category, value) frame into a roughly
//! 256-row by 16-column grid; `melt` unpivots a wide (id, v0..v3) frame into the
//! long form with the `Str` `variable` column. Throughput-contracted: each timed
//! sample is a whole-frame reshape, not a single op.

use subms::{
    SubMsBenchParams, SubMsLcg, SubMsPerfHarness, SubMsRecipe, SubMsStageKind, SubMsTimer,
};
use subms_ts::{TsColumn, TsDataFrame, TsSeries};

use crate::{melt, pivot, PivotAgg};

pub struct ReshapeRecipe;

const ROWS: usize = 4_096;
const INDEX_CARD: u64 = 256;
const CATEGORY_CARD: u64 = 16;

// A long-form frame: index in [0, 256), a STRING category in {"c00".."c15"}, a
// random value. Pivoting it yields a dense-ish 256x16 grid (most cells
// populated), the realistic shape for a sensor-by-symbol rollup - and the
// category is a real string column, not a numeric stand-in.
fn build_long(seed: u64) -> TsDataFrame {
    let mut rng = SubMsLcg::new(seed);
    let mut index = TsSeries::<i64>::with_capacity(ROWS);
    let mut category = TsSeries::<String>::with_capacity(ROWS);
    let mut value = TsSeries::<f64>::with_capacity(ROWS);
    for i in 0..ROWS {
        let idx = (rng.next_u32() as u64 % INDEX_CARD) as i64;
        let cat = format!("c{:02}", rng.next_u32() as u64 % CATEGORY_CARD);
        let val = (rng.next_u32() >> 16) as f64;
        let _ = index.push(i as i64, idx);
        let _ = category.push(i as i64, cat);
        let _ = value.push(i as i64, val);
    }
    TsDataFrame::new()
        .with_column("index", TsColumn::I64(index))
        .with_column("category", TsColumn::Str(category))
        .with_column("value", TsColumn::F64(value))
}

// A wide frame: an i64 id plus four f64 value columns. Melting it to long form
// emits ROWS * 4 rows, each carrying the source column name in a Str column.
fn build_wide(seed: u64) -> TsDataFrame {
    let mut rng = SubMsLcg::new(seed);
    let mut id = TsSeries::<i64>::with_capacity(ROWS);
    let mut v0 = TsSeries::<f64>::with_capacity(ROWS);
    let mut v1 = TsSeries::<f64>::with_capacity(ROWS);
    let mut v2 = TsSeries::<f64>::with_capacity(ROWS);
    let mut v3 = TsSeries::<f64>::with_capacity(ROWS);
    for i in 0..ROWS {
        let _ = id.push(i as i64, i as i64);
        let _ = v0.push(i as i64, (rng.next_u32() >> 16) as f64);
        let _ = v1.push(i as i64, (rng.next_u32() >> 16) as f64);
        let _ = v2.push(i as i64, (rng.next_u32() >> 16) as f64);
        let _ = v3.push(i as i64, (rng.next_u32() >> 16) as f64);
    }
    TsDataFrame::new()
        .with_column("id", TsColumn::I64(id))
        .with_column("v0", TsColumn::F64(v0))
        .with_column("v1", TsColumn::F64(v1))
        .with_column("v2", TsColumn::F64(v2))
        .with_column("v3", TsColumn::F64(v3))
}

impl SubMsRecipe for ReshapeRecipe {
    fn name(&self) -> &str {
        "subms-ts-reshape"
    }

    fn run(&self, h: &mut SubMsPerfHarness, params: &SubMsBenchParams) {
        let rounds = params.entries;
        let long = build_long(params.seed);
        let wide = build_wide(params.seed);

        let s_pivot = h.stage("pivot", rounds).with_kind(SubMsStageKind::HotPath);
        for _ in 0..rounds {
            let t0 = SubMsTimer::tick();
            let out = pivot(&long, "index", "category", "value", PivotAgg::Sum).unwrap();
            s_pivot.record(t0.elapsed_ns());
            std::hint::black_box(out.nrows());
        }

        let s_melt = h.stage("melt", rounds).with_kind(SubMsStageKind::HotPath);
        for _ in 0..rounds {
            let t0 = SubMsTimer::tick();
            let out = melt(&wide, &["id"], &["v0", "v1", "v2", "v3"]).unwrap();
            s_melt.record(t0.elapsed_ns());
            std::hint::black_box(out.nrows());
        }

        h.add_meta("frame_rows", &ROWS.to_string());
        h.add_meta("subms.workload.feature", "reshape");
        h.add_meta("subms.workload.category_type", "str");
    }
}
