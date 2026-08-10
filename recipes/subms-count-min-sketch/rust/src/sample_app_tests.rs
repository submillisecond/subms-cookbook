//! Pins the behaviour each section of the `sample_app` example demonstrates:
//! the one-sided guarantee the rate governor rests on, the bandwidth-weighted
//! throttle list, the ageing window, and the cross-venue rollup.

use super::*;

struct Msg {
    symbol: String,
    bytes: u32,
}

fn tape() -> Vec<Msg> {
    let mut tape = Vec::new();
    for (symbol, msgs, bytes) in [
        ("ESZ5", 5000, 96),
        ("NQZ5", 3000, 96),
        ("CLF6", 1500, 128),
        ("ZNH6", 900, 64),
    ] {
        for _ in 0..msgs {
            tape.push(Msg {
                symbol: symbol.to_string(),
                bytes,
            });
        }
    }
    for i in 0..4000 {
        tape.push(Msg {
            symbol: format!("THIN{i:04}"),
            bytes: 64,
        });
    }
    tape
}

#[test]
fn governor_never_under_counts_and_brackets_the_truth() {
    let tape = tape();
    let mut exact = std::collections::HashMap::new();
    let mut rates = CountMinSketch::with_error_bounds(0.001, 0.999);
    for msg in &tape {
        *exact.entry(msg.symbol.clone()).or_insert(0u32) += 1;
        rates.add(&msg.symbol);
    }
    let margin = rates.error_margin();
    for (symbol, &truth) in &exact {
        let est = rates.estimate(symbol);
        assert!(est >= truth, "a throttle decision may never miss a talker");
        assert!(est.saturating_sub(margin) <= truth, "lower bound holds");
    }
    assert_eq!(rates.total(), tape.len() as u64);
    // A symbol never on the tape reads 0 here - no collision inflated it.
    assert_eq!(rates.estimate("NEVER-SEEN"), 0);
}

#[test]
fn governor_state_survives_a_checkpoint() {
    let tape = tape();
    let mut rates = CountMinSketch::with_error_bounds(0.001, 0.999);
    for msg in &tape {
        rates.add(&msg.symbol);
    }
    let restored = CountMinSketch::from_bytes(&rates.to_bytes()).unwrap();
    assert_eq!(restored.total(), rates.total());
    assert_eq!(restored.estimate("ESZ5"), rates.estimate("ESZ5"));
    assert_eq!(restored.estimate("THIN0007"), rates.estimate("THIN0007"));
}

#[cfg(feature = "heavy-hitters")]
#[test]
fn bandwidth_ranking_differs_from_message_ranking() {
    use crate::HeavyHitters;
    let tape = tape();
    let mut by_bytes = HeavyHitters::new(3, 5, 8192);
    let mut by_msgs = HeavyHitters::new(3, 5, 8192);
    for msg in &tape {
        by_bytes.add_n(&msg.symbol, msg.bytes);
        by_msgs.add(&msg.symbol);
    }
    assert_eq!(by_bytes.top().len(), 3);
    assert_eq!(by_bytes.top()[0].key, "ESZ5");
    assert_eq!(by_msgs.top()[0].key, "ESZ5");
    // 1500 messages at 128 bytes beats 900 at 64 by far more than the message
    // ranking suggests, which is the reason the throttle list weights by size.
    assert!(by_bytes.estimate("CLF6") > 3 * by_bytes.estimate("ZNH6"));
    assert!(by_msgs.estimate("CLF6") < 2 * by_msgs.estimate("ZNH6"));
}

#[cfg(feature = "windowed")]
#[test]
fn burst_ages_out_of_the_window() {
    use crate::WindowedCountMinSketch;
    let mut recent = WindowedCountMinSketch::new(3, 5, 8192);
    for _ in 0..500 {
        recent.add("ESZ5");
    }
    assert!(recent.estimate("ESZ5") >= 500);
    for _ in 0..3 {
        recent.tick();
        for _ in 0..40 {
            recent.add("ESZ5");
        }
    }
    let settled = recent.estimate("ESZ5");
    assert!(settled >= 120, "the quiet traffic is still counted");
    assert!(settled <= 400, "the burst left the window: {settled}");
}

#[cfg(feature = "merge")]
#[test]
fn cross_venue_rollup_keeps_the_union_bound() {
    use crate::merge_into;
    let mut cme = CountMinSketch::new(5, 8192);
    let mut ice = CountMinSketch::new(5, 8192);
    for _ in 0..300 {
        cme.add("ESZ5");
    }
    for _ in 0..120 {
        cme.add("CLF6");
    }
    for _ in 0..200 {
        ice.add("CLF6");
    }
    merge_into(&mut cme, &ice).unwrap();
    assert!(cme.estimate("ESZ5") >= 300);
    let clf = cme.estimate("CLF6");
    assert!(clf >= 320, "union of both legs, got {clf}");
    assert!(clf < 360, "and not much above it, got {clf}");
    assert_eq!(cme.total(), 620);
}
