//! Feature classification bench. Each feature's representative op is swept
//! across three RESIDENT-TIMER counts, `classify_feature` DECIDES the category
//! from the shape of that sweep, and the decision plus a measured `p99ByStage`
//! is merge-written into `.subms/features/rust.json`.
//!
//! Resident timers is the sweep axis because it is the only thing that can make
//! a wheel op superlinear: `schedule` and the base wheel's `cancel` are O(1) by
//! construction, and `tick` walks one bucket, whose occupancy is
//! resident/slots. The slot count is held fixed throughout so a slope has one
//! cause.
//!
//! The workload is built so the tick measurement is honest:
//!
//!   - DUE_PER_TICK timers fire on EVERY measured tick. Ticking a wheel with
//!     nothing due measures an empty bucket walk and would publish the cheap
//!     path.
//!   - The due rate is FIXED across sweep points. Scheduling the resident
//!     population uniformly over the horizon instead would scale the fire rate
//!     with N, and the sweep would be reading the workload, not the wheel.
//!   - The resident population is scheduled BEYOND the measured window, so it
//!     never fires and occupancy holds constant for the whole run.
//!
//! Run:
//!   cargo run --release --example perf_features \
//!       --features "harness hierarchical concurrent deadline-scheduler cron metrics"

use std::collections::BTreeMap;
use std::io::{self, Write};
use std::path::PathBuf;

use subms::{SubMsFeatureManifest, SubMsP99Source, SubMsPerfHarness, classify_feature, summarize};
use subms_timer_wheel::TimerWheel;

/// Resident (scheduled, not yet due) timers. A 16x span starting at 32768: the
/// base wheel's bucket holds resident/SLOTS entries, so the smallest point
/// already walks 128 of them and the per-call cost does not dominate the walk.
const SIZES: [usize; 3] = [32_768, 131_072, 524_288];
const CANON: usize = SIZES[SIZES.len() - 1];
/// 256 and not the 1024 the standing bench uses. A tick's cost is a fixed
/// per-call part (take the bucket, rebuild the survivors vec) plus a per-entry
/// part, and the sweep only measures the second once occupancy is large enough
/// to dominate. At 1024 slots the smallest sweep point walks 32 entries, the
/// fixed part is two thirds of it, and the ratio compresses to the point where
/// the Java port's `poll` measured exactly 8.5x over 16x - the classifier's
/// threshold, decided by a rounding. 256 slots quadruples occupancy at the same
/// resident count, which is the cheap way to start the sweep an octave up.
const SLOTS: usize = 256;
/// Timers due on each tick. Fixed across sweep points - see the module note.
const DUE_PER_TICK: usize = 1;
/// Untimed ticks before the measured window. Large enough that the Java port's
/// tick path is fully C2-compiled before the first sweep point is measured - a
/// point measured while still interpreted reads SLOW, which the sweep sees as a
/// curve that falls with size. Kept identical in both ports so the workload is
/// the same.
const WARM_TICKS: usize = 20_000;
const TIMED_TICKS: usize = 4_096;
const DUE_TICKS: usize = WARM_TICKS + TIMED_TICKS;
/// Resident delays sit above the measured window and below the hierarchical
/// wheel's 262144-tick capacity, so one workload builder drives both wheels.
const HORIZON: usize = 262_000;
/// Timed reps for a keyed op. Kept well under the smallest sweep point: the
/// timed schedules add to the resident population, and at 10k against 32768
/// that inflation is small enough not to flatten the size axis.
const OPS: usize = 10_000;
/// Untimed reps before a keyed measurement, run against a scratch instance so
/// the warm-up does not itself change the resident count being swept.
const WARM_OPS: usize = 50_000;
/// Ops per timed sample in a SWEEP. A single `schedule` costs tens of
/// nanoseconds and this platform's timer quantum is 100 ns, so an unbatched p50
/// is pinned to one or two quanta and the category is decided by rounding: two
/// runs of unchanged code put `metrics/schedule` at p50 100 and p50 300, which
/// classified auxiliary and then hot-path. 64 and not 16 because the base op at
/// 16 still measures 800 ns - one quantum is 12% of that, which is the whole
/// margin the classifier's base-delta test works in, and `metrics` flapped
/// across it. At 64 the quantum is under 3% of the sample. Every sweep and the
/// base op use the same batch, so the base-delta test compares like with like;
/// `p99ByStage` is measured separately at batch 1 and stays a true per-op
/// figure.
const BATCH: usize = 64;
/// Timed reps for a whole-structure op, far too slow to run OPS times. 256 and
/// not 32: at 32 reps the reported p99 IS the max, so one preemption of a 736 us
/// cancel published 5.2 ms and moved run to run.
const BULK_REPS: usize = 256;
/// Bulk warm-up is TIME-BOXED, not a fixed rep count. Rust has no JIT, but a
/// wheel op allocates (a fresh survivors vec per tick, a fired vec per drain)
/// and the allocator ramp does not settle in a handful of reps.
const BULK_WARM_NANOS: u64 = 300_000_000;
const BULK_WARM_MAX_REPS: usize = 5_000;

const TICK_NS: u64 = 1_000_000;

#[derive(Clone, Copy, Default)]
struct M {
    p50: u64,
    p99: u64,
    max: u64,
}

fn stat(h: &SubMsPerfHarness) -> M {
    summarize(h)
        .stages
        .iter()
        .find(|s| s.name == "op")
        .map_or(M::default(), |s| M {
            p50: s.p50_ns,
            p99: s.p99_ns,
            max: s.max_ns,
        })
}

/// Delay for the j-th resident timer. Above the measured window so it never
/// fires, spread evenly so bucket occupancy is uniform across the wheel.
fn resident_delay(j: usize) -> usize {
    let span = HORIZON - DUE_TICKS - 1;
    DUE_TICKS + 1 + (j % span)
}

/// Loads a wheel with the due stream and `n` resident timers via the caller's
/// schedule adapter, which is all the two wheel types differ by here.
fn load<W>(w: &mut W, n: usize, mut sched: impl FnMut(&mut W, usize)) {
    for t in 1..=DUE_TICKS {
        for _ in 0..DUE_PER_TICK {
            sched(w, t);
        }
    }
    for j in 0..n {
        sched(w, resident_delay(j));
    }
}

/// A per-op measurement. `warm` runs against a scratch instance: warming on the
/// measured instance would add WARM_OPS entries to the resident population and
/// compress the size axis at the small end of the sweep.
///
/// The warm-up goes THROUGH the harness's timed wrapper, not around it. Warming
/// the op alone leaves the wrapper itself cold, and at batch 64 a measurement
/// only enters it OPS/64 times - 156, far short of what the Java port needs to
/// compile it. That showed up as the first keyed measurements of a run reading
/// 5400 ns and later ones 1600 ns, and as `concurrent/schedule` sweeping
/// DOWNWARD across sizes, which is the under-warm signature rather than a
/// feature that gets cheaper with more timers.
fn keyed(batch: usize, mut warm: impl FnMut(usize), mut op: impl FnMut(usize)) -> M {
    let mut wh = SubMsPerfHarness::new("timer-feature-warm", "rust");
    let wst = wh.stage("op", WARM_OPS);
    for i in 0..WARM_OPS {
        wst.time(|| warm(i));
    }
    let samples = OPS / batch;
    let mut h = SubMsPerfHarness::new("timer-feature", "rust");
    let st = h.stage("op", samples);
    for s in 0..samples {
        let first = s * batch;
        st.time(|| {
            for k in 0..batch {
                op(first + k);
            }
        });
    }
    stat(&h)
}

/// Ticks a loaded wheel. The warm ticks are untimed and the due stream covers
/// them, so the measured region sees the same fire rate and the same occupancy
/// as the warm region.
fn drain<W>(batch: usize, mut w: W, mut tick: impl FnMut(&mut W)) -> M {
    for _ in 0..WARM_TICKS {
        tick(&mut w);
    }
    let samples = TIMED_TICKS / batch;
    let mut h = SubMsPerfHarness::new("timer-feature", "rust");
    let st = h.stage("op", samples);
    for _ in 0..samples {
        st.time(|| {
            for _ in 0..batch {
                tick(&mut w);
            }
        });
    }
    stat(&h)
}

/// A whole-structure op, repeated against one input built outside the timed
/// region. Only safe for a NON-destructive op - every use here is a cancel of
/// an id that does not exist, which walks the same buckets every rep.
fn bulk<W>(mut w: W, mut op: impl FnMut(&mut W)) -> M {
    let start = std::time::Instant::now();
    for _ in 0..BULK_WARM_MAX_REPS {
        op(&mut w);
        if start.elapsed().as_nanos() as u64 >= BULK_WARM_NANOS {
            break;
        }
    }
    let mut h = SubMsPerfHarness::new("timer-feature", "rust");
    let st = h.stage("op", BULK_REPS);
    for _ in 0..BULK_REPS {
        st.time(|| op(&mut w));
    }
    stat(&h)
}

/// Sweeps and PRINTS the curve, p50 / p99 / max at every point. The classifier
/// reads p50; the other two are here because a ratio-compressed or
/// non-monotonic curve classifies flat and the only way to catch one is to look
/// at the rows.
fn sweep(label: &str, mut at: impl FnMut(usize) -> M) -> Vec<(usize, u64)> {
    let ms: Vec<(usize, M)> = SIZES.iter().map(|&n| (n, at(n))).collect();
    let cells: Vec<String> = ms
        .iter()
        .map(|(n, m)| format!("({n}: p50 {} p99 {} max {})", m.p50, m.p99, m.max))
        .collect();
    eprintln!("sweep {label}: {}", cells.join(" "));
    ms.iter().map(|(n, m)| (*n, m.p50)).collect()
}

fn base_wheel(n: usize) -> TimerWheel<u32> {
    let mut w: TimerWheel<u32> = TimerWheel::new(SLOTS);
    load(&mut w, n, |w, d| {
        w.schedule(d, 0);
    });
    w
}

/// The baseline: base `schedule`, the O(1) per-op write every feature either
/// decorates or replaces. Re-measured immediately before EACH feature is
/// classified rather than once at the top. Measured once, it sits several
/// half-million-timer builds away from the feature it is compared against, and
/// on this host that gap moves it between 3000 and 4300 ns run to run - as large
/// as a real feature delta. `metrics`, whose entire cost is one u64 increment,
/// flipped between auxiliary and hot-path on that drift alone; measured
/// adjacent, both runs land on auxiliary.
fn base_p50() -> u64 {
    let mut scratch: TimerWheel<u32> = TimerWheel::new(SLOTS);
    let mut w = base_wheel(CANON);
    let m = keyed(
        BATCH,
        |i| {
            scratch.schedule(resident_delay(i), 0);
        },
        |i| {
            w.schedule(resident_delay(i), 0);
        },
    );
    eprintln!("base schedule: p50 {} p99 {} max {}", m.p50, m.p99, m.max);
    m.p50
}

fn main() -> io::Result<()> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(".subms")
        .join("features")
        .join("rust.json");
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    let mut manifest = SubMsFeatureManifest::load_str("rust", &existing);
    // Stamp the box these numbers came from. The bench runs wherever it is
    // invoked, so an unstamped manifest is indistinguishable from a fleet
    // capture; the renderer will not publish one it cannot attribute.
    let (source, instance) = SubMsP99Source::from_env();
    manifest.set_p99_source(source, instance.as_deref());

    // Diagnostic, not a feature: the base wheel's own tick. A single-level
    // wheel decrements the rounds counter of every entry in the bucket it
    // walks, fired or not, so its tick is O(resident/slots) - the cost the
    // hierarchical feature exists to remove. Printed so the feature curves
    // below have something to be read against.
    sweep("base/tick", |n| {
        drain(BATCH, base_wheel(n), |w| {
            let _ = w.tick();
        })
    });

    // ---------- hierarchical: cascade across three 64-slot wheels ----------
    #[cfg(feature = "hierarchical")]
    {
        use subms_timer_wheel::HierarchicalTimerWheel;

        fn hier(n: usize) -> HierarchicalTimerWheel<u32> {
            let mut w: HierarchicalTimerWheel<u32> = HierarchicalTimerWheel::new();
            load(&mut w, n, |w, d| {
                w.schedule(d as u64, 0);
            });
            w
        }

        // Swept on `tick`, the op the feature transforms. The cascade is the
        // expensive path and it fires on 1 tick in 64 (level 1) and 1 in 4096
        // (level 2), so the measured window has to be long enough to contain
        // both: 4096 timed ticks contains 64 level-1 cascades and one level-2.
        //
        // The curve is flat, and that is the correct reading rather than a
        // hidden cost: a cascade moves the entries in ONE coarse bucket, which
        // holds the timers due in the next 64 (level 1) or 4096 (level 2)
        // ticks. With the due rate held fixed that bucket's size is fixed too,
        // so resident timers further out cost the tick nothing. This is exactly
        // what the level structure buys - the base wheel's own tick, printed
        // above, walks resident/slots entries on EVERY tick.
        let sw = sweep("hierarchical/tick", |n| {
            drain(BATCH, hier(n), |w| {
                let _ = w.tick();
            })
        });

        // `cancel` is the O(resident) op the feature introduces. It has no
        // id->slot index (the base wheel's index would need patching on every
        // cascade) so it sweeps all 192 buckets and every entry in them.
        // Cancelling a MISS walks all of them and is non-destructive, which is
        // what makes it safe to repeat against one input.
        sweep("hierarchical/cancel-miss", |n| {
            bulk(hier(n), |w| {
                let _ = w.cancel(u64::MAX);
            })
        });

        // PINNED structural on the strength of `cancel`, not of the swept op.
        // From the source, `HierarchicalTimerWheel::cancel` iterates
        // LEVELS * SLOTS buckets and every entry in each until it matches, so
        // it is O(resident) - measured 29x over a 16x sweep, 1.0 ms p99 at
        // 524288 resident. The base wheel does not have that op shape: it keeps
        // an id->slot map and cancels in O(bucket). Classifying the feature
        // hot-path off a flat `tick` would tell a reader every op it introduces
        // is safe per-operation, and one of them lands on the millisecond line
        // at half a million timers.
        let (cat, reason) = classify_feature(
            &sw,
            Some(base_p50()),
            Some(subms::SubMsFeatureCategory::Structural),
        );

        let mut p99 = BTreeMap::new();
        p99.insert(
            "tick".to_string(),
            drain(1, hier(CANON), |w| {
                let _ = w.tick();
            })
            .p99,
        );
        p99.insert(
            "schedule".to_string(),
            {
                let mut scratch: HierarchicalTimerWheel<u32> = HierarchicalTimerWheel::new();
                let mut w = hier(CANON);
                keyed(
                    1,
                    |i| {
                        scratch.schedule(resident_delay(i) as u64, 0);
                    },
                    |i| {
                        w.schedule(resident_delay(i) as u64, 0);
                    },
                )
            }
            .p99,
        );
        p99.insert(
            "cancel".to_string(),
            bulk(hier(CANON), |w| {
                let _ = w.cancel(u64::MAX);
            })
            .p99,
        );
        manifest.set_feature("hierarchical", cat, &p99, &reason);
    }

    // ---------- concurrent: short-mutex wrapper ----------
    #[cfg(feature = "concurrent")]
    {
        use subms_timer_wheel::ConcurrentTimerWheel;

        fn conc(n: usize) -> ConcurrentTimerWheel<u32> {
            let mut w: ConcurrentTimerWheel<u32> = ConcurrentTimerWheel::new(SLOTS);
            load(&mut w, n, |w, d| {
                w.schedule(d, 0);
            });
            w
        }

        // Swept on `schedule` and measured single-threaded. The feature adds a
        // lock acquire and release to every op; running it contended would
        // measure the contention instead of the indirection, and the thread
        // count would then be a second thing varying across the sweep.
        let sw = sweep("concurrent/schedule", |n| {
            let scratch: ConcurrentTimerWheel<u32> = ConcurrentTimerWheel::new(SLOTS);
            let w = conc(n);
            keyed(
                BATCH,
                |i| {
                    scratch.schedule(resident_delay(i), 0);
                },
                |i| {
                    w.schedule(resident_delay(i), 0);
                },
            )
        });
        let (cat, reason) = classify_feature(&sw, Some(base_p50()), None);

        let mut p99 = BTreeMap::new();
        p99.insert(
            "schedule".to_string(),
            {
                let scratch: ConcurrentTimerWheel<u32> = ConcurrentTimerWheel::new(SLOTS);
                let w = conc(CANON);
                keyed(
                    1,
                    |i| {
                        scratch.schedule(resident_delay(i), 0);
                    },
                    |i| {
                        w.schedule(resident_delay(i), 0);
                    },
                )
            }
            .p99,
        );
        p99.insert(
            "tick".to_string(),
            drain(1, conc(CANON), |w| {
                let _ = w.tick();
            })
            .p99,
        );
        manifest.set_feature("concurrent", cat, &p99, &reason);
    }

    // ---------- deadline-scheduler: absolute deadlines over an injected clock ----------
    #[cfg(feature = "deadline-scheduler")]
    {
        use std::cell::Cell;
        use std::rc::Rc;
        use std::time::Duration;
        use subms_timer_wheel::{Clock, DeadlineScheduler};

        /// Time only moves when the bench moves it. A free-running clock makes
        /// `poll` tick however many ticks the host happened to take, which is
        /// neither repeatable nor comparable across sweep points; a frozen one
        /// makes `poll` a no-op and publishes an empty drain as the cost.
        struct StepClock {
            now: Cell<u64>,
            step: Cell<u64>,
        }
        struct Shared(Rc<StepClock>);
        impl Clock for Shared {
            fn now_nanos(&self) -> u64 {
                self.0.now.set(self.0.now.get() + self.0.step.get());
                self.0.now.get()
            }
        }

        fn sched(n: usize) -> (DeadlineScheduler<u32, Shared>, Rc<StepClock>) {
            let clock = Rc::new(StepClock {
                now: Cell::new(0),
                step: Cell::new(0),
            });
            let mut s: DeadlineScheduler<u32, Shared> = DeadlineScheduler::new(
                SLOTS,
                Shared(Rc::clone(&clock)),
                Duration::from_nanos(TICK_NS),
            );
            load(&mut s, n, |s, d| {
                s.schedule_at(d as u64 * TICK_NS, 0);
            });
            (s, clock)
        }

        // Swept on `poll`, the op the layer introduces. With the clock stepped
        // exactly one tick per call, a poll is one wheel tick plus the deadline
        // arithmetic, so the sweep reads the drain the layer is driving.
        let sw = sweep("deadline-scheduler/poll", |n| {
            let (s, clock) = sched(n);
            clock.step.set(TICK_NS);
            drain(BATCH, s, |s| {
                let _ = s.poll();
            })
        });
        let (cat, reason) = classify_feature(&sw, Some(base_p50()), None);

        let mut p99 = BTreeMap::new();
        p99.insert(
            "schedule_at".to_string(),
            {
                let (mut scratch, _sc) = sched(0);
                let (mut s, _c) = sched(CANON);
                keyed(
                    1,
                    |i| {
                        scratch.schedule_at(resident_delay(i) as u64 * TICK_NS, 0);
                    },
                    |i| {
                        s.schedule_at(resident_delay(i) as u64 * TICK_NS, 0);
                    },
                )
            }
            .p99,
        );
        p99.insert(
            "poll".to_string(),
            {
                let (s, clock) = sched(CANON);
                clock.step.set(TICK_NS);
                drain(1, s, |s| {
                    let _ = s.poll();
                })
            }
            .p99,
        );
        manifest.set_feature("deadline-scheduler", cat, &p99, &reason);
    }

    // ---------- cron: 5-field expression parser + next-fire search ----------
    #[cfg(feature = "cron")]
    {
        use subms_timer_wheel::{CronSchedule, CronScheduler};
        const EXPR: &str = "*/5 * * * *";
        const EPOCH0: u64 = 1_704_067_200;

        // Swept on `next_fire`, the op the feature introduces. It searches
        // forward minute by minute from a rolling epoch and never touches a
        // wheel, so it is expected to read FLAT against resident timers - that
        // is the correct result for this feature, not a broken sweep.
        let sw = sweep("cron/next_fire", |_n| {
            let mut warm =
                CronScheduler::new(CronSchedule::parse(EXPR).expect("valid cron expr"), EPOCH0);
            let mut warm_epoch = EPOCH0;
            let mut cs =
                CronScheduler::new(CronSchedule::parse(EXPR).expect("valid cron expr"), EPOCH0);
            let mut epoch = EPOCH0;
            keyed(
                BATCH,
                |_| {
                    if let Some(n) = warm.next_fire(warm_epoch) {
                        warm.record_fire(n);
                        warm_epoch = n;
                    }
                },
                |_| {
                    let next = cs.next_fire(epoch);
                    if let Some(n) = next {
                        cs.record_fire(n);
                        epoch = n;
                    }
                },
            )
        });
        let (cat, reason) = classify_feature(&sw, Some(base_p50()), None);

        let mut p99 = BTreeMap::new();
        p99.insert(
            "parse".to_string(),
            keyed(
                1,
                |_| {
                    let _ = CronSchedule::parse(EXPR);
                },
                |_| {
                    let _ = CronSchedule::parse(EXPR);
                },
            )
            .p99,
        );
        p99.insert(
            "next_fire".to_string(),
            {
                let mut cs =
                    CronScheduler::new(CronSchedule::parse(EXPR).expect("valid cron expr"), EPOCH0);
                let mut epoch = EPOCH0;
                let mut warm =
                    CronScheduler::new(CronSchedule::parse(EXPR).expect("valid cron expr"), EPOCH0);
                let mut warm_epoch = EPOCH0;
                keyed(
                    1,
                    |_| {
                        if let Some(n) = warm.next_fire(warm_epoch) {
                            warm.record_fire(n);
                            warm_epoch = n;
                        }
                    },
                    |_| {
                        let next = cs.next_fire(epoch);
                        if let Some(n) = next {
                            cs.record_fire(n);
                            epoch = n;
                        }
                    },
                )
            }
            .p99,
        );
        manifest.set_feature("cron", cat, &p99, &reason);
    }

    // ---------- metrics: per-instance counters ----------
    #[cfg(feature = "metrics")]
    {
        use subms_timer_wheel::MeteredTimerWheel;

        fn metered(n: usize) -> MeteredTimerWheel<u32> {
            let mut w: MeteredTimerWheel<u32> = MeteredTimerWheel::new(SLOTS);
            load(&mut w, n, |w, d| {
                w.schedule(d, 0);
            });
            w
        }

        // Swept on `schedule`. The counters are the feature and they sit on the
        // per-op path; sweeping `tick` instead would measure the base wheel's
        // bucket walk and attribute it to a pair of u64 increments. The tick
        // number is still recorded below so it is visible.
        let sw = sweep("metrics/schedule", |n| {
            let mut scratch: MeteredTimerWheel<u32> = MeteredTimerWheel::new(SLOTS);
            let mut w = metered(n);
            keyed(
                BATCH,
                |i| {
                    scratch.schedule(resident_delay(i), 0);
                },
                |i| {
                    w.schedule(resident_delay(i), 0);
                },
            )
        });
        // PINNED auxiliary. From the source, `MeteredTimerWheel::schedule` is
        // one non-atomic increment of an owned u64 field followed by the base
        // call - no allocation, no branch, no lock. That is well under a
        // nanosecond against a ~55 ns schedule, and nothing on this host
        // resolves half a percent: the base op's own p50 spreads 3300-4400 ns
        // per 64-op sample across runs, and the feature crossed the classifier's
        // 10% band in both directions on four consecutive runs of unchanged
        // code. Pinning states that a human read the source instead of
        // publishing a coin toss as a measurement.
        let (cat, reason) = classify_feature(
            &sw,
            Some(base_p50()),
            Some(subms::SubMsFeatureCategory::Auxiliary),
        );

        let mut p99 = BTreeMap::new();
        p99.insert(
            "schedule".to_string(),
            {
                let mut scratch: MeteredTimerWheel<u32> = MeteredTimerWheel::new(SLOTS);
                let mut w = metered(CANON);
                keyed(
                    1,
                    |i| {
                        scratch.schedule(resident_delay(i), 0);
                    },
                    |i| {
                        w.schedule(resident_delay(i), 0);
                    },
                )
            }
            .p99,
        );
        p99.insert(
            "tick".to_string(),
            drain(1, metered(CANON), |w| {
                let _ = w.tick();
            })
            .p99,
        );
        manifest.set_feature("metrics", cat, &p99, &reason);
    }

    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(&path, manifest.to_json())?;
    io::stdout().write_all(manifest.to_json().as_bytes())?;
    Ok(())
}
