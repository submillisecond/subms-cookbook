//! Pins the behaviour each section of the `sample_app` example demonstrates:
//! the instrument-dictionary scenario resolves symbols correctly, and every
//! opt-in feature holds the invariant the sample leans on.

use super::*;

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

#[test]
fn base_symbol_lookup_scenario() {
    let dict = load_dictionary();
    assert_eq!(
        dict.get(b"XNAS:AAPL").copied(),
        Some(6001),
        "listed symbol resolves"
    );
    assert_eq!(dict.get(b"XNAS:TSLA"), None, "unlisted symbol misses");
    // Shared "XNAS:A" stem resolves to distinct ids under path compression.
    assert_eq!(dict.get(b"XNAS:AMZN").copied(), Some(6002));
    assert_eq!(dict.len(), REFERENCE_DATA.len());
}

#[cfg(feature = "range-scan")]
#[test]
fn range_scan_prefix_selects_one_venue() {
    use crate::{Bound, range};
    let dict = load_dictionary();
    let venue = range(&dict, Bound::Included(b"XNAS:"), Bound::Excluded(b"XNAS;"));
    let symbols: Vec<Vec<u8>> = venue.into_iter().map(|(k, _)| k).collect();
    assert_eq!(
        symbols,
        vec![
            b"XNAS:AAPL".to_vec(),
            b"XNAS:AMZN".to_vec(),
            b"XNAS:MSFT".to_vec(),
            b"XNAS:NVDA".to_vec(),
        ],
        "prefix range returns exactly the venue's listings, byte-lex sorted"
    );
}

#[cfg(feature = "serialize")]
#[test]
fn serialize_round_trips_every_listing() {
    use crate::{parse, write_to};
    let dict = load_dictionary();
    let mut bytes = Vec::new();
    write_to(&dict, &mut bytes).unwrap();
    let restored: Art<u64> = parse(&mut &bytes[..]).unwrap();
    assert_eq!(restored.len(), dict.len());
    for (symbol, id) in REFERENCE_DATA {
        assert_eq!(
            restored.get(symbol).copied(),
            Some(*id),
            "listing survives the round trip"
        );
    }
}

#[cfg(feature = "concurrent-reads")]
#[test]
fn snapshot_is_frozen_against_later_writes() {
    use crate::ArtSnapshot;
    let mut dict = load_dictionary();
    let snap = ArtSnapshot::from_tree(&dict);
    dict.insert(b"XNAS:TSLA", 6005);
    assert_eq!(
        snap.get(b"XNAS:AAPL").copied(),
        Some(6001),
        "pre-snapshot listing visible"
    );
    assert!(
        snap.get(b"XNAS:TSLA").is_none(),
        "post-snapshot listing invisible"
    );
    assert_eq!(snap.len(), REFERENCE_DATA.len());
}

#[cfg(feature = "metrics")]
#[test]
fn metrics_track_the_op_mix() {
    use crate::MeasuredArt;
    let mut dict: MeasuredArt<u64> = MeasuredArt::new();
    for (symbol, id) in REFERENCE_DATA {
        dict.insert(symbol, *id);
    }
    let _ = dict.get(b"XNYS:JPM");
    let _ = dict.get(b"XNYS:GS");
    let m = dict.metrics();
    assert_eq!(m.insertions, REFERENCE_DATA.len() as u64);
    assert_eq!(m.lookups, 2);
    assert_eq!(m.entries, REFERENCE_DATA.len());
}

#[cfg(feature = "compaction")]
#[test]
fn compaction_reclaims_delisted_paths() {
    use crate::{compact, delete};
    let mut dict = load_dictionary();
    assert_eq!(delete(&mut dict, b"XNYS:KO"), Some(7003));
    assert_eq!(delete(&mut dict, b"XNYS:XOM"), Some(7004));
    let changes = compact(&mut dict);
    assert!(
        changes > 0,
        "compaction reports structural changes after a delisting"
    );
    assert!(dict.get(b"XNYS:KO").is_none());
    assert!(dict.get(b"XNYS:XOM").is_none());
    assert_eq!(
        dict.get(b"XNAS:AAPL").copied(),
        Some(6001),
        "survivors still resolve"
    );
    assert_eq!(dict.len(), REFERENCE_DATA.len() - 2);
}
