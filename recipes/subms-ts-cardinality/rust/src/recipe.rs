//! `SubMsRecipe` impl - time the two hot-path admission decisions: a guard
//! `admit` (counter compare + bump or reject) and a dedup `is_new` (one hash
//! set probe + insert). Both are O(1); the bench confirms the p99 stays an
//! order of magnitude under the 1 ms budget.

use subms::{
    SubMsBenchParams, SubMsLcg, SubMsPerfHarness, SubMsRecipe, SubMsStageKind, SubMsTimer,
};

use crate::{TsCardinalityGuard, TsDedupFilter, TsIngestKey, TsOverflowPolicy};

pub struct CardinalityRecipe;

// Wide enough that the guard refills (admit + occasional release) and the
// dedup set holds a realistic working set rather than degenerating to one key.
const CAP: usize = 8_192;

impl SubMsRecipe for CardinalityRecipe {
    fn name(&self) -> &str {
        "subms-ts-cardinality"
    }

    fn run(&self, h: &mut SubMsPerfHarness, params: &SubMsBenchParams) {
        let rounds = params.entries;
        let mut rng = SubMsLcg::new(params.seed);

        // admit stage: an Allow guard so the decision is always counter
        // arithmetic (no early-out branch), the worst case for the timing.
        let s_admit = h.stage("admit", rounds).with_kind(SubMsStageKind::HotPath);
        let mut guard = TsCardinalityGuard::new(CAP, TsOverflowPolicy::Allow);
        for _ in 0..rounds {
            let t0 = SubMsTimer::tick();
            let r = guard.admit();
            s_admit.record(t0.elapsed_ns());
            std::hint::black_box(&r);
            if guard.count() >= CAP {
                guard.release();
            }
        }

        // dedup stage: half the keys are fresh, half are replays of a recent
        // key, so the hash set sees both the insert and the probe-hit path.
        let s_dedup = h.stage("dedup", rounds).with_kind(SubMsStageKind::HotPath);
        let mut filter = TsDedupFilter::with_capacity(rounds);
        let mut seq: u64 = 0;
        for i in 0..rounds {
            let key = if i % 2 == 0 {
                seq += 1;
                TsIngestKey::new((rng.next_u32() % 256) as u64, seq)
            } else {
                TsIngestKey::new((rng.next_u32() % 256) as u64, seq)
            };
            let t0 = SubMsTimer::tick();
            let fresh = filter.is_new(key);
            s_dedup.record(t0.elapsed_ns());
            std::hint::black_box(fresh);
        }

        h.add_meta("cardinality_cap", &CAP.to_string());
        h.add_meta("subms.workload.feature", "cardinality");
    }
}
