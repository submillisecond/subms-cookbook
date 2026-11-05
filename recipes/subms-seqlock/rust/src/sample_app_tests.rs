use std::thread;

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TopOfBook {
    bid: u64,
    bid_qty: u64,
    ask: u64,
    ask_qty: u64,
    seq: u64,
}

impl SeqLockImage for TopOfBook {
    const WORDS: usize = 5;
    fn store(&self, mut put: impl FnMut(usize, u64)) {
        put(0, self.bid);
        put(1, self.bid_qty);
        put(2, self.ask);
        put(3, self.ask_qty);
        put(4, self.seq);
    }
    fn load(mut get: impl FnMut(usize) -> u64) -> Self {
        TopOfBook {
            bid: get(0),
            bid_qty: get(1),
            ask: get(2),
            ask_qty: get(3),
            seq: get(4),
        }
    }
}

impl TopOfBook {
    fn at(seq: u64) -> TopOfBook {
        let mid = 100_000 + (seq % 7) * 10;
        TopOfBook {
            bid: mid - 5,
            bid_qty: 100 + seq % 900,
            ask: mid + 5,
            ask_qty: 100 + (seq * 3) % 900,
            seq,
        }
    }
}

#[test]
fn base_two_readers_take_only_consistent_snapshots() {
    let updates = 50_000u64;
    let (mut writer, reader) = seqlock(TopOfBook::at(0));
    let readers: Vec<_> = (0..2)
        .map(|_| {
            let r = reader.clone();
            thread::spawn(move || {
                let mut last = 0u64;
                while last < updates {
                    let (top, version) = r.read_versioned();
                    assert_eq!(top, TopOfBook::at(top.seq));
                    assert_eq!(version, top.seq);
                    last = top.seq;
                }
            })
        })
        .collect();
    for seq in 1..=updates {
        writer.set(TopOfBook::at(seq));
    }
    for h in readers {
        h.join().unwrap();
    }
    assert_eq!(writer.version(), updates);
    assert_eq!(reader.read(), TopOfBook::at(updates));
    assert!(reader.try_read(1).is_some());
}

#[cfg(feature = "array")]
#[test]
fn array_section_isolates_symbols() {
    let (mut writer, reader) = seqlock_array(&[TopOfBook::at(0); 4]);
    for seq in 1..=1_000u64 {
        writer.set(0, TopOfBook::at(seq));
    }
    writer.write(2, |top| {
        top.bid_qty = 42;
        top.seq = 1;
    });
    assert_eq!(reader.version(0), 1_000);
    assert_eq!(reader.version(1), 0);
    assert_eq!(reader.version(2), 1);
    assert_eq!(reader.version(3), 0);
    assert_eq!(reader.read(2).bid_qty, 42);
    let mut all = Vec::new();
    reader.read_all(&mut all);
    assert_eq!(all.len(), 4);
    assert_eq!(all[1], TopOfBook::at(0));
}

#[cfg(feature = "latch")]
#[test]
fn latch_section_reads_are_consistent_and_both_copies_current() {
    let updates = 50_000u64;
    let (mut writer, reader) = seqlatch(TopOfBook::at(0));
    let r = reader.clone();
    let h = thread::spawn(move || {
        let mut last = 0u64;
        while last < updates {
            let (top, version) = r.read_versioned();
            assert_eq!(top, TopOfBook::at(top.seq));
            assert!(version == top.seq || version + 1 == top.seq);
            last = top.seq;
        }
    });
    for seq in 1..=updates {
        writer.set(TopOfBook::at(seq));
    }
    h.join().unwrap();
    assert_eq!(reader.read_versioned(), (TopOfBook::at(updates), updates));
    assert_eq!(reader.read(), writer.get());
}

#[cfg(feature = "stats")]
#[test]
fn stats_section_counts_idle_reads_and_resets() {
    let (mut writer, reader) = seqlock(TopOfBook::at(0));
    let counted = InstrumentedReader::new(reader);
    for seq in 1..=1_000u64 {
        writer.set(TopOfBook::at(seq));
        counted.read();
    }
    let s = counted.snapshot();
    assert_eq!((s.reads, s.retries, s.max_retries), (1_000, 0, 0));
    counted.stats().reset();
    assert_eq!(counted.snapshot().reads, 0);
}
