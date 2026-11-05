//! `SubMsRecipe` impl.

use std::hint::black_box;
use std::thread;

use subms::{SubMsBenchParams, SubMsPerfHarness, SubMsRecipe, SubMsStageKind, SubMsTimer};

use crate::seqlock;

pub const IMAGE_WORDS: usize = 5;

pub struct SeqLockRecipe;

impl SubMsRecipe for SeqLockRecipe {
    fn name(&self) -> &str {
        "seqlock"
    }

    fn run(&self, h: &mut SubMsPerfHarness, params: &SubMsBenchParams) {
        let entries = params.entries;
        let warmup = params.warmup;
        let (mut writer, reader) = seqlock([100_000_000u64, 300, 100_010_000, 500, 0]);

        for i in 0..warmup as u64 {
            writer.write(|top| {
                top[0] = 100_000_000 + (i % 100) * 10_000;
                top[4] = i;
            });
            black_box(reader.read());
        }

        let consumer = thread::spawn(move || {
            let mut samples = Vec::with_capacity(entries);
            let mut sink = 0u64;
            for _ in 0..entries {
                let t0 = SubMsTimer::tick();
                let top = reader.read();
                let ns = t0.elapsed_ns();
                sink ^= top[4];
                samples.push(ns);
            }
            black_box(sink);
            samples
        });

        let mut write_samples = Vec::with_capacity(entries);
        for i in 0..entries as u64 {
            let t0 = SubMsTimer::tick();
            writer.write(|top| {
                top[0] = 100_000_000 + (i % 100) * 10_000;
                top[1] = 100 + (i % 7) * 100;
                top[4] = i;
            });
            write_samples.push(t0.elapsed_ns());
        }
        let read_samples = consumer.join().expect("reader thread");

        let s = h
            .stage("write", write_samples.len())
            .with_kind(SubMsStageKind::HotPath);
        for ns in &write_samples {
            s.record(*ns);
        }
        let s = h
            .stage("read", read_samples.len())
            .with_kind(SubMsStageKind::HotPath);
        for ns in &read_samples {
            s.record(*ns);
        }
        h.add_meta("image_words", "5");
        h.add_meta("readers", "1");
    }
}
