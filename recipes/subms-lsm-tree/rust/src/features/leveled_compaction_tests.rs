use super::*;

fn run(id: u64, kvs: &[(&str, Option<&[u8]>)]) -> LeveledRun {
    LeveledRun::new(
        id,
        kvs.iter()
            .map(|(k, v)| (k.to_string(), v.map(|b| b.to_vec())))
            .collect(),
    )
}

#[test]
fn level_budget_grows_by_fanout() {
    let p = LeveledCompactionPlanner::new(100, 10, 4);
    assert_eq!(p.level_budget(1), 100);
    assert_eq!(p.level_budget(2), 1_000);
    assert_eq!(p.level_budget(3), 10_000);
}

#[test]
fn pick_level_fires_on_l0_run_limit() {
    let p = LeveledCompactionPlanner::new(10_000, 10, 2);
    let mut m = LeveledManifest::new();
    m.push(0, run(1, &[("a", Some(b"v"))]));
    m.push(0, run(2, &[("b", Some(b"v"))]));
    assert_eq!(p.pick_level(&m), Some(0));
}

#[test]
fn pick_level_fires_when_level_over_budget() {
    let p = LeveledCompactionPlanner::new(10, 10, 10);
    let mut m = LeveledManifest::new();
    // 100-byte values at level 1 vs 10-byte budget.
    let big = vec![b'x'; 100];
    m.push(1, LeveledRun::new(1, vec![("k".to_string(), Some(big))]));
    assert_eq!(p.pick_level(&m), Some(1));
}

#[test]
fn compact_l0_into_l1_produces_non_overlapping() {
    let p = LeveledCompactionPlanner::new(1_000_000, 10, 2);
    let mut m = LeveledManifest::new();
    // L0 overlaps with itself + L1.
    m.push(0, run(1, &[("a", Some(b"1")), ("c", Some(b"3"))]));
    m.push(0, run(2, &[("b", Some(b"2")), ("d", Some(b"4"))]));
    m.push(1, run(3, &[("a", Some(b"old")), ("e", Some(b"5"))]));
    p.compact(&mut m, 0, 100);
    assert!(
        level_is_non_overlapping(&m, 1),
        "L1 must be key-disjoint after compaction"
    );
    let merged = &m.levels[1][0];
    // L0 latest writes shadow L1 (newer wins).
    let map: BTreeMap<&str, &[u8]> = merged
        .entries
        .iter()
        .filter_map(|(k, v)| v.as_deref().map(|b| (k.as_str(), b)))
        .collect();
    assert_eq!(
        map.get("a"),
        Some(&&b"1"[..]),
        "L0 'a=1' shadows L1 'a=old'"
    );
    assert_eq!(map.get("e"), Some(&&b"5"[..]), "L1 'e=5' carried through");
    assert_eq!(m.level_run_count(0), 0);
}

#[test]
fn compact_single_l1_run_into_l2() {
    let p = LeveledCompactionPlanner::new(1_000_000, 10, 10);
    let mut m = LeveledManifest::new();
    m.push(1, run(1, &[("a", Some(b"1"))]));
    m.push(2, run(2, &[("a", Some(b"old")), ("c", Some(b"3"))]));
    p.compact(&mut m, 1, 50);
    assert_eq!(m.level_run_count(1), 0);
    assert_eq!(m.level_run_count(2), 1);
    let merged = &m.levels[2][0];
    let map: BTreeMap<&str, &[u8]> = merged
        .entries
        .iter()
        .filter_map(|(k, v)| v.as_deref().map(|b| (k.as_str(), b)))
        .collect();
    assert_eq!(map.get("a"), Some(&&b"1"[..]), "L1 wins over L2");
    assert!(map.contains_key("c"));
}

#[test]
fn compact_preserves_non_overlapping_l1_runs() {
    let p = LeveledCompactionPlanner::new(1_000_000, 10, 10);
    let mut m = LeveledManifest::new();
    m.push(0, run(1, &[("a", Some(b"1"))]));
    m.push(1, run(2, &[("a", Some(b"old"))]));
    // 'z' run is disjoint from 'a' run; must survive untouched.
    m.push(1, run(3, &[("z", Some(b"zed"))]));
    p.compact(&mut m, 0, 100);
    assert!(level_is_non_overlapping(&m, 1));
    let z_survives = m.levels[1]
        .iter()
        .any(|r| r.entries.iter().any(|(k, _)| k == "z"));
    assert!(
        z_survives,
        "disjoint L1 run must not be dragged into the merge"
    );
}

#[test]
fn tombstone_carried_through_levels() {
    let p = LeveledCompactionPlanner::new(1_000_000, 10, 2);
    let mut m = LeveledManifest::new();
    m.push(0, run(1, &[("k", Some(b"v"))]));
    m.push(0, run(2, &[("k", None)]));
    p.compact(&mut m, 0, 100);
    let merged = &m.levels[1][0];
    assert_eq!(merged.entries.len(), 1);
    assert!(
        merged.entries[0].1.is_none(),
        "tombstone shadowed the put as newer write"
    );
}

#[test]
fn pick_level_returns_none_when_no_level_full() {
    let p = LeveledCompactionPlanner::new(10_000, 10, 5);
    let mut m = LeveledManifest::new();
    m.push(0, run(1, &[("a", Some(b"v"))]));
    m.push(1, run(2, &[("b", Some(b"v"))]));
    assert!(p.pick_level(&m).is_none());
}

#[test]
fn empty_compact_is_noop() {
    let p = LeveledCompactionPlanner::new(10_000, 10, 5);
    let mut m = LeveledManifest::new();
    m.levels.push(Vec::new());
    p.compact(&mut m, 0, 100);
    assert_eq!(m.total_run_count(), 0);
}

fn _total_count(m: &LeveledManifest) -> usize {
    m.levels.iter().map(|l| l.len()).sum()
}

#[test]
fn planner_accessors_report_clamped_config() {
    // base>=1, fanout>=2, l0_run_limit>=1 after clamping.
    let p = LeveledCompactionPlanner::new(0, 1, 0);
    assert_eq!(p.base_bytes(), 1);
    assert_eq!(p.fanout(), 2);
    assert_eq!(p.l0_run_limit(), 1);
}

#[test]
fn level_bytes_sums_run_sizes() {
    let mut m = LeveledManifest::new();
    m.push(1, run(1, &[("a", Some(b"xx"))])); // 1 + 2
    m.push(1, run(2, &[("bb", Some(b"y"))])); // 2 + 1
    assert_eq!(m.level_bytes(1), 6);
    assert_eq!(m.level_bytes(9), 0, "missing level is zero bytes");
}

#[test]
fn pick_level_skips_empty_higher_levels() {
    let p = LeveledCompactionPlanner::new(10, 10, 5);
    let mut m = LeveledManifest::new();
    m.push(0, run(1, &[("a", Some(b"v"))])); // L0 below its run limit
    m.push(2, run(2, &[("z", Some(b"v"))])); // L1 stays empty, L2 tiny
    assert!(p.pick_level(&m).is_none());
}

#[test]
fn compact_from_empty_level_is_noop() {
    let p = LeveledCompactionPlanner::new(1_000_000, 10, 10);
    let mut m = LeveledManifest::new();
    m.levels.push(Vec::new()); // L0
    m.levels.push(Vec::new()); // L1 empty
    p.compact(&mut m, 1, 100);
    assert_eq!(m.total_run_count(), 0);
}

#[test]
fn empty_run_has_no_key_fences() {
    let r = LeveledRun::new(1, Vec::new());
    assert_eq!(r.min_key(), None);
    assert_eq!(r.max_key(), None);
    assert_eq!(r.size_bytes(), 0);
    // An empty run cannot overlap a real one (no key fences to compare).
    let mut m = LeveledManifest::new();
    m.push(1, r);
    m.push(1, run(2, &[("m", Some(b"v"))]));
    assert!(level_is_non_overlapping(&m, 1));
    assert!(
        level_is_non_overlapping(&m, 5),
        "missing level counts as disjoint"
    );
}
