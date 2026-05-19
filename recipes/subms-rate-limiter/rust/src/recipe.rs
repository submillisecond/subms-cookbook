//! `SubMsRecipe` impl. Behind the `harness` feature.

use std::sync::Arc;
use std::thread;
use std::time::Instant;

use subms::{SubMsBenchParams, SubMsPerfHarness, SubMsRecipe};

use crate::RateLimiter;

/// Stage: `try_acquire` (under 8-way contention).
pub struct RateLimiterRecipe;

impl SubMsRecipe for RateLimiterRecipe {
    fn name(&self) -> &str {
        "rate-limiter"
    }

    fn run(&self, h: &mut SubMsPerfHarness, params: &SubMsBenchParams) {
        let entries = params.entries;
        let warmup = params.warmup;
        // 1M permits/sec; nearly all attempts succeed; the bench measures call latency
        // not throttling behaviour. Throttling correctness is in the unit tests.
        let rl = Arc::new(RateLimiter::new(1_000_000.0, 1_000_000));
        for _ in 0..warmup {
            let _ = rl.try_acquire();
        }

        let threads = 8usize;
        let per_thread = entries / threads;
        let mut handles = Vec::with_capacity(threads);
        for _ in 0..threads {
            let rl = rl.clone();
            handles.push(thread::spawn(move || {
                let mut samples = Vec::with_capacity(per_thread);
                for _ in 0..per_thread {
                    let t0 = Instant::now();
                    let _ = rl.try_acquire();
                    samples.push(t0.elapsed().as_nanos() as u64);
                }
                samples
            }));
        }

        let total = threads * per_thread;
        let s = h.stage("try_acquire", total);
        for handle in handles {
            for ns in handle.join().expect("joined") {
                s.record(ns);
            }
        }
        h.add_meta("threads", &threads.to_string());
    }
}
