//! `SubMsGrowthRecipe` impl.

use subms::{SubMsGrowthClass, SubMsGrowthRecipe};

use crate::HdrHistogram;

/// Exclusive top of the recorded value range. It fixes the counter array's
/// maximum size, so it also fixes the bound the verdict is checked against.
const VALUE_CEILING: u64 = 1_000_000_000;

/// Records an unbounded stream of values and reports the counter array's real
/// size each round. The array is sized by the largest value seen, never by how
/// many values were recorded, so the curve is flat in the sample count.
pub struct HdrGrowthRecipe {
    hist: HdrHistogram,
    rounds: usize,
    records_per_round: usize,
    bound_bytes: u64,
    lcg: u64,
}

impl HdrGrowthRecipe {
    pub fn new(significant_digits: u32, rounds: usize, records_per_round: usize) -> Self {
        Self {
            hist: HdrHistogram::new(significant_digits),
            rounds,
            records_per_round,
            bound_bytes: ceiling_footprint_bytes(significant_digits),
            lcg: 0x1234_5678,
        }
    }
}

/// The array size once the top of the value range has been recorded, measured on
/// a throwaway histogram rather than modelled. A second footprint model in the
/// harness is what published a figure 2.4x under the real allocation.
fn ceiling_footprint_bytes(significant_digits: u32) -> u64 {
    let mut probe = HdrHistogram::new(significant_digits);
    probe.record(VALUE_CEILING - 1);
    probe.footprint_bytes() as u64
}

impl SubMsGrowthRecipe for HdrGrowthRecipe {
    fn name(&self) -> &str {
        "subms-hdr-histogram"
    }
    fn op_name(&self) -> &str {
        "record"
    }
    fn rounds(&self) -> usize {
        self.rounds
    }
    fn ops_per_round(&self) -> usize {
        self.records_per_round
    }
    fn op(&mut self, _round: usize, _i: usize) {
        // A spread of values across the whole range, so every major bucket is
        // touched - still O(1) memory once the array covers the range.
        self.lcg = self.lcg.wrapping_mul(6364136223846793005).wrapping_add(1);
        let v = (self.lcg >> 33) % VALUE_CEILING;
        self.hist.record(v.max(1));
    }
    fn memory_bytes(&mut self) -> u64 {
        self.hist.footprint_bytes() as u64
    }
    fn live_bytes(&mut self) -> u64 {
        self.hist.footprint_bytes() as u64
    }
    fn structures(&mut self) -> Vec<(String, u64)> {
        vec![("records".to_string(), self.hist.count())]
    }
    fn expected(&self) -> (SubMsGrowthClass, f64) {
        (SubMsGrowthClass::Bounded, self.bound_bytes as f64 * 1.01)
    }
    fn compact(&self) -> bool {
        true
    }
}

#[cfg(test)]
#[path = "growth_tests.rs"]
mod tests;
