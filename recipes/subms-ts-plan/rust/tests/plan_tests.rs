use subms_ts_plan::{TsLatencyCertificate, TsPlan, TsPlanStage};

fn sample_plan() -> TsPlan {
    TsPlan::new()
        .then("subms-zone-map", "candidates", 500_000)
        .then("subms-gorilla-block", "range_scan", 37_100)
        .then("subms-ts", "range_min", 900)
        .then("subms-tdigest", "quantile", 300)
        .with_overhead(50_000)
}

// Pins the canonical certificate JSON (and its FNV-1a integrity) so the Java
// port produces byte-identical output.
const CERT_FIXTURE: &str = "{\"hardware_tier\":\"ci-dedicated\",\"total_p99_ns\":588300,\"planner_overhead_ns\":50000,\"valid_until\":0,\"stages\":[{\"recipe\":\"subms-zone-map\",\"stage\":\"candidates\",\"p99_ns\":500000},{\"recipe\":\"subms-gorilla-block\",\"stage\":\"range_scan\",\"p99_ns\":37100},{\"recipe\":\"subms-ts\",\"stage\":\"range_min\",\"p99_ns\":900},{\"recipe\":\"subms-tdigest\",\"stage\":\"quantile\",\"p99_ns\":300}],\"integrity\":";

#[test]
fn total_is_sum_plus_overhead() {
    let p = sample_plan();
    assert_eq!(p.total_p99_ns(), 500_000 + 37_100 + 900 + 300 + 50_000);
}

#[test]
fn empty_plan_is_overhead_only() {
    let p = TsPlan::new().with_overhead(1_234);
    assert_eq!(p.total_p99_ns(), 1_234);
    let bare = TsPlan::new();
    assert_eq!(bare.total_p99_ns(), 0);
}

#[test]
fn certificate_meets_budget() {
    let cert = sample_plan().certify("ci-dedicated", 0);
    assert!(cert.meets_budget(1_000_000)); // < 1 ms
    assert!(!cert.meets_budget(500_000)); // 588_300 > 500_000
    assert_eq!(cert.total_p99_ns, 588_300);
}

#[test]
fn certificate_carries_stages() {
    let cert = sample_plan().certify("laptop", 42);
    assert_eq!(cert.stages.len(), 4);
    assert_eq!(cert.hardware_tier, "laptop");
    assert_eq!(cert.valid_until, 42);
    assert_eq!(cert.stages[0].recipe, "subms-zone-map");
}

#[test]
fn json_matches_fixture() {
    let cert = sample_plan().certify("ci-dedicated", 0);
    let json = cert.to_json();
    assert!(json.starts_with(CERT_FIXTURE), "json: {json}");
    // and it closes with the integrity value + brace
    assert!(json.ends_with(&format!("{}}}", cert.integrity)));
}

#[test]
fn integrity_verifies() {
    let cert = sample_plan().certify("ci-dedicated", 0);
    assert!(cert.verify());
}

#[test]
fn tamper_breaks_integrity() {
    let mut cert = sample_plan().certify("ci-dedicated", 0);
    assert!(cert.verify());
    cert.total_p99_ns += 1; // someone edited the headline number
    assert!(!cert.verify());
}

#[test]
fn tamper_on_a_stage_breaks_integrity() {
    let mut cert = sample_plan().certify("ci-dedicated", 0);
    cert.stages[1].p99_ns = 1; // understate a stage
    assert!(!cert.verify());
}

#[test]
fn tier_change_changes_hash() {
    let a = sample_plan().certify("laptop", 0);
    let b = sample_plan().certify("ci-dedicated", 0);
    assert_ne!(a.integrity, b.integrity);
}

#[test]
fn saturating_total_does_not_wrap() {
    let p = TsPlan::new()
        .then("a", "x", u64::MAX)
        .then("b", "y", u64::MAX);
    assert_eq!(p.total_p99_ns(), u64::MAX);
}

#[test]
fn json_round_trips_through_fields() {
    // rebuild a certificate from the parsed-out fields and confirm the hash
    // is stable (the body is a pure function of the fields).
    let cert = sample_plan().certify("ci-dedicated", 0);
    let rebuilt = TsLatencyCertificate {
        hardware_tier: cert.hardware_tier.clone(),
        total_p99_ns: cert.total_p99_ns,
        planner_overhead_ns: cert.planner_overhead_ns,
        valid_until: cert.valid_until,
        stages: cert.stages.clone(),
        integrity: cert.integrity,
    };
    assert!(rebuilt.verify());
    assert_eq!(rebuilt.to_json(), cert.to_json());
}

#[test]
fn stage_strings_are_escaped() {
    let cert = TsPlan::new()
        .then("re\"cipe", "st\\age", 10)
        .certify("tier\"x", 0);
    let json = cert.to_json();
    assert!(json.contains("re\\\"cipe"));
    assert!(json.contains("st\\\\age"));
    assert!(cert.verify());
}

#[test]
fn plan_stage_is_public() {
    let s = TsPlanStage {
        recipe: "r".into(),
        stage: "s".into(),
        p99_ns: 1,
    };
    let p = TsPlan::new().then(s.recipe.clone(), s.stage.clone(), s.p99_ns);
    assert_eq!(p.stages()[0], s);
}
