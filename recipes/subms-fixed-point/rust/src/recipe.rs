//! `SubMsRecipe` impl.

use std::hint::black_box;

use subms::{SubMsBenchParams, SubMsPerfHarness, SubMsRecipe, SubMsStageKind, SubMsTimer};

use crate::{Fixed, Rounding};

/// Deterministic operand generator.
pub(crate) struct Lcg(u64);

impl Lcg {
    pub(crate) fn new(seed: u64) -> Lcg {
        Lcg(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(1))
    }

    pub(crate) fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 11
    }

    /// A price within +/- 5% of 100.00 on a 0.01 grid, as a `Fixed`.
    pub(crate) fn price(&mut self) -> Fixed {
        let cents = 9_500 + (self.next() % 1_001) as i64;
        Fixed::from_raw(cents * 10_000_000)
    }

    /// A positive quantity of 1..=5000 lots.
    pub(crate) fn qty(&mut self) -> Fixed {
        Fixed::from_int(1 + (self.next() % 5_000) as i64).unwrap()
    }

    /// A price a few billionths off the grid, for the rounding stage.
    pub(crate) fn off_grid(&mut self) -> Fixed {
        let p = self.price();
        Fixed::from_raw(p.raw() + (self.next() % 9_999_999) as i64)
    }
}

pub struct FixedPointRecipe;

impl SubMsRecipe for FixedPointRecipe {
    fn name(&self) -> &str {
        "fixed-point"
    }

    fn run(&self, h: &mut SubMsPerfHarness, params: &SubMsBenchParams) {
        let entries = params.entries;
        let warmup = params.warmup;
        let mut rng = Lcg::new(params.seed);
        let n = entries.max(1);
        let prices: Vec<Fixed> = (0..n).map(|_| rng.price()).collect();
        let qtys: Vec<Fixed> = (0..n).map(|_| rng.qty()).collect();
        let dirty: Vec<Fixed> = (0..n).map(|_| rng.off_grid()).collect();
        let tick = Fixed::from_raw(10_000_000);
        let mode = Rounding::HalfEven;
        let mut sink = 0i64;

        for i in 0..warmup {
            let k = i % n;
            sink ^= prices[k].checked_add(qtys[k]).map_or(0, Fixed::raw);
            sink ^= prices[k].checked_mul(qtys[k], mode).map_or(0, Fixed::raw);
            sink ^= prices[k].checked_div(qtys[k], mode).map_or(0, Fixed::raw);
            sink ^= dirty[k].round_to_tick(tick, mode).map_or(0, Fixed::raw);
        }

        let mut stage = |name: &str, op: &mut dyn FnMut(usize) -> Option<Fixed>| {
            let s = h.stage(name, entries).with_kind(SubMsStageKind::HotPath);
            for k in 0..entries {
                let t0 = SubMsTimer::tick();
                let r = op(k);
                let ns = t0.elapsed_ns();
                sink ^= r.map_or(0, Fixed::raw);
                s.record(ns);
            }
        };
        stage("add", &mut |k| prices[k].checked_add(qtys[k]));
        stage("mul", &mut |k| prices[k].checked_mul(qtys[k], mode));
        stage("div", &mut |k| prices[k].checked_div(qtys[k], mode));
        stage("round_tick", &mut |k| dirty[k].round_to_tick(tick, mode));

        black_box(sink);
        h.add_meta("decimals", "9");
        h.add_meta("rounding", mode.as_str());
        h.add_meta("tick", "0.01");
    }
}
