//! `SubMsRecipe` impl - join two 4,096-row frames on a shared STRING `sym` key.
//! Two stages: `hash_inner` runs an inner hash join (the common case);
//! `hash_outer` runs a full outer hash join (every left + right row, missing
//! cells filled via validity). Throughput-contracted: each timed sample is a
//! whole-frame join, not a single probe.

use subms::{
    SubMsBenchParams, SubMsLcg, SubMsPerfHarness, SubMsRecipe, SubMsStageKind, SubMsTimer,
};
use subms_ts::{TsColumn, TsDataFrame, TsSeries};

use crate::{TsJoinKind, hash_join};

pub struct JoinRecipe;

const ROWS: usize = 4_096;

// Two frames that overlap on about half their string key space, so an inner
// join keeps roughly half the rows and an outer join emits the unmatched
// remainder on both sides - a realistic one-to-one-ish join keyed on a symbol
// string, not a degenerate all-match.
fn build_frames(seed: u64) -> (TsDataFrame, TsDataFrame) {
    let mut rng = SubMsLcg::new(seed);

    let mut sym_l = TsSeries::<String>::with_capacity(ROWS);
    let mut px = TsSeries::<f64>::with_capacity(ROWS);
    for i in 0..ROWS {
        let _ = sym_l.push(i as i64, format!("S{i:06}"));
        let _ = px.push(i as i64, (rng.next_u32() >> 16) as f64);
    }
    let left = TsDataFrame::new()
        .with_column("sym", TsColumn::Str(sym_l))
        .with_column("px", TsColumn::F64(px));

    let mut sym_r = TsSeries::<String>::with_capacity(ROWS);
    let mut qty = TsSeries::<f64>::with_capacity(ROWS);
    for i in 0..ROWS {
        // shift the right key space by half so ~half the symbols match.
        let _ = sym_r.push(i as i64, format!("S{:06}", i + ROWS / 2));
        let _ = qty.push(i as i64, (rng.next_u32() >> 16) as f64);
    }
    let right = TsDataFrame::new()
        .with_column("sym", TsColumn::Str(sym_r))
        .with_column("qty", TsColumn::F64(qty));

    (left, right)
}

impl SubMsRecipe for JoinRecipe {
    fn name(&self) -> &str {
        "subms-ts-join"
    }

    fn run(&self, h: &mut SubMsPerfHarness, params: &SubMsBenchParams) {
        let rounds = params.entries;
        let (left, right) = build_frames(params.seed);

        let s_inner = h
            .stage("hash_inner", rounds)
            .with_kind(SubMsStageKind::HotPath);
        for _ in 0..rounds {
            let t0 = SubMsTimer::tick();
            let out = hash_join(&left, &right, &["sym"], &["sym"], TsJoinKind::Inner).unwrap();
            s_inner.record(t0.elapsed_ns());
            std::hint::black_box(out.nrows());
        }

        let s_outer = h
            .stage("hash_outer", rounds)
            .with_kind(SubMsStageKind::HotPath);
        for _ in 0..rounds {
            let t0 = SubMsTimer::tick();
            let out = hash_join(&left, &right, &["sym"], &["sym"], TsJoinKind::Outer).unwrap();
            s_outer.record(t0.elapsed_ns());
            std::hint::black_box(out.nrows());
        }

        h.add_meta("frame_rows", &ROWS.to_string());
        h.add_meta("subms.workload.feature", "equi-join");
        h.add_meta("subms.workload.key_type", "str");
    }
}
