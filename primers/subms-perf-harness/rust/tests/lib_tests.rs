//! Tests for the perf-harness primer's example surface: TinyMap correctness
//! and the round-trip through the subms harness (recipe -> summary -> assert).

use subms::{
    SubMsBenchParams, SubMsPerfHarness, SubMsRecipe, assert_p99_under, run_bench, summarize_lean,
    summary_to_json,
};
use subms_primer_perf_harness::{HarnessRecipe, SUB_MS_NS, TinyMap, default_assertions};

// ----- TinyMap correctness -------------------------------------------------

#[test]
fn tinymap_put_then_get_returns_inserted_value() {
    let mut m = TinyMap::with_capacity(8);
    assert!(m.is_empty());
    assert_eq!(m.put(1, 100), None);
    assert_eq!(m.put(2, 200), None);
    assert_eq!(m.len(), 2);
    assert_eq!(m.get(1), Some(100));
    assert_eq!(m.get(2), Some(200));
}

#[test]
fn tinymap_get_miss_returns_none() {
    let mut m = TinyMap::with_capacity(8);
    m.put(42, 7);
    assert_eq!(m.get(99), None);
    assert_eq!(m.get(0), None);
}

#[test]
fn tinymap_overwrite_returns_previous_value() {
    let mut m = TinyMap::with_capacity(8);
    assert_eq!(m.put(5, 50), None);
    assert_eq!(m.put(5, 51), Some(50));
    assert_eq!(m.get(5), Some(51));
    assert_eq!(m.len(), 1, "overwrite must not bump len");
}

#[test]
fn tinymap_grows_past_initial_capacity() {
    let mut m = TinyMap::with_capacity(4);
    let start_cap = m.capacity();
    for k in 0..256u32 {
        m.put(k, k.wrapping_mul(3));
    }
    assert_eq!(m.len(), 256);
    assert!(
        m.capacity() > start_cap,
        "capacity {start_cap} should have grown for 256 inserts"
    );
    for k in 0..256u32 {
        assert_eq!(m.get(k), Some(k.wrapping_mul(3)), "key {k} lost after grow");
    }
}

#[test]
fn tinymap_handles_keys_that_collide_in_low_bits() {
    // Keys spaced at multiples of 16 stride the low 4 bits identically; the
    // map's mixer should defuse the clustering but linear probing must still
    // resolve hits correctly even if it doesn't.
    let mut m = TinyMap::with_capacity(64);
    let keys: Vec<u32> = (0..32).map(|i| i * 16).collect();
    for &k in &keys {
        m.put(k, k + 1);
    }
    for &k in &keys {
        assert_eq!(m.get(k), Some(k + 1));
    }
}

// ----- HarnessRecipe wiring ------------------------------------------------

#[test]
fn recipe_name_is_stable() {
    assert_eq!(HarnessRecipe.name(), "tinymap");
}

#[test]
fn recipe_records_all_three_stages() {
    let params = SubMsBenchParams {
        entries: 256,
        warmup: 32,
        seed: 0,
    };
    let mut h = SubMsPerfHarness::new(HarnessRecipe.name(), "rust");
    HarnessRecipe.run(&mut h, &params);

    let stage_names: Vec<&str> = h.stages().iter().map(|s| s.name()).collect();
    assert_eq!(stage_names, vec!["put", "get_hit", "get_miss"]);
    for s in h.stages() {
        assert_eq!(
            s.samples().len(),
            params.entries,
            "stage {} sample count mismatch",
            s.name()
        );
    }
}

#[test]
fn recipe_sets_workload_meta() {
    let params = SubMsBenchParams {
        entries: 64,
        warmup: 0,
        seed: 0,
    };
    let mut h = SubMsPerfHarness::new(HarnessRecipe.name(), "rust");
    HarnessRecipe.run(&mut h, &params);
    assert_eq!(
        h.meta().get("workload").map(String::as_str),
        Some("tinymap")
    );
    assert_eq!(
        h.meta().get("hardware_tier").map(String::as_str),
        Some("laptop")
    );
}

#[test]
fn run_bench_drives_recipe_under_default_params() {
    let params = SubMsBenchParams {
        entries: 1_024,
        warmup: 128,
        seed: 7,
    };
    let h = run_bench(&HarnessRecipe, &params);
    assert_eq!(h.workload(), "tinymap");
    assert_eq!(h.lang(), "rust");
    let s = summarize_lean(&h);
    assert_eq!(s.stages.len(), 3);
    assert!(s.stages.iter().all(|st| st.count == params.entries));
}

#[test]
fn sub_millisecond_gate_passes_on_realistic_workload() {
    // Tiny by recipe standards - all this primer needs to prove is that the
    // harness asserts cleanly on a workload that comfortably fits the budget.
    let params = SubMsBenchParams {
        entries: 5_000,
        warmup: 1_000,
        seed: 0,
    };
    let h = run_bench(&HarnessRecipe, &params);
    let s = summarize_lean(&h);
    assert_p99_under(&s, &default_assertions()).expect("sub-ms gate must hold");

    for stage in &s.stages {
        assert!(
            stage.p99_ns < SUB_MS_NS,
            "stage {} p99 = {} ns exceeded {} ns",
            stage.name,
            stage.p99_ns,
            SUB_MS_NS
        );
    }
}

#[test]
fn summary_to_json_round_trips_through_harness() {
    let params = SubMsBenchParams {
        entries: 128,
        warmup: 16,
        seed: 0,
    };
    let s = subms::summarize(&run_bench(&HarnessRecipe, &params));
    let mut buf = Vec::new();
    summary_to_json(&s, &mut buf).unwrap();
    let json = String::from_utf8(buf).unwrap();
    assert!(json.starts_with('{'));
    assert!(json.contains("\"workload\":\"tinymap\""));
    assert!(json.contains("\"lang\":\"rust\""));
    assert!(json.contains("\"put\":"));
    assert!(json.contains("\"get_hit\":"));
    assert!(json.contains("\"get_miss\":"));
}
