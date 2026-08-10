//! Sample app: distinct-count telemetry for a two-venue trading gateway.
//!
//! One deterministic tape of order events drives the whole thing. The gateway
//! counts distinct sessions per window, risk keeps a per-symbol counterparty
//! sketch, and each venue ships its account sketch to a collector that merges
//! them into a firm-wide reach number without ever seeing an account id.
//!
//! Run the base with `cargo run --example sample_app`; add `--all-features`
//! (or a subset like `--features sparse`) to light up the later stages.

use subms_hyperloglog::HyperLogLog;

// The per-symbol section is a `sparse` tour, so its inputs are inert on a
// default-feature build.
#[cfg_attr(not(feature = "sparse"), allow(dead_code))]
const SYMBOLS: [&str; 8] = [
    "AAPL", "MSFT", "NVDA", "TSLA", "AMZN", "META", "GOOG", "NFLX",
];
const EVENTS: usize = 200_000;

/// Distinct counterparties trading each symbol. Two liquid names and a long
/// tail, which is what makes a per-symbol dense array wasteful.
const COUNTERPARTY_POOL: [u64; 8] = [30_000, 30_000, 900, 700, 60, 40, 15, 9];

#[cfg_attr(not(feature = "sparse"), allow(dead_code))]
struct Event {
    venue: u8,
    symbol: usize,
    account: u64,
    counterparty: u64,
    session: u64,
}

/// Seeded so the printed report is the same on every run and on both ports.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 11
    }
}

/// The tape. Venue 0 and venue 1 draw accounts from overlapping ranges, and
/// symbol popularity follows a sharp head-and-tail split - the two facts every
/// stage below is trying to measure.
fn tape() -> Vec<Event> {
    let mut rng = Lcg(0x5eed);
    (0..EVENTS)
        .map(|_| {
            let venue = (rng.next() % 2) as u8;
            // Two thirds of the flow lands on the first two symbols.
            let r = rng.next() % 100;
            let symbol = if r < 66 {
                (r % 2) as usize
            } else {
                2 + (rng.next() % 6) as usize
            };
            // Venue 0 accounts 0..30k, venue 1 accounts 20k..50k: a 10k overlap.
            let account = if venue == 0 {
                rng.next() % 30_000
            } else {
                20_000 + rng.next() % 30_000
            };
            Event {
                venue,
                symbol,
                account,
                counterparty: (symbol as u64) << 32 | (rng.next() % COUNTERPARTY_POOL[symbol]),
                session: rng.next() % 40_000,
            }
        })
        .collect()
}

fn main() {
    let tape = tape();
    println!(
        "tape: {} order events across 2 venues, 8 symbols\n",
        tape.len()
    );

    gateway_sessions(&tape);
    size_from_an_error_budget();

    #[cfg(feature = "sparse")]
    per_symbol_counterparties(&tape);

    #[cfg(feature = "union-intersect")]
    cross_venue_overlap(&tape);

    collector_fan_in(&tape);
}

/// The hot path. Every message on the gateway records its session id, and the
/// answer costs 16 KB whether the window held 40 thousand sessions or 40
/// million. `add_u64` skips rendering the id to a string first.
fn gateway_sessions(tape: &[Event]) {
    println!("== gateway: distinct sessions this window ==");
    let mut hll = HyperLogLog::new(14);
    let mut first_sightings = 0u64;
    for e in tape {
        if hll.add_u64(e.session) {
            first_sightings += 1;
        }
    }
    let est = hll.estimate();
    println!("  {} messages -> {:.0} distinct sessions", tape.len(), est);
    println!(
        "  {} registers advanced, {} bytes of state, +/- {:.2}% standard error",
        first_sightings,
        hll.state_bytes(),
        hll.standard_error() * 100.0
    );
    assert!(est > 30_000.0 && est < 50_000.0, "~40k sessions, got {est}");
}

/// Sizing runs the other way round in production: you are handed an error
/// budget, not a precision. `precision_for_standard_error` turns the budget
/// into the cheapest register array that meets it.
fn size_from_an_error_budget() {
    println!("\n== sizing: error budget -> byte budget ==");
    for budget in [0.05, 0.02, 0.01, 0.005] {
        let p = HyperLogLog::precision_for_standard_error(budget);
        let hll = HyperLogLog::new(p);
        println!(
            "  budget {:>5.1}%  ->  p={:<2} {:>6} bytes, actual {:.5}%",
            budget * 100.0,
            p,
            hll.state_bytes(),
            hll.standard_error() * 100.0
        );
    }
}

/// `sparse`: risk wants distinct counterparties per symbol. Most of the book is
/// thin, so allocating 16 KB per name would cost 128 KB here and gigabytes
/// across a real universe. The sparse encoding pays only for registers actually
/// touched, and promotes the two busy names once they earn it.
#[cfg(feature = "sparse")]
fn per_symbol_counterparties(tape: &[Event]) {
    use subms_hyperloglog::SparseHyperLogLog;
    println!("\n== risk: distinct counterparties per symbol ==");

    let mut books: Vec<SparseHyperLogLog> = (0..SYMBOLS.len())
        .map(|_| SparseHyperLogLog::with_threshold(14, 2_000))
        .collect();
    for e in tape {
        books[e.symbol].add_u64(e.counterparty);
    }

    let mut sparse_bytes = 0usize;
    for (i, b) in books.iter().enumerate() {
        println!(
            "  {:<5} {:>7.0} counterparties  {:>6} bytes  {}",
            SYMBOLS[i],
            b.estimate(),
            b.state_bytes(),
            if b.is_sparse() { "sparse" } else { "dense" }
        );
        sparse_bytes += b.state_bytes();
    }
    let dense_bytes = SYMBOLS.len() * 16_384;
    println!("  total {sparse_bytes} bytes against {dense_bytes} if every name held a dense array");
    assert!(
        sparse_bytes < dense_bytes,
        "sparse must win on the long tail"
    );
}

/// `union-intersect`: how many accounts trade on both venues? Inclusion-
/// exclusion answers it from two sketches. The error bound is printed next to
/// the answer because it scales with |A| + |B| rather than with the overlap,
/// and an overlap smaller than its own bound is not a number to act on.
#[cfg(feature = "union-intersect")]
fn cross_venue_overlap(tape: &[Event]) {
    use subms_hyperloglog::{estimate_intersect, estimate_union, intersect_error_bound};
    println!("\n== venues: account reach and overlap ==");

    let mut a = HyperLogLog::new(14);
    let mut b = HyperLogLog::new(14);
    for e in tape {
        if e.venue == 0 {
            a.add_u64(e.account);
        } else {
            b.add_u64(e.account);
        }
    }
    let union = estimate_union(&a, &b).expect("same precision");
    let inter = estimate_intersect(&a, &b).expect("same precision");
    let bound = intersect_error_bound(&a, &b).expect("same precision");
    println!("  venue 0: {:>7.0} accounts", a.estimate());
    println!("  venue 1: {:>7.0} accounts", b.estimate());
    println!("  reach:   {union:>7.0} (true 50000)");
    println!("  both:    {inter:>7.0} (true 10000) +/- {bound:.0}");
    assert!(
        (union - 50_000.0).abs() / 50_000.0 < 0.05,
        "reach within 5%, got {union}"
    );
    assert!(inter > 0.0, "a 10k overlap must survive the subtraction");
}

/// `serialize`: each venue ships its sketch, not its account list. The
/// collector decodes and merges, and the firm-wide number falls out of 16 KB
/// per venue instead of a million ids on the wire.
fn collector_fan_in(tape: &[Event]) {
    println!("\n== collector: merge shipped sketches into a firm-wide reach ==");

    let mut per_venue = [HyperLogLog::new(14), HyperLogLog::new(14)];
    for e in tape {
        per_venue[e.venue as usize].add_u64(e.account);
    }
    let shipped: Vec<Vec<u8>> = per_venue.iter().map(|h| h.to_bytes()).collect();
    let on_wire: usize = shipped.iter().map(|b| b.len()).sum();

    let mut firm = HyperLogLog::new(14);
    for bytes in &shipped {
        let decoded = HyperLogLog::from_bytes(bytes).expect("collector reads its own format");
        firm.merge(&decoded).expect("same precision");
    }
    println!(
        "  {} sketches on the wire, {} bytes total",
        shipped.len(),
        on_wire
    );
    println!(
        "  raw ids would have been ~{} bytes",
        tape.len() * core::mem::size_of::<u64>()
    );
    println!("  firm-wide reach: {:.0} (true 50000)", firm.estimate());
    assert!(
        (firm.estimate() - 50_000.0).abs() / 50_000.0 < 0.05,
        "merged reach within 5%"
    );
}
