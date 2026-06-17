//! Weighted block cache: per-entry byte size.
//!
//! The base cache assumes uniform entry size and counts capacity in
//! slots. A weighted cache counts capacity in bytes. The caller
//! supplies `size_of: fn(&V) -> usize` (or any closure). On `put`, the
//! cache adds the entry's size to a running total; if the total
//! exceeds `capacity_bytes` it evicts via clock-sweep (LRU-ish via
//! second-chance) until the total fits again. Eviction may free more
//! than the minimum needed for one put if the working set has many
//! small entries.
//!
//! A single put whose value is itself larger than capacity is
//! rejected: the cache cannot hold it, so we return the new
//! key+value as the "evicted" pair without touching residents.

use std::collections::HashMap;
use std::hash::Hash;

const NIL: u32 = u32::MAX;

struct Slot<K, V> {
    key: K,
    value: V,
    size: usize,
    referenced: bool,
}

pub struct WeightedCache<K, V> {
    capacity_bytes: usize,
    used_bytes: usize,
    slots: Vec<Option<Slot<K, V>>>,
    index: HashMap<K, u32>,
    hand: u32,
    size_of: Box<dyn Fn(&V) -> usize>,
    // Indices of slots that were vacated by eviction. `put` pops from
    // here instead of scanning `slots` for a `None` hole, keeping the
    // insert path O(1) under a steady eviction workload.
    free_slots: Vec<u32>,
}

impl<K: Hash + Eq + Clone, V> WeightedCache<K, V> {
    pub fn with_capacity_bytes<F>(capacity_bytes: usize, size_of: F) -> Self
    where
        F: Fn(&V) -> usize + 'static,
    {
        Self {
            capacity_bytes: capacity_bytes.max(1),
            used_bytes: 0,
            slots: Vec::new(),
            index: HashMap::new(),
            hand: 0,
            size_of: Box::new(size_of),
            free_slots: Vec::new(),
        }
    }

    pub fn capacity_bytes(&self) -> usize {
        self.capacity_bytes
    }
    pub fn used_bytes(&self) -> usize {
        self.used_bytes
    }
    pub fn len(&self) -> usize {
        self.index.len()
    }
    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    pub fn get(&mut self, key: &K) -> Option<&V> {
        let id = *self.index.get(key)?;
        let s = self.slots[id as usize].as_mut().unwrap();
        s.referenced = true;
        Some(&self.slots[id as usize].as_ref().unwrap().value)
    }

    /// Insert or update. May evict multiple entries to fit the new one.
    /// Returns a Vec of evicted (key, value) pairs in eviction order;
    /// empty if nothing was evicted.
    pub fn put(&mut self, key: K, value: V) -> Vec<(K, V)> {
        let new_size = (self.size_of)(&value);

        // Single-value-larger-than-capacity: reject outright.
        if new_size > self.capacity_bytes {
            return vec![(key, value)];
        }

        // Update path: replace in place; adjust used_bytes by delta.
        if let Some(&id) = self.index.get(&key) {
            let old_size = self.slots[id as usize].as_ref().unwrap().size;
            let s = self.slots[id as usize].as_mut().unwrap();
            s.value = value;
            s.referenced = true;
            s.size = new_size;
            self.used_bytes = self.used_bytes + new_size - old_size;
            // If the new value bloats us past capacity, evict others.
            let mut evicted = Vec::new();
            while self.used_bytes > self.capacity_bytes {
                if let Some(ev) = self.sweep_evict_excluding(id) {
                    evicted.push(ev);
                } else {
                    break;
                }
            }
            return evicted;
        }

        // Insert path. Evict until there's room for `new_size`.
        let mut evicted = Vec::new();
        while self.used_bytes + new_size > self.capacity_bytes {
            if let Some(ev) = self.sweep_evict_excluding(NIL) {
                evicted.push(ev);
            } else {
                break;
            }
        }

        // Reuse a vacated slot in O(1) via the free-slot stack; fall
        // back to pushing a new slot only when none are free.
        let new_id = if let Some(i) = self.free_slots.pop() {
            self.slots[i as usize] = Some(Slot {
                key: key.clone(),
                value,
                size: new_size,
                referenced: true,
            });
            i
        } else {
            let id = self.slots.len() as u32;
            self.slots.push(Some(Slot {
                key: key.clone(),
                value,
                size: new_size,
                referenced: true,
            }));
            id
        };
        self.index.insert(key, new_id);
        self.used_bytes += new_size;
        evicted
    }

    /// Sweep, evicting a non-referenced slot. Returns None if nothing
    /// is resident. Sweep terminates because every populated,
    /// non-skipped slot has its ref bit cleared on first visit and
    /// is evictable on second visit. Total visits bounded by 2n + 1.
    fn sweep_evict_excluding(&mut self, skip: u32) -> Option<(K, V)> {
        if self.index.is_empty() {
            return None;
        }
        let n = self.slots.len();
        if n == 0 {
            return None;
        }
        for _ in 0..(2 * n + 1) {
            let i = (self.hand as usize) % n;
            self.hand = ((self.hand as usize + 1) % n.max(1)) as u32;
            let want_evict = {
                let s = match self.slots[i].as_mut() {
                    Some(s) => s,
                    None => continue,
                };
                if (i as u32) == skip {
                    false
                } else if s.referenced {
                    s.referenced = false;
                    false
                } else {
                    true
                }
            };
            if want_evict {
                let s = self.slots[i].take().unwrap();
                self.index.remove(&s.key);
                self.used_bytes -= s.size;
                self.free_slots.push(i as u32);
                return Some((s.key, s.value));
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_entries_fit_under_capacity() {
        let mut c: WeightedCache<u32, Vec<u8>> =
            WeightedCache::with_capacity_bytes(100, |v: &Vec<u8>| v.len());
        let ev1 = c.put(1, vec![0u8; 10]);
        let ev2 = c.put(2, vec![0u8; 20]);
        assert!(ev1.is_empty());
        assert!(ev2.is_empty());
        assert_eq!(c.used_bytes(), 30);
        assert_eq!(c.len(), 2);
    }

    #[test]
    fn evicts_to_make_room() {
        let mut c: WeightedCache<u32, Vec<u8>> =
            WeightedCache::with_capacity_bytes(50, |v: &Vec<u8>| v.len());
        c.put(1, vec![0u8; 30]);
        c.put(2, vec![0u8; 10]);
        let ev = c.put(3, vec![0u8; 30]);
        assert!(
            !ev.is_empty(),
            "should have evicted to fit 30B + 10B + 30B into 50B"
        );
        assert!(c.used_bytes() <= 50, "used={}", c.used_bytes());
    }

    #[test]
    fn entry_larger_than_capacity_is_rejected() {
        let mut c: WeightedCache<u32, Vec<u8>> =
            WeightedCache::with_capacity_bytes(10, |v: &Vec<u8>| v.len());
        let ev = c.put(1, vec![0u8; 100]);
        assert_eq!(ev.len(), 1);
        assert_eq!(ev[0].0, 1);
        assert_eq!(c.used_bytes(), 0, "rejected entry must not be counted");
        assert!(c.get(&1).is_none());
    }

    #[test]
    fn update_in_place_adjusts_used_bytes() {
        let mut c: WeightedCache<u32, Vec<u8>> =
            WeightedCache::with_capacity_bytes(100, |v: &Vec<u8>| v.len());
        c.put(1, vec![0u8; 10]);
        c.put(1, vec![0u8; 40]);
        assert_eq!(c.used_bytes(), 40);
        assert_eq!(c.len(), 1);
        assert_eq!(c.get(&1).unwrap().len(), 40);
    }

    #[test]
    fn update_bloating_evicts_others() {
        let mut c: WeightedCache<u32, Vec<u8>> =
            WeightedCache::with_capacity_bytes(60, |v: &Vec<u8>| v.len());
        c.put(1, vec![0u8; 10]);
        c.put(2, vec![0u8; 10]);
        c.put(3, vec![0u8; 10]);
        // Grow key 1 from 10 -> 50; that bloats us to 70B over a 60B cap.
        let ev = c.put(1, vec![0u8; 50]);
        assert!(!ev.is_empty());
        assert!(c.used_bytes() <= 60);
        // Key 1 must still be resident (it's the one we just updated).
        assert!(c.get(&1).is_some());
    }

    #[test]
    fn touched_entry_survives_sweep() {
        // Fill to capacity, then trigger one eviction. That sweep
        // clears all ref bits + evicts one of them, leaving the
        // remaining residents with ref=false. Touch key 2 so its ref
        // bit goes back to true. The next eviction's sweep should
        // give key 2 a second chance and pick one of the others.
        let mut c: WeightedCache<u32, Vec<u8>> =
            WeightedCache::with_capacity_bytes(40, |v: &Vec<u8>| v.len());
        c.put(1, vec![0u8; 10]);
        c.put(2, vec![0u8; 10]);
        c.put(3, vec![0u8; 10]);
        c.put(4, vec![0u8; 10]);
        let _ = c.put(5, vec![0u8; 10]); // first eviction; sweeps + clears refs.
        // Re-touch key 2 (might or might not be resident; only proceed if so).
        if c.get(&2).is_some() {
            let _ = c.put(6, vec![0u8; 10]);
            assert!(
                c.get(&2).is_some(),
                "touched key 2 should survive the next sweep"
            );
        }
    }

    #[test]
    fn capacity_bytes_floor_is_one() {
        let c: WeightedCache<u32, Vec<u8>> =
            WeightedCache::with_capacity_bytes(0, |v: &Vec<u8>| v.len());
        assert_eq!(c.capacity_bytes(), 1);
    }
}
