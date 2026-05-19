//! `SubMsRecipe` impl.

use std::time::Instant;

use subms::{SubMsBenchParams, SubMsLcg, SubMsPerfHarness, SubMsRecipe};

use crate::TimerWheel;

pub struct TimerWheelRecipe;

impl SubMsRecipe for TimerWheelRecipe {
    fn name(&self) -> &str {
        "timer-wheel"
    }

    fn run(&self, h: &mut SubMsPerfHarness, params: &SubMsBenchParams) {
        let entries = params.entries;
        let warmup = params.warmup;
        let seed = params.seed;
        let slots = 1024usize;
        let mut w: TimerWheel<u32> = TimerWheel::new(slots);

        let mut rng = SubMsLcg::new(seed);
        for i in 0..warmup as u32 {
            let _ = w.schedule(rng.bounded((slots * 4) as u32) as usize, i);
        }

        let s_sched = h.stage("schedule", entries);
        let mut rng = SubMsLcg::new(seed.wrapping_add(1));
        let mut ids = Vec::with_capacity(entries);
        for i in 0..entries as u32 {
            let delay = rng.bounded((slots * 4) as u32) as usize;
            let t0 = Instant::now();
            let id = w.schedule(delay, i);
            s_sched.record(t0.elapsed().as_nanos() as u64);
            ids.push(id);
        }

        // Cancel half of the scheduled timers.
        let s_cancel = h.stage("cancel", entries / 2);
        for &id in ids.iter().step_by(2) {
            let t0 = Instant::now();
            let _ = w.cancel(id);
            s_cancel.record(t0.elapsed().as_nanos() as u64);
        }

        // Drain ticks.
        let s_tick = h.stage("tick", slots * 5);
        for _ in 0..(slots * 5) {
            let t0 = Instant::now();
            let _ = w.tick();
            s_tick.record(t0.elapsed().as_nanos() as u64);
        }

        h.add_meta("slots", &slots.to_string());
    }
}
