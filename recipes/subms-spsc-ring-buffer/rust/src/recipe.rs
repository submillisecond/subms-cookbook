//! `SubMsRecipe` impl. Behind the `harness` feature.

use std::thread;

use subms::{SubMsBenchParams, SubMsPerfHarness, SubMsRecipe};

use crate::SpscRingBuffer;

/// Stages: `enqueue`, `dequeue`. Each measured on the thread that owns the
/// respective end; sample arrays merged back into the harness on the producer
/// thread after the consumer joins.
pub struct SpscRingBufferRecipe;

impl SubMsRecipe for SpscRingBufferRecipe {
    fn name(&self) -> &str {
        "spsc-ring-buffer"
    }

    fn run(&self, h: &mut SubMsPerfHarness, params: &SubMsBenchParams) {
        let entries = params.entries;
        let warmup = params.warmup;
        let (mut tx, mut rx) = SpscRingBuffer::with_capacity::<u64>(1024);

        // Warm-up: a small round-trip so branch predictors / page faults are not
        // in the timed samples.
        for i in 0..warmup as u64 {
            while tx.try_push(i).is_err() {}
            while rx.try_pop().is_none() {}
        }

        let consumer = thread::spawn(move || {
            let mut dq = Vec::with_capacity(entries);
            let mut next = 0u64;
            while next < entries as u64 {
                let t0 = std::time::Instant::now();
                if let Some(v) = rx.try_pop() {
                    let ns = t0.elapsed().as_nanos() as u64;
                    debug_assert_eq!(v, next);
                    dq.push(ns);
                    next += 1;
                }
            }
            dq
        });

        let enqueue_samples: Vec<u64> = {
            let mut samples = Vec::with_capacity(entries);
            let mut i = 0u64;
            while i < entries as u64 {
                let t0 = std::time::Instant::now();
                if tx.try_push(i).is_ok() {
                    samples.push(t0.elapsed().as_nanos() as u64);
                    i += 1;
                }
            }
            samples
        };
        let dequeue_samples = consumer.join().expect("consumer thread");

        let s_enq = h.stage("enqueue", enqueue_samples.len());
        for ns in &enqueue_samples {
            s_enq.record(*ns);
        }
        let s_deq = h.stage("dequeue", dequeue_samples.len());
        for ns in &dequeue_samples {
            s_deq.record(*ns);
        }
        h.add_meta("capacity", "1024");
    }
}
