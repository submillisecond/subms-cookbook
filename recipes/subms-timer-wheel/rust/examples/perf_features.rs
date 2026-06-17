//! Per-feature bench: runs the same 50k-entry workload against the base
//! `TimerWheel`, plus each opt-in feature (`hierarchical`, `concurrent`,
//! `deadline-scheduler`, `cron`, `metrics`) when its Cargo feature is
//! enabled at compile time.
//!
//! The output JSON has one stage block per feature variant - e.g.
//! `base_schedule`, `base_tick`, `hierarchical_schedule`, etc. - so the
//! cookbook page can fill in the per-feature p99 table without juggling
//! multiple JSON files.
//!
//! Each variant times its `schedule` path and its `tick`/`poll` drain
//! path against a deterministic LCG-driven delay stream (seed 0). The
//! deadline + cron layers use the monotonic / system-ish clocks they
//! ship rather than a `TestClock` so the recorded numbers reflect a
//! real driving workload.
//!
//! Run:
//!   cargo run --release --example perf_features \
//!       --features "harness hierarchical concurrent deadline-scheduler cron metrics"

use std::io::{self, Write};

use subms::{SubMsLcg, SubMsPerfHarness, SubMsStageKind, SubMsTimer, summarize, summary_to_json};
use subms_timer_wheel::TimerWheel;

const ENTRIES: usize = 50_000;
const SEED: u64 = 0;
const SLOTS: usize = 1024;

fn main() -> io::Result<()> {
    let mut h = SubMsPerfHarness::new("timer-wheel-features", "rust");
    h.input("entries", &ENTRIES.to_string());
    h.input("seed", &SEED.to_string());
    h.input("slots", &SLOTS.to_string());
    h.add_meta("subms.recipe.slug", "subms-timer-wheel");
    h.add_meta("subms.recipe.category", "scheduling");

    // ---------- base ----------
    {
        h.add_meta("subms.workload.feature", "base");
        let mut w: TimerWheel<u32> = TimerWheel::new(SLOTS);
        let mut rng = SubMsLcg::new(SEED);
        let s = h
            .stage("base_schedule", ENTRIES)
            .with_kind(SubMsStageKind::HotPath);
        for i in 0..ENTRIES as u32 {
            let delay = rng.bounded((SLOTS * 4) as u32) as usize;
            let t0 = SubMsTimer::tick();
            let _ = w.schedule(delay, i);
            s.record(t0.elapsed_ns());
        }
        let ticks = SLOTS * 5;
        let s = h
            .stage("base_tick", ticks)
            .with_kind(SubMsStageKind::HotPath);
        for _ in 0..ticks {
            let t0 = SubMsTimer::tick();
            let _ = w.tick();
            s.record(t0.elapsed_ns());
        }
    }

    // ---------- hierarchical ----------
    #[cfg(feature = "hierarchical")]
    {
        use subms_timer_wheel::HierarchicalTimerWheel;
        h.add_meta("subms.workload.feature", "hierarchical");
        let cap = HierarchicalTimerWheel::<u32>::max_delay() as u32;
        let mut w: HierarchicalTimerWheel<u32> = HierarchicalTimerWheel::new();
        let mut rng = SubMsLcg::new(SEED);
        let s = h
            .stage("hierarchical_schedule", ENTRIES)
            .with_kind(SubMsStageKind::HotPath);
        for i in 0..ENTRIES as u32 {
            let delay = rng.bounded(cap) as u64;
            let t0 = SubMsTimer::tick();
            let _ = w.schedule(delay, i);
            s.record(t0.elapsed_ns());
        }
        let ticks = SLOTS * 5;
        let s = h
            .stage("hierarchical_tick", ticks)
            .with_kind(SubMsStageKind::HotPath);
        for _ in 0..ticks {
            let t0 = SubMsTimer::tick();
            let _ = w.tick();
            s.record(t0.elapsed_ns());
        }
    }

    // ---------- concurrent (single-threaded path) ----------
    #[cfg(feature = "concurrent")]
    {
        use subms_timer_wheel::ConcurrentTimerWheel;
        h.add_meta("subms.workload.feature", "concurrent");
        let w: ConcurrentTimerWheel<u32> = ConcurrentTimerWheel::new(SLOTS);
        let mut rng = SubMsLcg::new(SEED);
        let s = h
            .stage("concurrent_schedule", ENTRIES)
            .with_kind(SubMsStageKind::HotPath);
        for i in 0..ENTRIES as u32 {
            let delay = rng.bounded((SLOTS * 4) as u32) as usize;
            let t0 = SubMsTimer::tick();
            let _ = w.schedule(delay, i);
            s.record(t0.elapsed_ns());
        }
        let ticks = SLOTS * 5;
        let s = h
            .stage("concurrent_tick", ticks)
            .with_kind(SubMsStageKind::HotPath);
        for _ in 0..ticks {
            let t0 = SubMsTimer::tick();
            let _ = w.tick();
            s.record(t0.elapsed_ns());
        }
    }

    // ---------- deadline-scheduler ----------
    #[cfg(feature = "deadline-scheduler")]
    {
        use std::time::Duration;
        use subms_timer_wheel::{Clock, DeadlineScheduler, MonotonicClock};
        h.add_meta("subms.workload.feature", "deadline-scheduler");
        let mut sched: DeadlineScheduler<u32, MonotonicClock> =
            DeadlineScheduler::new(SLOTS, MonotonicClock::new(), Duration::from_millis(1));
        let mut rng = SubMsLcg::new(SEED);
        // Absolute deadlines spread across a few-second horizon off a
        // monotonic origin; nanos_to_ticks maps them onto the wheel.
        let base = MonotonicClock::new().now_nanos();
        let s = h
            .stage("deadline_scheduler_schedule_at", ENTRIES)
            .with_kind(SubMsStageKind::HotPath);
        for _ in 0..ENTRIES {
            let offset_ms = rng.bounded((SLOTS * 4) as u32) as u64;
            let when = base + offset_ms * 1_000_000;
            let t0 = SubMsTimer::tick();
            let _ = sched.schedule_at(when, 0u32);
            s.record(t0.elapsed_ns());
        }
        let polls = SLOTS * 5;
        let s = h
            .stage("deadline_scheduler_poll", polls)
            .with_kind(SubMsStageKind::HotPath);
        for _ in 0..polls {
            let t0 = SubMsTimer::tick();
            let _ = sched.poll();
            s.record(t0.elapsed_ns());
        }
    }

    // ---------- cron ----------
    #[cfg(feature = "cron")]
    {
        use subms_timer_wheel::{CronSchedule, CronScheduler};
        h.add_meta("subms.workload.feature", "cron");
        // Parse the expression once, outside the timed loop - parsing is a
        // one-shot setup cost, not a per-fire cost.
        let schedule = CronSchedule::parse("*/5 * * * *").expect("valid cron expr");

        // Re-arm path: schedule a recurring fire onto the base wheel each
        // iteration, mirroring how a CronScheduler drives a wheel.
        let mut w: TimerWheel<u32> = TimerWheel::new(SLOTS);
        let mut rng = SubMsLcg::new(SEED);
        let s = h
            .stage("cron_schedule", ENTRIES)
            .with_kind(SubMsStageKind::HotPath);
        for i in 0..ENTRIES as u32 {
            let delay = rng.bounded((SLOTS * 4) as u32) as usize;
            let t0 = SubMsTimer::tick();
            let _ = w.schedule(delay, i);
            s.record(t0.elapsed_ns());
        }

        // next-match path: compute the next firing second from a rolling
        // epoch, the hot loop a CronScheduler runs on every re-arm.
        let mut cs = CronScheduler::new(schedule, 1_704_067_200);
        let mut epoch = 1_704_067_200u64;
        let s = h
            .stage("cron_next_match", ENTRIES)
            .with_kind(SubMsStageKind::HotPath);
        for _ in 0..ENTRIES {
            let t0 = SubMsTimer::tick();
            let next = cs.next_fire(epoch);
            s.record(t0.elapsed_ns());
            if let Some(n) = next {
                cs.record_fire(n);
                epoch = n;
            }
        }
    }

    // ---------- metrics ----------
    #[cfg(feature = "metrics")]
    {
        use subms_timer_wheel::MeteredTimerWheel;
        h.add_meta("subms.workload.feature", "metrics");
        let mut w: MeteredTimerWheel<u32> = MeteredTimerWheel::new(SLOTS);
        let mut rng = SubMsLcg::new(SEED);
        let s = h
            .stage("metrics_schedule", ENTRIES)
            .with_kind(SubMsStageKind::HotPath);
        for i in 0..ENTRIES as u32 {
            let delay = rng.bounded((SLOTS * 4) as u32) as usize;
            let t0 = SubMsTimer::tick();
            let _ = w.schedule(delay, i);
            s.record(t0.elapsed_ns());
        }
        let ticks = SLOTS * 5;
        let s = h
            .stage("metrics_tick", ticks)
            .with_kind(SubMsStageKind::HotPath);
        for _ in 0..ticks {
            let t0 = SubMsTimer::tick();
            let _ = w.tick();
            s.record(t0.elapsed_ns());
        }
    }

    let summary = summarize(&h);
    let mut stdout = io::stdout();
    summary_to_json(&summary, &mut stdout)?;
    writeln!(stdout)?;
    Ok(())
}
