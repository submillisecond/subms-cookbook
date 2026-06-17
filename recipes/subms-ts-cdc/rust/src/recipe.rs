//! `SubMsRecipe` impl - measure the CDC publish + receive hot paths.
//!
//! `push_notify`: append a point to a one-series collection that has a single
//! live 4096-cap subscriber, timing the push + fan-out together. `recv`: time a
//! single `try_recv` off a pre-filled ring. The ring is drained between push
//! batches so it never saturates and the drop path stays cold.

use subms::{SubMsBenchParams, SubMsPerfHarness, SubMsRecipe, SubMsStageKind, SubMsTimer};
use subms_ts::TsSeriesMetadata;

use crate::TsObservableCollection;

pub struct CdcRecipe;

const RING_CAP: usize = 4_096;
const DRAIN_EVERY: usize = 2_048;

impl SubMsRecipe for CdcRecipe {
    fn name(&self) -> &str {
        "subms-ts-cdc"
    }

    fn run(&self, h: &mut SubMsPerfHarness, params: &SubMsBenchParams) {
        let rounds = params.entries;

        let mut obs = TsObservableCollection::<f64>::new();
        let mut sub = obs.subscribe(RING_CAP);
        let id = obs
            .register(TsSeriesMetadata::new(1, "bench"))
            .expect("register");

        let s_push = h
            .stage("push_notify", rounds)
            .with_kind(SubMsStageKind::HotPath);
        let mut ts = 0i64;
        for i in 0..rounds {
            ts += 1;
            let t0 = SubMsTimer::tick();
            obs.push(id, ts, ts as f64).expect("push");
            s_push.record(t0.elapsed_ns());
            // Keep the ring well under capacity so the drop path stays cold and
            // push_notify measures the steady-state fan-out, not a saturated ring.
            if i % DRAIN_EVERY == DRAIN_EVERY - 1 {
                while sub.try_recv().is_some() {}
            }
        }
        while sub.try_recv().is_some() {}

        // Pre-fill the ring up to capacity, then time individual receives.
        let recv_rounds = rounds.min(RING_CAP - 1);
        let s_recv = h
            .stage("recv", recv_rounds)
            .with_kind(SubMsStageKind::HotPath);
        for i in 0..recv_rounds {
            ts += 1;
            obs.push(id, ts, ts as f64).expect("push");
            let _ = i;
        }
        for _ in 0..recv_rounds {
            let t0 = SubMsTimer::tick();
            let ev = sub.try_recv();
            s_recv.record(t0.elapsed_ns());
            std::hint::black_box(ev);
        }

        h.add_meta("ring_capacity", &RING_CAP.to_string());
        h.add_meta("dropped_events", &obs.dropped_events().to_string());
        h.add_meta("subms.workload.feature", "cdc-publish-recv");
    }
}
