use super::*;

#[test]
fn clean_signal_is_low() {
    let samples = vec![100u64; 100];
    assert!(jitter_score(&samples) < 0.01);
}

#[test]
fn returns_zero_for_short_input() {
    assert_eq!(jitter_score(&[]), 0.0);
    assert_eq!(jitter_score(&[1, 2, 3]), 0.0);
}

#[test]
fn noisy_signal_is_higher() {
    let mut samples = Vec::new();
    for w in 0..4 {
        let base = if w % 2 == 0 { 100 } else { 1000 };
        for _ in 0..32 {
            samples.push(base);
        }
    }
    let noisy = jitter_score(&samples);
    let clean = jitter_score(&vec![100u64; 128]);
    assert!(
        noisy > clean,
        "noisy {} should exceed clean {}",
        noisy,
        clean
    );
    assert!(noisy > 0.1, "noisy jitter should clear 0.1: {}", noisy);
}

#[test]
fn score_clamped_to_unit_interval() {
    // Construct extreme alternation that produces CV > 1; should clamp.
    let mut samples = Vec::new();
    for w in 0..10 {
        let base = if w % 2 == 0 { 1 } else { 100_000 };
        for _ in 0..32 {
            samples.push(base);
        }
    }
    let s = jitter_score(&samples);
    assert!((0.0..=1.0).contains(&s), "score out of range: {}", s);
}
