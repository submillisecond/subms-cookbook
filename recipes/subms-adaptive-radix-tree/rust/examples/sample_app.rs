//! Sample app: a tour of `subms-adaptive-radix-tree`, base API first, then each
//! optional feature. Run the base with `cargo run --example sample_app`; add
//! `--all-features` (or a subset like `--features range-scan`) to see the
//! feature sections light up.
//!
//! The running scenario is a venue-qualified instrument dictionary: byte-string
//! symbols (`XNAS:AAPL`, `XNYS:BRK.A`) map to internal instrument ids. It is the
//! ordered/prefix index a market-data or order-management path keeps in front of
//! the slower reference-data store.
//!
//! * base            - point lookup of an instrument id by its venue symbol
//! * range-scan      - every instrument listed on one venue, via a prefix range
//! * serialize       - persist the dictionary to bytes and rebuild it (EOD/SOD)
//! * concurrent-reads - a frozen snapshot many pricing readers fan out over
//! * metrics         - per-instance op counters + node-shape census
//! * compaction      - delist instruments, then reclaim the byte paths they held

use subms_adaptive_radix_tree::Art;

/// Venue-qualified symbol -> internal instrument id. The `XNAS:` / `XNYS:` heads
/// are what path compression collapses, and what the prefix range scan keys on.
const REFERENCE_DATA: &[(&[u8], u64)] = &[
    (b"XNAS:AAPL", 6001),
    (b"XNAS:AMZN", 6002),
    (b"XNAS:MSFT", 6003),
    (b"XNAS:NVDA", 6004),
    (b"XNYS:BRK.A", 7001),
    (b"XNYS:JPM", 7002),
    (b"XNYS:KO", 7003),
    (b"XNYS:XOM", 7004),
];

fn load_dictionary() -> Art<u64> {
    let mut dict: Art<u64> = Art::new();
    for (symbol, id) in REFERENCE_DATA {
        dict.insert(symbol, *id);
    }
    dict
}

fn main() {
    base_symbol_lookup();

    #[cfg(feature = "range-scan")]
    range_scan_by_venue();

    #[cfg(feature = "serialize")]
    serialize_round_trip();

    #[cfg(feature = "concurrent-reads")]
    concurrent_reads_snapshot();

    #[cfg(feature = "metrics")]
    metrics_census();

    #[cfg(feature = "compaction")]
    compaction_after_delisting();
}

/// Base API: resolve an instrument id from its symbol. A hit returns the id, an
/// unlisted symbol returns `None`, and two symbols sharing a venue prefix both
/// resolve through the one path-compressed node that holds `XNAS:A`.
fn base_symbol_lookup() {
    println!("== base: symbol -> instrument id ==");
    let dict = load_dictionary();

    let hit = dict.get(b"XNAS:AAPL").copied();
    let miss = dict.get(b"XNAS:TSLA").copied(); // not listed
    println!("  XNAS:AAPL -> {hit:?}");
    println!("  XNAS:TSLA -> {miss:?}");
    assert_eq!(hit, Some(6001));
    assert_eq!(miss, None);

    // Both share the compressed "XNAS:A" stem yet resolve to distinct ids.
    assert_eq!(dict.get(b"XNAS:AMZN").copied(), Some(6002));
    assert_eq!(dict.len(), REFERENCE_DATA.len());
    println!("  {} symbols indexed", dict.len());
}

/// `range-scan` feature: a byte-lex ordered scan between two bounds. The prefix
/// idiom is `[prefix, prefix-with-last-byte-incremented)` - here `[XNAS:, XNAS;)`
/// captures exactly the venue's listings, in sorted order, pruning the rest.
#[cfg(feature = "range-scan")]
fn range_scan_by_venue() {
    use subms_adaptive_radix_tree::{Bound, range};
    println!("\n== range-scan: all listings on one venue ==");
    let dict = load_dictionary();

    let venue = range(&dict, Bound::Included(b"XNAS:"), Bound::Excluded(b"XNAS;"));
    for (symbol, id) in &venue {
        println!("  {} -> {id}", String::from_utf8_lossy(symbol));
    }
    let symbols: Vec<&[u8]> = venue.iter().map(|(k, _)| k.as_slice()).collect();
    assert_eq!(
        symbols,
        vec![
            b"XNAS:AAPL".as_ref(),
            b"XNAS:AMZN",
            b"XNAS:MSFT",
            b"XNAS:NVDA"
        ]
    );
}

/// `serialize` feature: dump the whole dictionary to bytes at end of day and
/// rebuild it at start of day. The `u64` value codec ships in the box; the
/// round-trip preserves every listing.
#[cfg(feature = "serialize")]
fn serialize_round_trip() {
    use subms_adaptive_radix_tree::{parse, write_to};
    println!("\n== serialize: persist and reload the dictionary ==");
    let dict = load_dictionary();

    let mut bytes = Vec::new();
    write_to(&dict, &mut bytes).expect("write");
    println!("  {} symbols -> {} bytes", dict.len(), bytes.len());

    let restored: Art<u64> = parse(&mut &bytes[..]).expect("parse");
    assert_eq!(restored.len(), dict.len());
    for (symbol, id) in REFERENCE_DATA {
        assert_eq!(restored.get(symbol).copied(), Some(*id));
    }
    println!("  reloaded and verified {} symbols", restored.len());
}

/// `concurrent-reads` feature: freeze the dictionary into an `ArtSnapshot` that
/// pricing / risk reader threads share lock-free while the loader keeps ingesting
/// new listings. The snapshot is a point-in-time view, unaffected by later writes.
#[cfg(feature = "concurrent-reads")]
fn concurrent_reads_snapshot() {
    use std::thread;
    use subms_adaptive_radix_tree::ArtSnapshot;
    println!("\n== concurrent-reads: lock-free reader fan-out ==");
    let mut dict = load_dictionary();
    let snap = ArtSnapshot::from_tree(&dict);

    // Loader lists a new instrument after the snapshot was taken.
    dict.insert(b"XNAS:TSLA", 6005);

    let hits: Vec<usize> = thread::scope(|s| {
        let handles: Vec<_> = (0..2)
            .map(|_| {
                let view = snap.clone(); // cheap Arc bump
                s.spawn(move || {
                    REFERENCE_DATA
                        .iter()
                        .filter(|(k, _)| view.get(k).is_some())
                        .count()
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    println!(
        "  each reader resolved {} of {} symbols",
        hits[0],
        REFERENCE_DATA.len()
    );
    assert!(hits.iter().all(|&h| h == REFERENCE_DATA.len()));
    assert!(
        snap.get(b"XNAS:TSLA").is_none(),
        "post-snapshot listing is invisible to the frozen view"
    );
    println!("  post-snapshot listing invisible to readers, as intended");
}

/// `metrics` feature: `MeasuredArt` bumps per-op counters and, on demand, walks
/// the tree for its `Node4/16/48/256` census - the shape a live index takes.
#[cfg(feature = "metrics")]
fn metrics_census() {
    use subms_adaptive_radix_tree::MeasuredArt;
    println!("\n== metrics: op counters + node-shape census ==");
    let mut dict: MeasuredArt<u64> = MeasuredArt::new();
    for (symbol, id) in REFERENCE_DATA {
        dict.insert(symbol, *id);
    }
    let _ = dict.get(b"XNYS:JPM"); // hit
    let _ = dict.get(b"XNYS:GS"); // miss

    let m = dict.metrics();
    println!(
        "  inserts={} lookups={} entries={}",
        m.insertions, m.lookups, m.entries
    );
    println!(
        "  nodes: n4={} n16={} n48={} n256={}",
        m.node_types.node4, m.node_types.node16, m.node_types.node48, m.node_types.node256
    );
    assert_eq!(m.insertions, REFERENCE_DATA.len() as u64);
    assert_eq!(m.lookups, 2);
    assert_eq!(m.entries, REFERENCE_DATA.len());
}

/// `compaction` feature: `delete` clears a value but leaves its byte path in
/// place; `compact` is the periodic sweep that prunes those now-empty paths and
/// demotes over-sized nodes. Run it after a bulk delisting, not per delete.
#[cfg(feature = "compaction")]
fn compaction_after_delisting() {
    use subms_adaptive_radix_tree::{compact, delete};
    println!("\n== compaction: reclaim delisted instrument paths ==");
    let mut dict = load_dictionary();

    let delisted: &[&[u8]] = &[b"XNYS:KO", b"XNYS:XOM"];
    for symbol in delisted {
        assert_eq!(
            delete(&mut dict, symbol),
            Some(
                *REFERENCE_DATA
                    .iter()
                    .find(|(k, _)| k == symbol)
                    .map(|(_, v)| v)
                    .unwrap()
            )
        );
    }
    let changes = compact(&mut dict);
    println!(
        "  delisted {} symbols, compaction made {changes} structural changes",
        delisted.len()
    );
    assert!(changes > 0);

    for symbol in delisted {
        assert!(
            dict.get(symbol).is_none(),
            "delisted symbol no longer resolves"
        );
    }
    assert_eq!(
        dict.get(b"XNAS:AAPL").copied(),
        Some(6001),
        "surviving listings still resolve"
    );
    assert_eq!(dict.len(), REFERENCE_DATA.len() - delisted.len());
    println!("  {} symbols remain", dict.len());
}
