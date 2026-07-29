//! Pins the behaviour each section of the `sample_app` example demonstrates:
//! the one-sided overestimate guarantee on the base sketch, top-K detection,
//! window ageing, and the max-merge combiner.

use subms_count_min_sketch::CountMinSketch;

fn market_stream() -> Vec<String> {
    let mut stream = Vec::new();
    for (sym, hits) in [("ES", 5000), ("NQ", 3000), ("CL", 1500), ("ZN", 900)] {
        for _ in 0..hits {
            stream.push(sym.to_string());
        }
    }
    for i in 0..4000 {
        stream.push(format!("T{i:04}"));
    }
    stream
}

#[test]
fn base_never_underestimates() {
    let stream = market_stream();
    let mut exact = std::collections::HashMap::new();
    let mut cms = CountMinSketch::new(4, 4096);
    for sym in &stream {
        *exact.entry(sym.clone()).or_insert(0u32) += 1;
        cms.add(sym);
    }
    for (sym, &truth) in &exact {
        assert!(
            cms.estimate(sym) >= truth,
            "one-sided error: never below true"
        );
    }
    // A cold one-shot symbol still reads at least its true count of 1.
    assert!(cms.estimate("T0007") >= 1);
    // A symbol never on the tape reads 0 here (no collision inflated it).
    assert_eq!(cms.estimate("NEVER-SEEN"), 0);
}

#[cfg(feature = "heavy-hitters")]
#[test]
fn heavy_hitters_ranks_the_hottest_symbols() {
    use subms_count_min_sketch::HeavyHitters;
    let mut hh = HeavyHitters::new(3, 4, 4096);
    for sym in market_stream() {
        hh.add(&sym);
    }
    let top = hh.top();
    assert_eq!(top.len(), 3);
    assert_eq!(top[0].key, "ES");
    assert_eq!(top[1].key, "NQ");
    assert_eq!(top[2].key, "CL");
}

#[cfg(feature = "windowed")]
#[test]
fn windowed_ages_a_burst_out() {
    use subms_count_min_sketch::WindowedCountMinSketch;
    let mut w = WindowedCountMinSketch::new(3, 4, 4096);
    for _ in 0..500 {
        w.add("ES");
    }
    assert!(w.estimate("ES") >= 500);
    w.tick();
    w.tick();
    w.tick();
    assert_eq!(
        w.estimate("ES"),
        0,
        "the burst slice rotated out of the window"
    );
}

#[cfg(feature = "merge")]
#[test]
fn merge_takes_max_not_sum() {
    use subms_count_min_sketch::merge_into;
    let mut venue_a = CountMinSketch::new(4, 4096);
    let mut venue_b = CountMinSketch::new(4, 4096);
    for _ in 0..300 {
        venue_a.add("ES");
    }
    for _ in 0..120 {
        venue_a.add("NQ");
    }
    for _ in 0..200 {
        venue_b.add("NQ");
    }
    merge_into(&mut venue_a, &venue_b).unwrap();
    assert!(venue_a.estimate("ES") >= 300);
    let nq = venue_a.estimate("NQ");
    assert!(
        (200..320).contains(&nq),
        "expected max(120,200) near 200, got {nq}"
    );
}
