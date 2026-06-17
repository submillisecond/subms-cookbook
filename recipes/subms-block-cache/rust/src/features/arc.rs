//! Adaptive Replacement Cache (Megiddo + Modha, 2003).
//!
//! Four lists, total budget `c`:
//!
//!   T1 - recently-seen-once entries (LRU at the back).
//!   T2 - recently-seen-more-than-once entries (LRU at the back).
//!   B1 - ghost list of keys recently evicted from T1.
//!   B2 - ghost list of keys recently evicted from T2.
//!
//! `|T1| + |T2| <= c`. `|T1| + |B1| <= c`, `|T2| + |B2| <= 2c`.
//! The split between T1 and T2 is governed by `p` (target |T1| size),
//! which adapts on ghost-list hits: a B1 hit grows `p` (recency
//! signal); a B2 hit shrinks `p` (frequency signal). Scan-resistant
//! because a one-shot scan only lifts entries into T1 and then evicts
//! them to B1 without polluting T2.
//!
//! O(1) per access using a doubly-linked-list-by-index. We allocate a
//! Node pool keyed by a u32 slot id, and the four "lists" are just
//! head/tail pointers into that pool. Hashmap on `K -> (list_tag,
//! slot_id)` for membership lookup.

use std::collections::HashMap;
use std::hash::Hash;

/// Which list a slot currently lives on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum List {
    T1,
    T2,
    B1,
    B2,
}

struct Node<K, V> {
    key: K,
    /// Resident-only. Ghost-list entries hold `None`.
    value: Option<V>,
    prev: u32,
    next: u32,
    list: List,
}

const NIL: u32 = u32::MAX;

/// Adaptive replacement cache. `K: Hash + Eq + Clone`. `c` is the
/// resident budget; the ghost lists may hold up to another `c` keys
/// combined.
pub struct ArcCache<K, V> {
    c: usize,
    p: usize,
    nodes: Vec<Option<Node<K, V>>>,
    free: Vec<u32>,
    index: HashMap<K, u32>,
    // Per-list head + tail + length.
    t1_head: u32,
    t1_tail: u32,
    t1_len: usize,
    t2_head: u32,
    t2_tail: u32,
    t2_len: usize,
    b1_head: u32,
    b1_tail: u32,
    b1_len: usize,
    b2_head: u32,
    b2_tail: u32,
    b2_len: usize,
}

impl<K: Hash + Eq + Clone, V> ArcCache<K, V> {
    pub fn with_capacity(c: usize) -> Self {
        let c = c.max(1);
        Self {
            c,
            p: 0,
            nodes: Vec::new(),
            free: Vec::new(),
            index: HashMap::new(),
            t1_head: NIL,
            t1_tail: NIL,
            t1_len: 0,
            t2_head: NIL,
            t2_tail: NIL,
            t2_len: 0,
            b1_head: NIL,
            b1_tail: NIL,
            b1_len: 0,
            b2_head: NIL,
            b2_tail: NIL,
            b2_len: 0,
        }
    }

    pub fn capacity(&self) -> usize {
        self.c
    }
    pub fn len(&self) -> usize {
        self.t1_len + self.t2_len
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn p(&self) -> usize {
        self.p
    }
    pub fn t1_len(&self) -> usize {
        self.t1_len
    }
    pub fn t2_len(&self) -> usize {
        self.t2_len
    }
    pub fn b1_len(&self) -> usize {
        self.b1_len
    }
    pub fn b2_len(&self) -> usize {
        self.b2_len
    }

    /// Get + promote. A T1 hit moves to T2 (now seen twice). A T2 hit
    /// moves to T2's MRU end. Ghost-list keys are NOT counted as
    /// resident hits and return None.
    pub fn get(&mut self, key: &K) -> Option<&V> {
        let id = *self.index.get(key)?;
        match self.nodes[id as usize].as_ref().unwrap().list {
            List::T1 => {
                self.unlink(id);
                self.t1_len -= 1;
                self.nodes[id as usize].as_mut().unwrap().list = List::T2;
                self.push_front_t2(id);
                self.t2_len += 1;
            }
            List::T2 => {
                self.unlink(id);
                self.t2_len -= 1;
                self.push_front_t2(id);
                self.t2_len += 1;
            }
            // Ghost. Not a resident hit.
            List::B1 | List::B2 => return None,
        }
        self.nodes[id as usize].as_ref().unwrap().value.as_ref()
    }

    /// Insert or update. Implements the ARC replacement policy.
    pub fn put(&mut self, key: K, value: V) -> Option<(K, V)> {
        if let Some(&id) = self.index.get(&key) {
            match self.nodes[id as usize].as_ref().unwrap().list {
                List::T1 => {
                    // Promote to T2, update value.
                    self.unlink(id);
                    self.t1_len -= 1;
                    let node = self.nodes[id as usize].as_mut().unwrap();
                    node.value = Some(value);
                    node.list = List::T2;
                    self.push_front_t2(id);
                    self.t2_len += 1;
                    return None;
                }
                List::T2 => {
                    self.unlink(id);
                    let node = self.nodes[id as usize].as_mut().unwrap();
                    node.value = Some(value);
                    self.push_front_t2(id);
                    return None;
                }
                List::B1 => {
                    // Case II: B1 hit -> grow p, replace, move to T2.
                    let delta = (self.b2_len.max(1) / self.b1_len.max(1)).max(1);
                    self.p = (self.p + delta).min(self.c);
                    let evicted = self.replace(false);
                    self.unlink(id);
                    self.b1_len -= 1;
                    let node = self.nodes[id as usize].as_mut().unwrap();
                    node.value = Some(value);
                    node.list = List::T2;
                    self.push_front_t2(id);
                    self.t2_len += 1;
                    return evicted;
                }
                List::B2 => {
                    // Case III: B2 hit -> shrink p, replace, move to T2.
                    let delta = (self.b1_len.max(1) / self.b2_len.max(1)).max(1);
                    self.p = self.p.saturating_sub(delta);
                    let evicted = self.replace(true);
                    self.unlink(id);
                    self.b2_len -= 1;
                    let node = self.nodes[id as usize].as_mut().unwrap();
                    node.value = Some(value);
                    node.list = List::T2;
                    self.push_front_t2(id);
                    self.t2_len += 1;
                    return evicted;
                }
            }
        }

        // Case IV: brand-new key. Insert into T1 (head). Maybe evict.
        let l1 = self.t1_len + self.b1_len;
        let l2 = self.t2_len + self.b2_len;
        let mut evicted = None;
        if l1 == self.c {
            // |L1| == c
            if self.t1_len < self.c {
                // Drop LRU of B1; replace from resident.
                if let Some(victim) = self.pop_lru_b1() {
                    self.index.remove(&victim);
                }
                evicted = self.replace(false);
            } else {
                // |T1| == c, B1 empty - evict LRU of T1 outright (no ghost).
                let id = self.t1_tail;
                self.unlink(id);
                self.t1_len -= 1;
                let n = self.nodes[id as usize].take().unwrap();
                self.index.remove(&n.key);
                self.free.push(id);
                evicted = Some((n.key, n.value.unwrap()));
            }
        } else if l1 + l2 >= self.c {
            if l1 + l2 == 2 * self.c {
                if let Some(victim) = self.pop_lru_b2() {
                    self.index.remove(&victim);
                }
            }
            evicted = self.replace(false);
        }

        let id = self.alloc(Node {
            key: key.clone(),
            value: Some(value),
            prev: NIL,
            next: NIL,
            list: List::T1,
        });
        self.index.insert(key, id);
        self.push_front_t1(id);
        self.t1_len += 1;
        evicted
    }

    /// ARC's REPLACE(L,x) routine. `b2_hit` says "we just had a B2 hit"
    /// which biases eviction toward T1 even when |T1| == p.
    fn replace(&mut self, b2_hit: bool) -> Option<(K, V)> {
        let force_t1 = b2_hit && self.t1_len == self.p;
        if self.t1_len > 0 && (self.t1_len > self.p || force_t1) {
            // Evict LRU of T1 -> B1.
            let id = self.t1_tail;
            self.unlink(id);
            self.t1_len -= 1;
            let value = self.nodes[id as usize]
                .as_mut()
                .unwrap()
                .value
                .take()
                .unwrap();
            self.nodes[id as usize].as_mut().unwrap().list = List::B1;
            self.push_front_b1(id);
            self.b1_len += 1;
            let key = self.nodes[id as usize].as_ref().unwrap().key.clone();
            Some((key, value))
        } else if self.t2_len > 0 {
            let id = self.t2_tail;
            self.unlink(id);
            self.t2_len -= 1;
            let value = self.nodes[id as usize]
                .as_mut()
                .unwrap()
                .value
                .take()
                .unwrap();
            self.nodes[id as usize].as_mut().unwrap().list = List::B2;
            self.push_front_b2(id);
            self.b2_len += 1;
            let key = self.nodes[id as usize].as_ref().unwrap().key.clone();
            Some((key, value))
        } else {
            None
        }
    }

    fn alloc(&mut self, node: Node<K, V>) -> u32 {
        if let Some(id) = self.free.pop() {
            self.nodes[id as usize] = Some(node);
            id
        } else {
            let id = self.nodes.len() as u32;
            self.nodes.push(Some(node));
            id
        }
    }

    fn pop_lru_b1(&mut self) -> Option<K> {
        if self.b1_tail == NIL {
            return None;
        }
        let id = self.b1_tail;
        self.unlink(id);
        self.b1_len -= 1;
        let n = self.nodes[id as usize].take().unwrap();
        self.free.push(id);
        Some(n.key)
    }

    fn pop_lru_b2(&mut self) -> Option<K> {
        if self.b2_tail == NIL {
            return None;
        }
        let id = self.b2_tail;
        self.unlink(id);
        self.b2_len -= 1;
        let n = self.nodes[id as usize].take().unwrap();
        self.free.push(id);
        Some(n.key)
    }

    // Doubly-linked-list ops. Each list has its own head/tail; we use
    // the node's `list` tag to know which head/tail to mutate.
    fn unlink(&mut self, id: u32) {
        let (prev, next, list) = {
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
        match list {
            List::T1 => {
                if self.t1_head == id {
                    self.t1_head = next;
                }
                if self.t1_tail == id {
                    self.t1_tail = prev;
                }
            }
            List::T2 => {
                if self.t2_head == id {
                    self.t2_head = next;
                }
                if self.t2_tail == id {
                    self.t2_tail = prev;
                }
            }
            List::B1 => {
                if self.b1_head == id {
                    self.b1_head = next;
                }
                if self.b1_tail == id {
                    self.b1_tail = prev;
                }
            }
            List::B2 => {
                if self.b2_head == id {
                    self.b2_head = next;
                }
                if self.b2_tail == id {
                    self.b2_tail = prev;
                }
            }
        }
    }

    fn push_front_t1(&mut self, id: u32) {
        let old_head = self.t1_head;
        self.nodes[id as usize].as_mut().unwrap().next = old_head;
        self.nodes[id as usize].as_mut().unwrap().prev = NIL;
        if old_head != NIL {
            self.nodes[old_head as usize].as_mut().unwrap().prev = id;
        }
        self.t1_head = id;
        if self.t1_tail == NIL {
            self.t1_tail = id;
        }
    }
    fn push_front_t2(&mut self, id: u32) {
        let old_head = self.t2_head;
        self.nodes[id as usize].as_mut().unwrap().next = old_head;
        self.nodes[id as usize].as_mut().unwrap().prev = NIL;
        if old_head != NIL {
            self.nodes[old_head as usize].as_mut().unwrap().prev = id;
        }
        self.t2_head = id;
        if self.t2_tail == NIL {
            self.t2_tail = id;
        }
    }
    fn push_front_b1(&mut self, id: u32) {
        let old_head = self.b1_head;
        self.nodes[id as usize].as_mut().unwrap().next = old_head;
        self.nodes[id as usize].as_mut().unwrap().prev = NIL;
        if old_head != NIL {
            self.nodes[old_head as usize].as_mut().unwrap().prev = id;
        }
        self.b1_head = id;
        if self.b1_tail == NIL {
            self.b1_tail = id;
        }
    }
    fn push_front_b2(&mut self, id: u32) {
        let old_head = self.b2_head;
        self.nodes[id as usize].as_mut().unwrap().next = old_head;
        self.nodes[id as usize].as_mut().unwrap().prev = NIL;
        if old_head != NIL {
            self.nodes[old_head as usize].as_mut().unwrap().prev = id;
        }
        self.b2_head = id;
        if self.b2_tail == NIL {
            self.b2_tail = id;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn put_then_get_returns_value() {
        let mut c: ArcCache<u32, u32> = ArcCache::with_capacity(4);
        c.put(1, 10);
        c.put(2, 20);
        assert_eq!(c.get(&1).copied(), Some(10));
        assert_eq!(c.get(&2).copied(), Some(20));
        assert_eq!(c.get(&99), None);
    }

    #[test]
    fn second_hit_promotes_into_t2() {
        let mut c: ArcCache<u32, u32> = ArcCache::with_capacity(4);
        c.put(1, 10);
        assert_eq!(c.t1_len(), 1);
        assert_eq!(c.t2_len(), 0);
        // Get promotes into T2.
        c.get(&1);
        assert_eq!(c.t1_len(), 0);
        assert_eq!(c.t2_len(), 1);
    }

    #[test]
    fn capacity_one_evicts_on_every_new_key() {
        let mut c: ArcCache<u32, u32> = ArcCache::with_capacity(1);
        assert!(c.put(1, 1).is_none());
        let ev = c.put(2, 2);
        assert!(ev.is_some());
        assert_eq!(c.len(), 1);
        assert!(c.get(&1).is_none());
        assert_eq!(c.get(&2).copied(), Some(2));
    }

    #[test]
    fn scan_resistance_preserves_t2() {
        // Build a frequent set in T2, then run a scan of non-overlapping keys.
        // The scan should pollute T1/B1 but leave T2 alone.
        let mut c: ArcCache<u32, u32> = ArcCache::with_capacity(8);
        for k in 0u32..4 {
            c.put(k, k);
            c.get(&k); // promote into T2
        }
        assert_eq!(c.t2_len(), 4);
        // Scan: 100 fresh keys. Each goes into T1 and is then evicted to B1.
        for k in 1000u32..1100 {
            c.put(k, k);
        }
        // T2 frequent entries should still be there.
        for k in 0u32..4 {
            assert!(c.get(&k).is_some(), "frequent key {k} was evicted by scan");
        }
    }

    #[test]
    fn ghost_hit_adapts_p() {
        let mut c: ArcCache<u32, u32> = ArcCache::with_capacity(4);
        for k in 0u32..8 {
            c.put(k, k);
        }
        // Some early keys are now in B1. Touch one of them; this is a B1
        // hit that should bump `p`.
        let p_before = c.p();
        c.put(0, 100);
        assert!(c.p() >= p_before);
    }

    #[test]
    fn update_in_place_does_not_evict() {
        let mut c: ArcCache<u32, u32> = ArcCache::with_capacity(2);
        c.put(1, 10);
        c.put(2, 20);
        let ev = c.put(1, 11);
        assert!(ev.is_none(), "update of existing T1 key should not evict");
        assert_eq!(c.get(&1).copied(), Some(11));
    }

    #[test]
    fn many_inserts_keeps_resident_at_or_below_c() {
        let mut c: ArcCache<u32, u32> = ArcCache::with_capacity(16);
        for k in 0u32..1000 {
            c.put(k, k);
            assert!(c.len() <= 16, "resident set exceeded c at k={k}");
        }
    }
}
