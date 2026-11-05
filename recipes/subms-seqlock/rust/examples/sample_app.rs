//! Sample app: a tour of `subms-seqlock`, base API first, then each optional
//! feature. Run the base with `cargo run --example sample_app`; add
//! `--all-features` to see the feature sections.
//!
//! Domain: a feed handler publishes top of book, strategy threads read it.
//!
//! * base  - one symbol, one writer, two readers, every snapshot consistent
//! * array - a per-symbol table; a tick in one symbol never disturbs another
//! * latch - the double-buffered variant, readers never wait on a writer
//! * stats - retry counters on a reader

use std::thread;

use subms_seqlock::{SeqLockImage, seqlock};

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

    fn consistent(&self) -> bool {
        *self == TopOfBook::at(self.seq)
    }
}

fn main() {
    base_one_symbol_two_readers();

    #[cfg(feature = "array")]
    array_per_symbol_table();

    #[cfg(feature = "latch")]
    latch_never_waits();

    #[cfg(feature = "stats")]
    stats_on_a_reader();
}

fn base_one_symbol_two_readers() {
    println!("== base: one symbol, one writer, two readers ==");
    let updates = 200_000u64;
    let (mut writer, reader) = seqlock(TopOfBook::at(0));

    let readers: Vec<_> = (0..2)
        .map(|_| {
            let r = reader.clone();
            thread::spawn(move || {
                let mut torn = 0u64;
                let mut last = 0u64;
                while last < updates {
                    let (top, version) = r.read_versioned();
                    if !top.consistent() || version != top.seq {
                        torn += 1;
                    }
                    last = top.seq;
                }
                torn
            })
        })
        .collect();

    for seq in 1..=updates {
        writer.set(TopOfBook::at(seq));
    }
    let torn: u64 = readers.into_iter().map(|h| h.join().unwrap()).sum();

    let last = reader.read();
    println!("  writer published {} updates", writer.version());
    println!("  torn snapshots across both readers: {torn}");
    println!(
        "  final top: {} x {} / {} x {} (seq {})",
        last.bid_qty, last.bid, last.ask, last.ask_qty, last.seq
    );
    println!(
        "  a bounded read on an idle lock succeeds first try: {}",
        reader.try_read(1).is_some()
    );
}

#[cfg(feature = "array")]
fn array_per_symbol_table() {
    use subms_seqlock::seqlock_array;

    println!("== array: a per-symbol table ==");
    let symbols = ["ESZ5", "NQZ5", "CLZ5", "GCZ5"];
    let (mut writer, reader) = seqlock_array(&[TopOfBook::at(0); 4]);

    for seq in 1..=1_000u64 {
        writer.set(0, TopOfBook::at(seq));
    }
    writer.write(2, |top| {
        top.bid_qty = 42;
        top.seq = 1;
    });

    for (i, sym) in symbols.iter().enumerate() {
        let (top, version) = reader.read_versioned(i);
        println!(
            "  {sym}: version {version}, bid_qty {}, seq {}",
            top.bid_qty, top.seq
        );
    }
    let mut all = Vec::new();
    reader.read_all(&mut all);
    println!(
        "  read_all returned {} slots; slot 1 and 3 were never written",
        all.len()
    );
}

#[cfg(feature = "latch")]
fn latch_never_waits() {
    use subms_seqlock::seqlatch;

    println!("== latch: readers never wait on the writer ==");
    let updates = 200_000u64;
    let (mut writer, reader) = seqlatch(TopOfBook::at(0));

    let r = reader.clone();
    let strategy = thread::spawn(move || {
        let mut torn = 0u64;
        let mut stale_by_one = 0u64;
        let mut last = 0u64;
        while last < updates {
            let (top, version) = r.read_versioned();
            if !top.consistent() {
                torn += 1;
            }
            if version + 1 == top.seq {
                stale_by_one += 1;
            }
            last = top.seq;
        }
        (torn, stale_by_one)
    });

    for seq in 1..=updates {
        writer.set(TopOfBook::at(seq));
    }
    let (torn, _stale) = strategy.join().unwrap();

    println!("  writer published {} updates", writer.version());
    println!("  torn snapshots: {torn}");
    let (top, version) = reader.read_versioned();
    println!("  idle read: seq {} at version {version}", top.seq);
    println!(
        "  two copies, both current after every write: {}",
        reader.read() == writer.get()
    );
}

#[cfg(feature = "stats")]
fn stats_on_a_reader() {
    use subms_seqlock::InstrumentedReader;

    println!("== stats: retry counters on a reader ==");
    let (mut writer, reader) = seqlock(TopOfBook::at(0));
    let counted = InstrumentedReader::new(reader);

    for seq in 1..=1_000u64 {
        writer.set(TopOfBook::at(seq));
        counted.read();
    }
    let s = counted.snapshot();
    println!(
        "  idle lock: reads {}, retries {}, worst {}, rate {:.3}",
        s.reads,
        s.retries,
        s.max_retries,
        s.retry_rate()
    );
    counted.stats().reset();
    println!("  after reset: reads {}", counted.snapshot().reads);
}
