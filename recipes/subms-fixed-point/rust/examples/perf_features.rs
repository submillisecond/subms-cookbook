//! Feature classification bench.

use std::collections::BTreeMap;
use std::hint::black_box;
use std::io::{self, Write};
use std::path::PathBuf;

use subms::{SubMsFeatureManifest, SubMsP99Source, SubMsPerfHarness, classify_feature, summarize};
use subms_fixed_point::{Fixed, Rounding};

const SIZES: [usize; 3] = [1_024, 65_536, 4_194_304];
const CANON: usize = SIZES[SIZES.len() - 1];
const ITEMS_PER_SAMPLE: usize = 1_024;
const SAMPLES: usize = 256;
const ROUNDS: usize = 7;
const OPS: usize = 50_000;
const WARM_NANOS: u64 = 300_000_000;
const WARM_MAX_SAMPLES: usize = 1_000;

struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 11
    }
}

/// `n` prices on the one-cent grid and `n` whole-lot quantities.
fn operands(n: usize) -> (Vec<Fixed>, Vec<Fixed>) {
    let mut r = Lcg(0x9E37_79B9_7F4A_7C15);
    let px = (0..n)
        .map(|_| Fixed::from_raw((9_500 + (r.next() % 1_001) as i64) * 10_000_000))
        .collect();
    let qty = (0..n)
        .map(|_| Fixed::from_int(1 + (r.next() % 5_000) as i64).unwrap())
        .collect();
    (px, qty)
}

fn stat(h: &SubMsPerfHarness, median: bool) -> u64 {
    summarize(h)
        .stages
        .iter()
        .find(|s| s.name == "op")
        .map_or(0, |s| if median { s.p50_ns } else { s.p99_ns })
}

fn batched(reps: usize, mut op: impl FnMut(usize) -> i64) -> u64 {
    let mut i = 0usize;
    let start = std::time::Instant::now();
    for _ in 0..WARM_MAX_SAMPLES {
        let mut acc = 0i64;
        for _ in 0..reps {
            acc = acc.wrapping_add(op(i));
            i += 1;
        }
        black_box(acc);
        if start.elapsed().as_nanos() as u64 >= WARM_NANOS {
            break;
        }
    }
    let mut h = SubMsPerfHarness::new("fixed-point-feature", "rust");
    let st = h.stage("op", SAMPLES);
    for _ in 0..SAMPLES {
        st.time(|| {
            let mut acc = 0i64;
            for _ in 0..reps {
                acc = acc.wrapping_add(op(i));
                i += 1;
            }
            black_box(acc);
        });
    }
    stat(&h, true)
}

fn single(mut op: impl FnMut(usize) -> i64) -> u64 {
    for i in 0..OPS {
        black_box(op(i));
    }
    let mut h = SubMsPerfHarness::new("fixed-point-feature", "rust");
    let st = h.stage("op", OPS);
    for i in 0..OPS {
        st.time(|| black_box(op(i)));
    }
    stat(&h, false)
}

fn sweep(label: &str, mut at: impl FnMut(usize) -> u64) -> Vec<(usize, u64)> {
    let mut runs: Vec<Vec<u64>> = vec![Vec::new(); SIZES.len()];
    for _ in 0..ROUNDS {
        for (k, &n) in SIZES.iter().enumerate() {
            runs[k].push(at(n));
        }
    }
    let rows: Vec<(usize, u64)> = SIZES
        .iter()
        .enumerate()
        .map(|(k, &n)| (n, runs[k].iter().copied().min().unwrap_or(0)))
        .collect();
    eprintln!("sweep {label}: {rows:?} raw {runs:?}");
    rows
}

fn main() -> io::Result<()> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(".subms")
        .join("features")
        .join("rust.json");
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    let mut manifest = SubMsFeatureManifest::load_str("rust", &existing);
    let (source, instance) = SubMsP99Source::from_env();
    manifest.set_p99_source(source, instance.as_deref());

    let mode = Rounding::HalfEven;

    // The baseline: a checked multiply, the base type's representative op.
    let base_sweep = sweep("base/mul", |n| {
        let (px, qty) = operands(n);
        let mask = n - 1;
        batched(ITEMS_PER_SAMPLE, |i| {
            px[i & mask]
                .checked_mul(qty[i & mask], mode)
                .map_or(0, Fixed::raw)
        })
    });
    let base_p50 = base_sweep
        .iter()
        .find(|(n, _)| *n == CANON)
        .map_or(0, |(_, v)| *v);
    eprintln!("base mul p50 per {ITEMS_PER_SAMPLE}-op sample: {base_p50}ns");

    // ---------- text: parse + write, no allocation ----------
    #[cfg(feature = "text")]
    {
        fn texts(n: usize) -> Vec<([u8; 16], usize)> {
            let (px, _) = operands(n);
            px.iter()
                .map(|p| {
                    let mut b = [0u8; 16];
                    let len = p.write_to(&mut b, 2).unwrap();
                    (b, len)
                })
                .collect()
        }
        let sw = sweep("text/parse+write", |n| {
            let t = texts(n);
            let mask = n - 1;
            let mut out = [0u8; 32];
            batched(ITEMS_PER_SAMPLE, |i| {
                let (b, len) = &t[i & mask];
                let v = Fixed::parse_bytes(&b[..*len]).unwrap();
                v.write_to(&mut out, 2).unwrap() as i64 + v.raw()
            })
        });
        let (cat, reason) = classify_feature(&sw, Some(base_p50), None);

        let t = texts(CANON);
        let mask = CANON - 1;
        let parse = single(|i| {
            let (b, len) = &t[i & mask];
            Fixed::parse_bytes(&b[..*len]).unwrap().raw()
        });
        let (px, _) = operands(CANON);
        let mut out = [0u8; 32];
        let write = single(|i| px[i & mask].write_to(&mut out, 2).unwrap() as i64);
        let mut p99 = BTreeMap::new();
        p99.insert("parse".to_string(), parse);
        p99.insert("write".to_string(), write);
        manifest.set_feature("text", cat, &p99, &reason);
    }

    // ---------- money: currency check + same arithmetic ----------
    #[cfg(feature = "money")]
    {
        use subms_fixed_point::{Currency, Money};
        let sw = sweep("money/add", |n| {
            let (px, qty) = operands(n);
            let mask = n - 1;
            let a: Vec<Money> = px.iter().map(|p| Money::new(*p, Currency::USD)).collect();
            let b: Vec<Money> = qty.iter().map(|q| Money::new(*q, Currency::USD)).collect();
            batched(ITEMS_PER_SAMPLE, |i| {
                a[i & mask]
                    .checked_add(b[i & mask])
                    .map_or(0, |m| m.amount().raw())
            })
        });
        let (cat, reason) = classify_feature(&sw, Some(base_p50), None);

        let (px, qty) = operands(CANON);
        let mask = CANON - 1;
        let a: Vec<Money> = px.iter().map(|p| Money::new(*p, Currency::USD)).collect();
        let b: Vec<Money> = qty.iter().map(|q| Money::new(*q, Currency::USD)).collect();
        let add = single(|i| {
            a[i & mask]
                .checked_add(b[i & mask])
                .map_or(0, |m| m.amount().raw())
        });
        let rate = Fixed::from_parts(1, 84_500_000).unwrap();
        let convert = single(|i| {
            a[i & mask]
                .convert(rate, Currency::EUR, mode)
                .map_or(0, |m| m.amount().raw())
        });
        let mut p99 = BTreeMap::new();
        p99.insert("money_add".to_string(), add);
        p99.insert("convert".to_string(), convert);
        manifest.set_feature("money", cat, &p99, &reason);
    }

    // ---------- wide: exact 128-bit product accumulation ----------
    #[cfg(feature = "wide")]
    {
        use subms_fixed_point::Fixed128;
        let sw = sweep("wide/add_product", |n| {
            let (px, qty) = operands(n);
            let mask = n - 1;
            let mut acc = Fixed128::ZERO;
            batched(ITEMS_PER_SAMPLE, |i| {
                acc = acc
                    .checked_add_product(px[i & mask], qty[i & mask])
                    .unwrap_or(Fixed128::ZERO);
                acc.raw() as i64
            })
        });
        let (cat, reason) = classify_feature(&sw, Some(base_p50), None);

        let (px, qty) = operands(CANON);
        let mask = CANON - 1;
        let mut acc = Fixed128::ZERO;
        let add_product = single(|i| {
            acc = acc
                .checked_add_product(px[i & mask], qty[i & mask])
                .unwrap_or(Fixed128::ZERO);
            acc.raw() as i64
        });
        let products: Vec<Fixed128> = px
            .iter()
            .zip(&qty)
            .map(|(p, q)| Fixed128::product(*p, *q))
            .collect();
        let to_fixed = single(|i| products[i & mask].to_fixed(mode).map_or(0, Fixed::raw));
        let mut p99 = BTreeMap::new();
        p99.insert("add_product".to_string(), add_product);
        p99.insert("to_fixed".to_string(), to_fixed);
        manifest.set_feature("wide", cat, &p99, &reason);
    }

    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(&path, manifest.to_json())?;
    io::stdout().write_all(manifest.to_json().as_bytes())?;
    Ok(())
}
