//! W-TinyLFU admission policy (Einziger + Friedman + Manasse, 2017).
//!
//! Cache is split into:
//!
//!   Window (1% of capacity) - small LRU. New keys enter here.
//!   Main protected (80% of main) - LRU; the popular working set.
//!   Main probation  (20% of main) - LRU; recent demotees + recent admits.
//!
//! When the Window evicts an entry, that entry becomes a CANDIDATE.
//! The candidate fights for admission against Main probation's LRU
//! VICTIM. We consult a count-min sketch (CMS) of recent access
//! frequencies and admit the candidate iff its frequency >= victim's
//! frequency.
//!
//! Doorkeeper: a small bloom filter that filters out "seen exactly
//! once" candidates before they ever touch the CMS. Reduces sketch
//! pressure.
//!
//! CMS uses a periodic "aging" pass: every `sample_size = 10*c`
//! accesses, all counters halve. This bounds the influence of stale
//! popularity bursts.
//!
//! The CMS implementation lives in this file rather than depending on
//! the sibling `subms-count-min-sketch` recipe. Reason: the cache is
//! a leaf in the dep graph; pulling in a sibling recipe would create
//! a cyclic-refresh risk on per-recipe releases and force consumers to
//! pull two crates for one feature. The local CMS is 4 rows of 32-bit
//! counters keyed by FNV-1a + linear probing, sized to 4 * c counters.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};

const FNV_OFFSET: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x100000001b3;

const CMS_ROWS: usize = 4;
const CMS_COUNTER_MAX: u32 = 15;

const NIL: u32 = u32::MAX;

struct Node<K, V> {
    key: K,
    value: V,
    prev: u32,
    next: u32,
    /// Window, Probation, or Protected.
    list: Segment,
}

#[derive(Copy, Clone, PartialEq, Eq)]
enum Segment {
    Window,
    Probation,
    Protected,
}

/// Count-min sketch with `CMS_ROWS` rows and `cols` counters per row.
/// Each counter is 4 bits (packed into u8). On insert we increment the
/// counter in every row; on query we return the min across rows.
struct Cms {
    cols: usize,
    rows: Vec<Vec<u8>>,
    additions: u64,
    sample_size: u64,
}

impl Cms {
    fn new(cells_per_row: usize, sample_size: u64) -> Self {
        let cols = cells_per_row.max(8).next_power_of_two();
        Self {
            cols,
            rows: (0..CMS_ROWS).map(|_| vec![0u8; cols.div_ceil(2)]).collect(),
            additions: 0,
            sample_size,
        }
    }

    fn idx(&self, row: usize, h: u64) -> usize {
        let seeded = h.wrapping_mul(SEEDS[row]).wrapping_add(SEEDS[row] >> 17);
        (seeded as usize) & (self.cols - 1)
    }

    fn read(&self, row: usize, idx: usize) -> u8 {
        let byte = self.rows[row][idx / 2];
        if idx & 1 == 0 {
            byte & 0x0f
        } else {
            (byte >> 4) & 0x0f
        }
    }
    fn write(&mut self, row: usize, idx: usize, v: u8) {
        let i = idx / 2;
        let v = v & 0x0f;
        if idx & 1 == 0 {
            self.rows[row][i] = (self.rows[row][i] & 0xf0) | v;
        } else {
            self.rows[row][i] = (self.rows[row][i] & 0x0f) | (v << 4);
        }
    }

    fn increment(&mut self, hash: u64) {
        let mut added = false;
        for r in 0..CMS_ROWS {
            let idx = self.idx(r, hash);
            let cur = self.read(r, idx) as u32;
            if cur < CMS_COUNTER_MAX {
                self.write(r, idx, (cur + 1) as u8);
                added = true;
            }
        }
        if added {
            self.additions += 1;
            if self.additions >= self.sample_size {
                self.reset();
            }
        }
    }

    fn estimate(&self, hash: u64) -> u32 {
        let mut m = u32::MAX;
        for r in 0..CMS_ROWS {
            let idx = self.idx(r, hash);
            let v = self.read(r, idx) as u32;
            if v < m {
                m = v;
            }
        }
        m
    }

    /// Halve every counter. Standard W-TinyLFU aging.
    fn reset(&mut self) {
        for r in 0..CMS_ROWS {
            for byte in self.rows[r].iter_mut() {
                let lo = (*byte & 0x0f) >> 1;
                let hi = ((*byte >> 4) & 0x0f) >> 1;
                *byte = (hi << 4) | lo;
            }
        }
        self.additions /= 2;
    }
}

const SEEDS: [u64; CMS_ROWS] = [
    0x9e3779b97f4a7c15,
    0xbf58476d1ce4e5b9,
    0x94d049bb133111eb,
    0x2545f4914f6cdd1d,
];

/// Doorkeeper: tiny bloom filter; toggles "have we seen this key at
/// least once recently". Catches the singleton scan case before it
/// pollutes the CMS counters.
struct Doorkeeper {
    bits: Vec<u64>,
    mask: u64,
}

impl Doorkeeper {
    fn new(bits: usize) -> Self {
        let words = bits.max(64).next_power_of_two().div_ceil(64);
        Self {
            bits: vec![0u64; words],
            mask: ((words * 64) as u64) - 1,
        }
    }
    fn idx(&self, hash: u64, salt: u64) -> usize {
        ((hash.wrapping_mul(salt)) & self.mask) as usize
    }
    /// Returns true if the key was already in the doorkeeper.
    fn check_or_add(&mut self, hash: u64) -> bool {
        let positions = [self.idx(hash, SEEDS[0]), self.idx(hash, SEEDS[1])];
        let mut all_set = true;
        for &p in &positions {
            let word = p >> 6;
            let bit = 1u64 << (p & 63);
            if self.bits[word] & bit == 0 {
                all_set = false;
                self.bits[word] |= bit;
            }
        }
        all_set
    }
    fn clear(&mut self) {
        for w in self.bits.iter_mut() {
            *w = 0;
        }
    }
}

/// W-TinyLFU cache.
pub struct TinyLfuCache<K, V> {
    c: usize,
    window_cap: usize,
    protected_cap: usize,
    probation_cap: usize,
    nodes: Vec<Option<Node<K, V>>>,
    free: Vec<u32>,
    index: HashMap<K, u32>,
    window_head: u32,
    window_tail: u32,
    window_len: usize,
    protected_head: u32,
    protected_tail: u32,
    protected_len: usize,
    probation_head: u32,
    probation_tail: u32,
    probation_len: usize,
    cms: Cms,
    doorkeeper: Doorkeeper,
    admissions: u64,
    rejections: u64,
}

impl<K: Hash + Eq + Clone, V> TinyLfuCache<K, V> {
    pub fn with_capacity(c: usize) -> Self {
        let c = c.max(4);
        // 1% window (floor of 1).
        let window_cap = (c / 100).max(1);
        let main = c - window_cap;
        let protected_cap = (main * 4 / 5).max(1);
        let probation_cap = (main - protected_cap).max(1);
        let cms_cells = (c * 4).max(64);
        let sample_size = (10 * c as u64).max(64);
        Self {
            c,
            window_cap,
            protected_cap,
            probation_cap,
            nodes: Vec::new(),
            free: Vec::new(),
            index: HashMap::new(),
            window_head: NIL,
            window_tail: NIL,
            window_len: 0,
            protected_head: NIL,
            protected_tail: NIL,
            protected_len: 0,
            probation_head: NIL,
            probation_tail: NIL,
            probation_len: 0,
            cms: Cms::new(cms_cells, sample_size),
            doorkeeper: Doorkeeper::new(c * 8),
            admissions: 0,
            rejections: 0,
        }
    }

    pub fn capacity(&self) -> usize {
        self.c
    }
    pub fn len(&self) -> usize {
        self.window_len + self.protected_len + self.probation_len
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn admissions(&self) -> u64 {
        self.admissions
    }
    pub fn rejections(&self) -> u64 {
        self.rejections
    }
    pub fn window_len(&self) -> usize {
        self.window_len
    }
    pub fn protected_len(&self) -> usize {
        self.protected_len
    }
    pub fn probation_len(&self) -> usize {
        self.probation_len
    }

    fn record_access(&mut self, hash: u64) {
        // If already past the doorkeeper, count in CMS proper; else
        // just record in the doorkeeper.
        if self.doorkeeper.check_or_add(hash) {
            self.cms.increment(hash);
            // Aging side-effect: when CMS resets (halves), also clear
            // doorkeeper to avoid permanent saturation.
            if self.cms.additions == 0 {
                self.doorkeeper.clear();
            }
        }
    }

    pub fn get(&mut self, key: &K) -> Option<&V> {
        let h = hash_one(key);
        self.record_access(h);
        let id = *self.index.get(key)?;
        let segment = self.nodes[id as usize].as_ref().unwrap().list;
        match segment {
            Segment::Window => {
                self.unlink(id);
                self.push_front_window(id);
            }
            Segment::Probation => {
                // Promote into Protected.
                self.unlink(id);
                self.probation_len -= 1;
                if self.protected_len >= self.protected_cap {
                    // Demote LRU of Protected back to Probation.
                    let demote = self.protected_tail;
                    self.unlink(demote);
                    self.protected_len -= 1;
                    self.nodes[demote as usize].as_mut().unwrap().list = Segment::Probation;
                    self.push_front_probation(demote);
                    self.probation_len += 1;
                }
                self.nodes[id as usize].as_mut().unwrap().list = Segment::Protected;
                self.push_front_protected(id);
                self.protected_len += 1;
            }
            Segment::Protected => {
                self.unlink(id);
                self.push_front_protected(id);
            }
        }
        self.nodes[id as usize].as_ref().map(|n| &n.value)
    }

    /// Insert or update. Returns the rejected/evicted key+value if any.
    pub fn put(&mut self, key: K, value: V) -> Option<(K, V)> {
        let h = hash_one(&key);
        self.record_access(h);

        if let Some(&id) = self.index.get(&key) {
            let n = self.nodes[id as usize].as_mut().unwrap();
            n.value = value;
            let seg = n.list;
            self.unlink(id);
            match seg {
                Segment::Window => self.push_front_window(id),
                Segment::Probation => self.push_front_probation(id),
                Segment::Protected => self.push_front_protected(id),
            }
            return None;
        }

        // Try to fit into Window first.
        if self.window_len < self.window_cap {
            let id = self.alloc(Node {
                key: key.clone(),
                value,
                prev: NIL,
                next: NIL,
                list: Segment::Window,
            });
            self.index.insert(key, id);
            self.push_front_window(id);
            self.window_len += 1;
            return None;
        }

        // Window full: eject window-LRU as the candidate; either move
        // to Probation or fight Probation-LRU for admission.
        let candidate_id = self.window_tail;
        self.unlink(candidate_id);
        self.window_len -= 1;

        // The new key takes the window head slot.
        let new_id = self.alloc(Node {
            key: key.clone(),
            value,
            prev: NIL,
            next: NIL,
            list: Segment::Window,
        });
        self.index.insert(key, new_id);
        self.push_front_window(new_id);
        self.window_len += 1;

        // Where does the candidate go?
        if self.probation_len < self.probation_cap {
            // Probation has room - move candidate in, no fight.
            self.nodes[candidate_id as usize].as_mut().unwrap().list = Segment::Probation;
            self.push_front_probation(candidate_id);
            self.probation_len += 1;
            self.admissions += 1;
            return None;
        }

        // Probation full - admission fight.
        let victim_id = self.probation_tail;
        let cand_hash = hash_one(&self.nodes[candidate_id as usize].as_ref().unwrap().key);
        let vic_hash = hash_one(&self.nodes[victim_id as usize].as_ref().unwrap().key);
        let cand_freq = self.cms.estimate(cand_hash);
        let vic_freq = self.cms.estimate(vic_hash);

        if cand_freq >= vic_freq {
            // Admit. Evict victim.
            self.unlink(victim_id);
            self.probation_len -= 1;
            let v = self.nodes[victim_id as usize].take().unwrap();
            self.index.remove(&v.key);
            self.free.push(victim_id);

            self.nodes[candidate_id as usize].as_mut().unwrap().list = Segment::Probation;
            self.push_front_probation(candidate_id);
            self.probation_len += 1;
            self.admissions += 1;
            Some((v.key, v.value))
        } else {
            // Reject candidate; drop it.
            let c = self.nodes[candidate_id as usize].take().unwrap();
            self.index.remove(&c.key);
            self.free.push(candidate_id);
            self.rejections += 1;
            Some((c.key, c.value))
        }
    }

    fn alloc(&mut self, n: Node<K, V>) -> u32 {
        if let Some(id) = self.free.pop() {
            self.nodes[id as usize] = Some(n);
            id
        } else {
            let id = self.nodes.len() as u32;
            self.nodes.push(Some(n));
            id
        }
    }

    fn unlink(&mut self, id: u32) {
        let (prev, next, seg) = {
            let n = self.nodes[id as usize].as_ref().unwrap();
            (n.prev, n.next, n.list)
        };
        if prev != NIL {
            self.nodes[prev as usize].as_mut().unwrap().next = next;
        }
        if next != NIL {
            self.nodes[next as usize].as_mut().unwrap().prev = prev;
        }
        let n = self.nodes[id as usize].as_mut().unwrap();
        n.prev = NIL;
        n.next = NIL;
        match seg {
            Segment::Window => {
                if self.window_head == id {
                    self.window_head = next;
                }
                if self.window_tail == id {
                    self.window_tail = prev;
                }
            }
            Segment::Probation => {
                if self.probation_head == id {
                    self.probation_head = next;
                }
                if self.probation_tail == id {
                    self.probation_tail = prev;
                }
            }
            Segment::Protected => {
                if self.protected_head == id {
                    self.protected_head = next;
                }
                if self.protected_tail == id {
                    self.protected_tail = prev;
                }
            }
        }
    }

    fn push_front_window(&mut self, id: u32) {
        let old = self.window_head;
        let n = self.nodes[id as usize].as_mut().unwrap();
        n.next = old;
        n.prev = NIL;
        if old != NIL {
            self.nodes[old as usize].as_mut().unwrap().prev = id;
        }
        self.window_head = id;
        if self.window_tail == NIL {
            self.window_tail = id;
        }
    }
    fn push_front_protected(&mut self, id: u32) {
        let old = self.protected_head;
        let n = self.nodes[id as usize].as_mut().unwrap();
        n.next = old;
        n.prev = NIL;
        if old != NIL {
            self.nodes[old as usize].as_mut().unwrap().prev = id;
        }
        self.protected_head = id;
        if self.protected_tail == NIL {
            self.protected_tail = id;
        }
    }
    fn push_front_probation(&mut self, id: u32) {
        let old = self.probation_head;
        let n = self.nodes[id as usize].as_mut().unwrap();
        n.next = old;
        n.prev = NIL;
        if old != NIL {
            self.nodes[old as usize].as_mut().unwrap().prev = id;
        }
        self.probation_head = id;
        if self.probation_tail == NIL {
            self.probation_tail = id;
        }
    }
}

fn hash_one<K: Hash>(k: &K) -> u64 {
    let mut h = FnvHasher::new();
    k.hash(&mut h);
    h.finish()
}

struct FnvHasher(u64);
impl FnvHasher {
    fn new() -> Self {
        FnvHasher(FNV_OFFSET)
    }
}
impl Hasher for FnvHasher {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 ^= b as u64;
            self.0 = self.0.wrapping_mul(FNV_PRIME);
        }
    }
}

#[cfg(test)]
#[path = "tinylfu_tests.rs"]
mod tests;
