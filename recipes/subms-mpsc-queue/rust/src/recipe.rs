//! `SubMsRecipe` impl. Behind the `harness` feature.

use std::sync::Arc;
use std::thread;

use subms::{SubMsBenchParams, SubMsPerfHarness, SubMsRecipe, SubMsStageKind, SubMsTimer};

use crate::{MpscQueue, PopResult};

/// Stages: `offer`, `poll`. Four producer threads share one queue; a single
/// consumer drains. Per-op timing is recorded on the thread that does the op.
pub struct MpscQueueRecipe;

impl SubMsRecipe for MpscQueueRecipe {
    fn name(&self) -> &str {
        "mpsc-queue"
    }

    fn run(&self, h: &mut SubMsPerfHarness, params: &SubMsBenchParams) {
        let entries = params.entries;
        let warmup = params.warmup;
        let producers = 4usize;
        let per_producer = entries / producers;

        let q: Arc<MpscQueue<u64>> = Arc::new(MpscQueue::new());

        // Warm-up: brief uncontended push/pop pair on one thread.
        {
            let q_ptr = Arc::as_ptr(&q) as *mut MpscQueue<u64>;
            let q_mut = unsafe { &mut *q_ptr };
            for i in 0..warmup as u64 {
                q.push(i);
                loop {
                    match q_mut.try_pop() {
                        PopResult::Some(_) => break,
                        _ => std::hint::spin_loop(),
                    }
                }
            }
        }

        // Producers each record their per-offer latencies into a private vec,
        // then return it for the main thread to feed into the harness.
        let mut producer_handles = Vec::with_capacity(producers);
        for tid in 0..producers as u64 {
            let q = q.clone();
            producer_handles.push(thread::spawn(move || {
                let mut samples = Vec::with_capacity(per_producer);
                for i in 0..per_producer as u64 {
                    let t0 = SubMsTimer::tick();
                    q.push((tid << 32) | i);
                    samples.push(t0.elapsed_ns());
                }
                samples
            }));
        }

        let consumer_q = q.clone();
        let total = producers * per_producer;
        let consumer = thread::spawn(move || {
            let q_ptr = Arc::as_ptr(&consumer_q) as *mut MpscQueue<u64>;
            let q_mut = unsafe { &mut *q_ptr };
            let mut samples = Vec::with_capacity(total);
            let mut count = 0usize;
            while count < total {
                let t0 = SubMsTimer::tick();
                match q_mut.try_pop() {
                    PopResult::Some(_) => {
                        samples.push(t0.elapsed_ns());
                        count += 1;
                    }
                    _ => std::hint::spin_loop(),
                }
            }
            samples
        });

        let s_offer = h.stage("offer", total).with_kind(SubMsStageKind::HotPath);
        for handle in producer_handles {
            for ns in handle.join().expect("producer joined") {
                s_offer.record(ns);
            }
        }
        let poll_samples = consumer.join().expect("consumer joined");
        let s_poll = h
            .stage("poll", poll_samples.len())
            .with_kind(SubMsStageKind::HotPath);
        for ns in poll_samples {
            s_poll.record(ns);
        }
        h.add_meta("producers", &producers.to_string());
    }
}
