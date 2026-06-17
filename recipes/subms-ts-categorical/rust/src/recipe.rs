//! `SubMsRecipe` impl - time the two hot-path optimizer ops: an interner
//! `intern` (one hash probe, occasional insert) over a stream of mostly
//! duplicate symbols, and a column `encode` (build the dictionary + code
//! array) over a freshly grown string series. `intern` is amortised O(1);
//! `encode` is O(n) in the column it processes. The bench confirms the per-op
//! p99 stays an order of magnitude under the 1 ms budget.

use subms::{
    SubMsBenchParams, SubMsLcg, SubMsPerfHarness, SubMsRecipe, SubMsStageKind, SubMsTimer,
};
use subms_ts::TsSeries;

use crate::{TsDictColumn, TsStringInterner};

pub struct CategoricalRecipe;

// A small fixed alphabet: the realistic shape for a categorical column (a
// handful of tickers / regions / statuses). The interner stays warm after the
// first few rounds and every later intern is a probe-hit.
const ALPHABET: &[&str] = &[
    "AAPL", "MSFT", "GOOG", "AMZN", "NVDA", "META", "TSLA", "NFLX",
];

// Points per encoded column. Big enough that the encode stage does real O(n)
// work, small enough that one column build stays well under the budget.
const COLUMN_POINTS: usize = 1_024;

impl SubMsRecipe for CategoricalRecipe {
    fn name(&self) -> &str {
        "subms-ts-categorical"
    }

    fn run(&self, h: &mut SubMsPerfHarness, params: &SubMsBenchParams) {
        let rounds = params.entries;
        let mut rng = SubMsLcg::new(params.seed);

        // intern stage: a single interner kept warm across the run, fed a
        // stream of mostly-duplicate symbols. After the alphabet is seen once
        // every op is a probe-hit, the steady-state hot path.
        let s_intern = h.stage("intern", rounds).with_kind(SubMsStageKind::HotPath);
        let mut interner = TsStringInterner::with_capacity(ALPHABET.len());
        let mut sink = 0u64;
        for _ in 0..rounds {
            let s = ALPHABET[(rng.next_u32() as usize) % ALPHABET.len()];
            let t0 = SubMsTimer::tick();
            let id = interner.intern(s);
            s_intern.record(t0.elapsed_ns());
            sink = sink.wrapping_add(id as u64);
        }
        std::hint::black_box(sink);

        // encode stage: dict-encode a fresh COLUMN_POINTS-long string series
        // each round. Distinct values stay bounded by the alphabet, so the
        // dictionary is tiny and the cost is the O(n) code build.
        let s_encode = h.stage("encode", rounds).with_kind(SubMsStageKind::HotPath);
        for _ in 0..rounds {
            let mut series = TsSeries::<String>::with_capacity(COLUMN_POINTS);
            for i in 0..COLUMN_POINTS {
                let s = ALPHABET[(rng.next_u32() as usize) % ALPHABET.len()];
                let _ = series.push(i as i64, s.to_string());
            }
            let t0 = SubMsTimer::tick();
            let col = TsDictColumn::encode(&series);
            s_encode.record(t0.elapsed_ns());
            std::hint::black_box(col.cardinality());
        }

        h.add_meta("alphabet_size", &ALPHABET.len().to_string());
        h.add_meta("column_points", &COLUMN_POINTS.to_string());
        h.add_meta("subms.workload.feature", "categorical");
    }
}
