//! Scalar-vs-simd comparison for the numeric scans. Build the same 50k-point
//! f64 series both ways (`cargo run --release --example simd_compare` for the
//! scalar build, add `--features simd` for the vectorised build) and read the
//! per-op median off each run. Std-only, no harness.

use std::time::Instant;
use subms_ts::TsSeries;

const N: usize = 50_000;
const ITERS: usize = 2_000;

fn median_ns(mut samples: Vec<u128>) -> u128 {
    samples.sort_unstable();
    samples[samples.len() / 2]
}

fn main() {
    let mut s = TsSeries::<f64>::with_capacity(N);
    // Deterministic pseudo-random values - a tiny LCG so the two builds see
    // byte-identical input without a dep.
    let mut state: u64 = 0x9e3779b97f4a7c15;
    for i in 0..N {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let v = (state >> 11) as f64 / (1u64 << 53) as f64;
        s.push(i as i64, v).unwrap();
    }

    let lo = 0i64;
    let hi = N as i64;

    let mut sum_t = Vec::with_capacity(ITERS);
    let mut min_t = Vec::with_capacity(ITERS);
    let mut max_t = Vec::with_capacity(ITERS);
    let mut acc = 0.0f64; // black-box accumulator so nothing is optimised out

    for _ in 0..ITERS {
        let t0 = Instant::now();
        let r = s.range_sum(lo, hi);
        sum_t.push(t0.elapsed().as_nanos());
        acc += r;

        let t0 = Instant::now();
        let r = s.range_min(lo, hi).unwrap();
        min_t.push(t0.elapsed().as_nanos());
        acc += r;

        let t0 = Instant::now();
        let r = s.range_max(lo, hi).unwrap();
        max_t.push(t0.elapsed().as_nanos());
        acc += r;
    }

    let mode = if cfg!(feature = "simd") {
        "simd"
    } else {
        "scalar"
    };
    println!("mode={mode} N={N} iters={ITERS}");
    println!("  range_sum  median {:>6} ns", median_ns(sum_t));
    println!("  range_min  median {:>6} ns", median_ns(min_t));
    println!("  range_max  median {:>6} ns", median_ns(max_t));
    // Defeat dead-code elimination of the whole loop.
    if acc.is_nan() {
        println!("unreachable {acc}");
    }
}
