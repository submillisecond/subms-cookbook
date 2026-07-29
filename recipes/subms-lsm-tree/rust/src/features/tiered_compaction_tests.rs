use super::*;

fn run(id: u64, kvs: &[(&str, Option<&[u8]>)]) -> TieredRun {
    TieredRun::new(
        id,
        kvs.iter()
            .map(|(k, v)| (k.to_string(), v.map(|b| b.to_vec())))
            .collect(),
    )
}

#[test]
fn pick_level_finds_full_level() {
    let mut m = TieredManifest::new();
    m.push(0, run(1, &[("a", Some(b"1"))]));
    m.push(0, run(2, &[("b", Some(b"2"))]));
    m.push(0, run(3, &[("c", Some(b"3"))]));
    let planner = TieredCompactionPlanner::new(3);
    assert_eq!(planner.pick_level(&m), Some(0));
}

#[test]
fn pick_level_returns_none_when_no_level_full() {
    let mut m = TieredManifest::new();
    m.push(0, run(1, &[("a", Some(b"1"))]));
    m.push(1, run(2, &[("b", Some(b"2"))]));
    let planner = TieredCompactionPlanner::new(3);
    assert!(planner.pick_level(&m).is_none());
}

#[test]
fn merge_promotes_to_next_level() {
    let mut m = TieredManifest::new();
    m.push(0, run(1, &[("a", Some(b"1"))]));
    m.push(0, run(2, &[("b", Some(b"2"))]));
    m.push(0, run(3, &[("c", Some(b"3"))]));
    let planner = TieredCompactionPlanner::new(3);
    planner.merge(&mut m, 0, 100);
    assert_eq!(m.level_run_count(0), 0, "level 0 emptied");
    assert_eq!(m.level_run_count(1), 1, "level 1 gained the merged run");
    let merged = &m.levels[1][0];
    assert_eq!(merged.id, 100);
    let keys: Vec<&str> = merged.entries.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(keys, vec!["a", "b", "c"]);
}

#[test]
fn newer_run_wins_on_key_collision() {
    let mut m = TieredManifest::new();
    m.push(0, run(1, &[("k", Some(b"old"))]));
    m.push(0, run(2, &[("k", Some(b"new"))]));
    let planner = TieredCompactionPlanner::new(2);
    planner.merge(&mut m, 0, 50);
    let merged = &m.levels[1][0];
    assert_eq!(merged.entries.len(), 1);
    assert_eq!(merged.entries[0].1.as_deref(), Some(&b"new"[..]));
}

#[test]
fn tombstone_is_preserved_in_merge() {
    let mut m = TieredManifest::new();
    m.push(0, run(1, &[("k", Some(b"v"))]));
    m.push(0, run(2, &[("k", None)]));
    let planner = TieredCompactionPlanner::new(2);
    planner.merge(&mut m, 0, 50);
    let merged = &m.levels[1][0];
    assert_eq!(merged.entries.len(), 1);
    assert!(merged.entries[0].1.is_none(), "tombstone wins");
}

#[test]
fn runs_per_level_floor_is_two() {
    let planner = TieredCompactionPlanner::new(0);
    assert_eq!(planner.runs_per_level(), 2);
    let planner = TieredCompactionPlanner::new(1);
    assert_eq!(planner.runs_per_level(), 2);
}

#[test]
fn merge_handles_non_overlapping_keys() {
    let mut m = TieredManifest::new();
    m.push(0, run(1, &[("a", Some(b"1")), ("c", Some(b"3"))]));
    m.push(0, run(2, &[("b", Some(b"2")), ("d", Some(b"4"))]));
    let planner = TieredCompactionPlanner::new(2);
    planner.merge(&mut m, 0, 99);
    let merged = &m.levels[1][0];
    let keys: Vec<&str> = merged.entries.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(keys, vec!["a", "b", "c", "d"]);
}

#[test]
fn cascading_compaction_via_repeated_pick_and_merge() {
    let mut m = TieredManifest::new();
    for i in 0..4 {
        m.push(0, run(i, &[(&format!("k{i}"), Some(b"v"))]));
    }
    let planner = TieredCompactionPlanner::new(4);
    let lvl = planner.pick_level(&m).unwrap();
    planner.merge(&mut m, lvl, 10);
    assert_eq!(
        planner.pick_level(&m),
        None,
        "single merged run does not trigger again"
    );
    assert_eq!(m.level_run_count(1), 1);
    assert_eq!(m.levels[1][0].entries.len(), 4);
}
