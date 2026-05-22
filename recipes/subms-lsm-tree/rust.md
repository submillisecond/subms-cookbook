---
lang: rust
---

## Quickstart

```toml
[dependencies]
subms-lsm-tree = "0.3"
```

```rust
use subms_lsm_tree::{BloomMode, LsmTree};

fn main() -> std::io::Result<()> {
    let dir = std::env::temp_dir().join("lsm-quickstart");
    std::fs::create_dir_all(&dir)?;

    let mut lsm = LsmTree::open_with(&dir, 16_000, BloomMode::On)?;
    lsm.put("alice", b"42")?;
    let v = lsm.get("alice")?;
    assert_eq!(v.as_deref(), Some(&b"42"[..]));
    Ok(())
}
```

That's the whole end-user surface (`open_with` / `put` / `get` / `delete` / `flush` plus `BloomMode`). The `harness` feature pulls in `subms` + `LsmTreeRecipe` if you want the standard `cargo run --example perf_main` and `sub_millisecond_bench`.

### Step 1 - the memtable

A `BTreeMap` keyed on `String`, values as `Option<Vec<u8>>`. `None` is a tombstone - *present* in the map, marking the key as deleted, so a later flush carries the deletion onto disk.

```rust
use std::collections::BTreeMap;

pub(crate) struct Memtable {
    entries: BTreeMap<String, Option<Vec<u8>>>,
    approx_size_bytes: usize,
}

impl Memtable {
    pub(crate) fn put(&mut self, key: &str, value: Option<Vec<u8>>) {
        let new_val_cost = value.as_ref().map(|v| v.len()).unwrap_or(1);
        match self.entries.insert(key.to_string(), value) {
            None => self.approx_size_bytes += key.len() + new_val_cost,
            Some(prev) => {
                let prev_cost = prev.as_ref().map(|v| v.len()).unwrap_or(1);
                self.approx_size_bytes = self.approx_size_bytes + new_val_cost - prev_cost;
            }
        }
    }

    pub(crate) fn get(&self, key: &str) -> Option<Option<&[u8]>> {
        self.entries.get(key).map(|opt| opt.as_deref())
    }
}
```

The three-valued return of `get` is the trick: `Some(Some(v))` is a hit, `Some(None)` is a tombstone, `None` is "not in this layer". The LSM coordinator uses that to stop the walk early when it finds a tombstone.

### Step 2 - the SSTable, with bloom trailer

On `open()`, slurp the whole file into a `Vec<u8>` and parse the bloom filter out of the trailer. After this, a get never touches the filesystem.

```rust
use bloom_filter_cookbook::BloomFilter;
use std::fs;

const MAGIC: u32 = 0x4C534D54; // "LSMT"
const FOOTER_BYTES: usize = 8 + 4;

pub(crate) struct SsTable {
    buf: Vec<u8>,
    records_end: usize,
    bloom: BloomFilter,
}

impl SsTable {
    pub(crate) fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let buf = fs::read(path)?;
        let magic = u32::from_be_bytes(buf[buf.len()-4..].try_into().unwrap());
        if magic != MAGIC { return Err(io::Error::new(io::ErrorKind::InvalidData, "bad magic")); }
        let footer_off = buf.len() - FOOTER_BYTES;
        let records_end = u64::from_be_bytes(buf[footer_off..footer_off+8].try_into().unwrap()) as usize;
        let bloom = BloomFilter::parse(&buf[records_end..footer_off])?;
        Ok(Self { buf, records_end, bloom })
    }
}
```

The footer (last 12 bytes) is the navigational anchor: it tells us where the records end and the bloom section begins, without scanning the file.

### Step 3 - the read path, bloom-checked

The bloom probe goes first. A negative answer is final; a positive answer falls through to a linear scan of the in-memory buffer.

```rust
impl SsTable {
    pub(crate) fn get(&self, key: &str, check_bloom: bool) -> Option<Option<Vec<u8>>> {
        if check_bloom && !self.bloom.might_contain(key) {
            return None;
        }
        let kb = key.as_bytes();
        let mut p = 0usize;
        while p < self.records_end {
            let key_len = u32::from_be_bytes(self.buf[p..p+4].try_into().unwrap()) as usize;
            p += 4;
            let key_slice = &self.buf[p..p+key_len];
            let cmp = key_slice.cmp(kb);
            p += key_len;
            let flag = self.buf[p]; p += 1;
            let value_len = u32::from_be_bytes(self.buf[p..p+4].try_into().unwrap()) as usize;
            p += 4;
            match cmp {
                Ordering::Equal => return Some(if flag == 0x01 { None } else { Some(self.buf[p..p+value_len].to_vec()) }),
                Ordering::Greater => return None,           // sorted file: passed it
                Ordering::Less => p += value_len,
            }
        }
        None
    }
}
```

`check_bloom` is the `BloomMode` knob plumbed down from the LSM coordinator. With it `true` (default), miss latency is dominated by seven hash probes per SSTable. With it `false`, you pay a full scan of every SSTable in the walk - that's the demonstrable case for the optimisation.

### Step 4 - the LSM coordinator

The top-level read walks memtable then SSTables newest-first; the bloom mode flows through to each `SsTable::get`.

```rust
pub enum BloomMode { On, Off }

pub struct LsmTree {
    memtable: Memtable,
    sstables: Vec<SsTable>,
    bloom_mode: BloomMode,
    // ... data_dir, flush threshold, next_seq
}

impl LsmTree {
    pub fn get(&self, key: &str) -> io::Result<Option<Vec<u8>>> {
        if let Some(hit) = self.memtable.get(key) {
            return Ok(hit.map(|v| v.to_vec()));
        }
        let check_bloom = matches!(self.bloom_mode, BloomMode::On);
        for sst in self.sstables.iter().rev() {     // newest first
            if let Some(hit) = sst.get(key, check_bloom) {
                return Ok(hit);
            }
        }
        Ok(None)
    }
}
```

A tombstone returned from any layer (`Some(None)` from the memtable or sstable's `get`) collapses to `Ok(None)` for the caller - they see "absent" without knowing if it was never written or actively deleted.

### Step 5 - the perf test

The cookbook's perf test runs both bloom modes back-to-back and asserts p99 < 1 ms in the on pass:

```text
$ cargo test --release --features harness --test sub_millisecond_bench -- --nocapture

entries=50000  flush_threshold_bytes=16000  warmup=5000

bloom = on
  stage            p50        p99      p99.9        max       mean
  put            200ns      400ns     20.0us    881.0us      571ns
  get_hit        4.2us     14.6us     76.7us    595.6us      4.6us
  get_miss       1.8us     17.5us     77.2us     2.61ms      3.9us

bloom = off
  stage            p50        p99      p99.9        max       mean
  put            200ns      400ns     25.8us     1.17ms      653ns
  get_hit        4.3us    310.1us    566.1us     5.39ms     41.4us
  get_miss     280.8us    596.7us     1.22ms     5.09ms    315.4us

OK (BloomMode::On - all p99 < 1ms)
```

The percentile table is emitted by `subms::print_summary` - the same formatter the Java bench uses, so output is layout-equivalent across languages. `put max ~ 1 ms` is the one-in-N puts that crossed the flush threshold and rolled an SSTable on the calling thread - the case for moving flushes to a background thread, which this cookbook deliberately doesn't. p99 is the honest steady-state number; the `max` excursions on `get_miss` and `get_hit` are cold-page touches plus the OS scheduler.

Full source at [`cookbook/recipes/subms-lsm-tree/rust`](https://github.com/submillisecond/subms-cookbook/tree/main/recipes/subms-lsm-tree/rust).
