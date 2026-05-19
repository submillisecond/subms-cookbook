---
lang: rust
---

## Quickstart

```toml
[dependencies]
subms-bloom-filter = "0.3"
```

```rust
use subms_bloom_filter::BloomFilter;

fn main() {
    let mut bf = BloomFilter::new(10_000);
    bf.add("alice");
    assert!(bf.might_contain("alice"));
    assert!(!bf.might_contain("bob"));
}
```

That's the whole end-user surface. The perf-harness adapter behind the bench numbers below sits behind the `harness` feature: enable it with `subms-bloom-filter = { version = "0.3", features = ["harness"] }` if you want the `Recipe` impl and the standard `cargo run --example perf_main`.

### Step 1 - the struct

A bit-array sized at construction, a fixed `k`, and a deterministic non-cryptographic hash.

```rust
const FNV_OFFSET: u64 = 0xcbf29ce484222325;
const FNV_PRIME:  u64 = 0x100000001b3;

pub struct BloomFilter {
    bit_count: u32,
    k: u32,
    bits: Vec<u64>,
}

impl BloomFilter {
    pub fn new(expected_entries: usize) -> Self {
        let bit_count = expected_entries.saturating_mul(10).max(64) as u32;
        let words = ((bit_count as usize) + 63) / 64;
        Self { bit_count, k: 7, bits: vec![0u64; words] }
    }
}

fn fnv1a64(key: &str) -> u64 {
    let mut h = FNV_OFFSET;
    for &b in key.as_bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(FNV_PRIME);
    }
    h
}
```

`saturating_mul(10).max(64)` is the 10-bits-per-key default with a 64-bit floor so `bit_count = 0` is impossible.

### Step 2 - add & might_contain, double-hashed

One FNV-1a call per operation, split into two halves, seven probes.

```rust
impl BloomFilter {
    pub fn add(&mut self, key: &str) {
        let h = fnv1a64(key);
        let h1 = h as u32;
        let h2 = ((h >> 32) as u32) | 1;     // force odd so we never degenerate
        for i in 0..self.k {
            let idx = h1.wrapping_add(i.wrapping_mul(h2)) % self.bit_count;
            self.bits[(idx / 64) as usize] |= 1u64 << (idx % 64);
        }
    }

    pub fn might_contain(&self, key: &str) -> bool {
        let h = fnv1a64(key);
        let h1 = h as u32;
        let h2 = ((h >> 32) as u32) | 1;
        for i in 0..self.k {
            let idx = h1.wrapping_add(i.wrapping_mul(h2)) % self.bit_count;
            if self.bits[(idx / 64) as usize] & (1u64 << (idx % 64)) == 0 {
                return false;
            }
        }
        true
    }
}
```

`wrapping_*` is important: `i * h2` overflows for large `h2`, and we want modular arithmetic, not a panic. The probe distribution stays uniform.

### Step 3 - serialisation

Fixed wire format so a filter written by one process (or one language) is readable by another.

```rust
use std::io::{self, Write};

impl BloomFilter {
    pub fn write_to<W: Write>(&self, out: &mut W) -> io::Result<()> {
        out.write_all(&self.bit_count.to_be_bytes())?;
        out.write_all(&self.k.to_be_bytes())?;
        out.write_all(&(self.bits.len() as u32).to_be_bytes())?;
        for w in &self.bits {
            out.write_all(&w.to_be_bytes())?;
        }
        Ok(())
    }

    pub fn parse(buf: &[u8]) -> io::Result<Self> {
        if buf.len() < 12 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "bloom section too short"));
        }
        let bit_count = u32::from_be_bytes(buf[0..4].try_into().unwrap());
        let k         = u32::from_be_bytes(buf[4..8].try_into().unwrap());
        let words     = u32::from_be_bytes(buf[8..12].try_into().unwrap()) as usize;
        if buf.len() < 12 + words * 8 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "bloom section truncated"));
        }
        let mut bits = Vec::with_capacity(words);
        for i in 0..words {
            let off = 12 + i * 8;
            bits.push(u64::from_be_bytes(buf[off..off + 8].try_into().unwrap()));
        }
        Ok(Self { bit_count, k, bits })
    }
}
```

`parse` takes a slice so the SSTable trailer can hand it a zero-copy view of the file's mmap'd byte buffer.

### Step 4 - verify the FPR

The cookbook's correctness test pins it at ~1%:

```rust
#[test]
fn false_positive_rate_is_roughly_1_percent() {
    let n = 10_000;
    let mut bf = BloomFilter::new(n);
    for i in 0..n { bf.add(&format!("present{i}")); }

    let probes = 100_000;
    let mut fp = 0;
    for i in 0..probes {
        if bf.might_contain(&format!("absent{i}")) { fp += 1; }
    }
    let fpr = fp as f64 / probes as f64;
    assert!(fpr < 0.05, "fpr {fpr:.4} too high");
}
```

5% is generous headroom; a typical run sits around 1%. The full crate is at [`cookbook/recipes/subms-bloom-filter/rust`](https://github.com/stochbook/cookbook/tree/main/recipes/subms-bloom-filter/rust) - `cargo test --release` runs round-trip, serialisation round-trip, and the FPR check.
