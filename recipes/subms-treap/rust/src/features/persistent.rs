//! Persistent (copy-on-write) treap.
//!
//! Every `insert` / `remove` returns a NEW `PersistentTreap` without
//! mutating the receiver. The mutation path is copied (`O(log N)`
//! nodes); every other subtree is shared via `Rc`, so memory stays
//! sub-linear in the number of held versions.
//!
//! Old versions remain fully queryable. This is the right shape for
//! undo stacks, time-travel debugging, and immutable-by-default
//! sequence builders.
//!
//! The base `Treap` in `lib.rs` is the arena-backed mutable variant.
//! These two share priority-generation conventions (same LCG) but
//! intentionally have different memory layouts: the arena base is
//! cache-dense; the persistent variant trades density for the
//! shared-subtree invariant `Rc` enforces.

use std::cmp::Ordering;
use std::rc::Rc;

type Link<K, V> = Option<Rc<Node<K, V>>>;

struct Node<K, V> {
    key: K,
    value: V,
    priority: u64,
    left: Link<K, V>,
    right: Link<K, V>,
}

pub struct PersistentTreap<K, V> {
    root: Link<K, V>,
    len: usize,
    rng_state: u64,
}

impl<K: Ord + Clone, V: Clone> PersistentTreap<K, V> {
    pub fn new(seed: u64) -> Self {
        Self {
            root: None,
            len: 0,
            rng_state: seed | 1,
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Return a NEW treap with `key -> value` inserted (or its value
    /// replaced). `self` is left untouched. Shared subtrees are
    /// reference-counted with the previous version.
    pub fn insert(&self, key: K, value: V) -> Self {
        let mut next_rng = self.rng_state;
        let priority = next_priority(&mut next_rng);
        let (new_root, replaced) = ins(&self.root, key, value, priority);
        let len = if replaced { self.len } else { self.len + 1 };
        Self {
            root: new_root,
            len,
            rng_state: next_rng,
        }
    }

    /// Return a NEW treap with `key` removed. If `key` is absent, the
    /// returned treap is structurally identical (root pointer cloned).
    pub fn remove(&self, key: &K) -> Self {
        let (new_root, removed) = rem(&self.root, key);
        let len = if removed { self.len - 1 } else { self.len };
        Self {
            root: new_root,
            len,
            rng_state: self.rng_state,
        }
    }

    pub fn get(&self, key: &K) -> Option<&V> {
        let mut cur = self.root.as_deref();
        while let Some(node) = cur {
            match key.cmp(&node.key) {
                Ordering::Less => cur = node.left.as_deref(),
                Ordering::Greater => cur = node.right.as_deref(),
                Ordering::Equal => return Some(&node.value),
            }
        }
        None
    }

    pub fn collect_in_order(&self) -> Vec<(K, V)> {
        let mut out = Vec::with_capacity(self.len);
        in_order(&self.root, &mut out);
        out
    }
}

impl<K, V> Clone for PersistentTreap<K, V> {
    fn clone(&self) -> Self {
        Self {
            root: self.root.clone(),
            len: self.len,
            rng_state: self.rng_state,
        }
    }
}

fn next_priority(state: &mut u64) -> u64 {
    *state = state
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    // SplitMix64 finalizer - decorrelate the priority from the key, which
    // is otherwise drawn from the same LCG family and sorts the tree into
    // a spine. Mirrors the base Treap::next_priority fix.
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
    z ^ (z >> 31)
}

fn ins<K: Ord + Clone, V: Clone>(
    link: &Link<K, V>,
    key: K,
    value: V,
    priority: u64,
) -> (Link<K, V>, bool) {
    match link {
        None => (
            Some(Rc::new(Node {
                key,
                value,
                priority,
                left: None,
                right: None,
            })),
            false,
        ),
        Some(node) => match key.cmp(&node.key) {
            Ordering::Equal => (
                Some(Rc::new(Node {
                    key,
                    value,
                    priority: node.priority,
                    left: node.left.clone(),
                    right: node.right.clone(),
                })),
                true,
            ),
            Ordering::Less => {
                let (new_left, replaced) = ins(&node.left, key, value, priority);
                let new_left_pri = new_left.as_ref().unwrap().priority;
                let rebuilt = Rc::new(Node {
                    key: node.key.clone(),
                    value: node.value.clone(),
                    priority: node.priority,
                    left: new_left,
                    right: node.right.clone(),
                });
                let rooted = if new_left_pri > node.priority {
                    rotate_right(&rebuilt)
                } else {
                    rebuilt
                };
                (Some(rooted), replaced)
            }
            Ordering::Greater => {
                let (new_right, replaced) = ins(&node.right, key, value, priority);
                let new_right_pri = new_right.as_ref().unwrap().priority;
                let rebuilt = Rc::new(Node {
                    key: node.key.clone(),
                    value: node.value.clone(),
                    priority: node.priority,
                    left: node.left.clone(),
                    right: new_right,
                });
                let rooted = if new_right_pri > node.priority {
                    rotate_left(&rebuilt)
                } else {
                    rebuilt
                };
                (Some(rooted), replaced)
            }
        },
    }
}

fn rem<K: Ord + Clone, V: Clone>(link: &Link<K, V>, key: &K) -> (Link<K, V>, bool) {
    match link {
        None => (None, false),
        Some(node) => match key.cmp(&node.key) {
            Ordering::Less => {
                let (new_left, removed) = rem(&node.left, key);
                let rebuilt = Rc::new(Node {
                    key: node.key.clone(),
                    value: node.value.clone(),
                    priority: node.priority,
                    left: new_left,
                    right: node.right.clone(),
                });
                (Some(rebuilt), removed)
            }
            Ordering::Greater => {
                let (new_right, removed) = rem(&node.right, key);
                let rebuilt = Rc::new(Node {
                    key: node.key.clone(),
                    value: node.value.clone(),
                    priority: node.priority,
                    left: node.left.clone(),
                    right: new_right,
                });
                (Some(rebuilt), removed)
            }
            Ordering::Equal => (merge_subtrees(&node.left, &node.right), true),
        },
    }
}

fn merge_subtrees<K: Clone, V: Clone>(left: &Link<K, V>, right: &Link<K, V>) -> Link<K, V> {
    match (left, right) {
        (None, r) => r.clone(),
        (l, None) => l.clone(),
        (Some(l), Some(r)) => {
            if l.priority > r.priority {
                let merged = merge_subtrees(&l.right, right);
                Some(Rc::new(Node {
                    key: l.key.clone(),
                    value: l.value.clone(),
                    priority: l.priority,
                    left: l.left.clone(),
                    right: merged,
                }))
            } else {
                let merged = merge_subtrees(left, &r.left);
                Some(Rc::new(Node {
                    key: r.key.clone(),
                    value: r.value.clone(),
                    priority: r.priority,
                    left: merged,
                    right: r.right.clone(),
                }))
            }
        }
    }
}

fn rotate_right<K: Clone, V: Clone>(node: &Rc<Node<K, V>>) -> Rc<Node<K, V>> {
    let left = node.left.as_ref().expect("rotate_right needs left child");
    let new_right = Rc::new(Node {
        key: node.key.clone(),
        value: node.value.clone(),
        priority: node.priority,
        left: left.right.clone(),
        right: node.right.clone(),
    });
    Rc::new(Node {
        key: left.key.clone(),
        value: left.value.clone(),
        priority: left.priority,
        left: left.left.clone(),
        right: Some(new_right),
    })
}

fn rotate_left<K: Clone, V: Clone>(node: &Rc<Node<K, V>>) -> Rc<Node<K, V>> {
    let right = node.right.as_ref().expect("rotate_left needs right child");
    let new_left = Rc::new(Node {
        key: node.key.clone(),
        value: node.value.clone(),
        priority: node.priority,
        left: node.left.clone(),
        right: right.left.clone(),
    });
    Rc::new(Node {
        key: right.key.clone(),
        value: right.value.clone(),
        priority: right.priority,
        left: Some(new_left),
        right: right.right.clone(),
    })
}

fn in_order<K: Clone, V: Clone>(link: &Link<K, V>, out: &mut Vec<(K, V)>) {
    if let Some(node) = link {
        in_order(&node.left, out);
        out.push((node.key.clone(), node.value.clone()));
        in_order(&node.right, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_persistent_state() {
        let t: PersistentTreap<i32, i32> = PersistentTreap::new(0);
        assert!(t.is_empty());
        assert_eq!(t.len(), 0);
        assert!(t.get(&1).is_none());
    }

    #[test]
    fn insert_returns_new_version_without_touching_old() {
        let v0: PersistentTreap<i32, &'static str> = PersistentTreap::new(7);
        let v1 = v0.insert(1, "one");
        assert_eq!(v0.len(), 0);
        assert_eq!(v1.len(), 1);
        assert!(v0.get(&1).is_none());
        assert_eq!(v1.get(&1).copied(), Some("one"));
    }

    #[test]
    fn version_chain_each_isolated() {
        let v0: PersistentTreap<i32, i32> = PersistentTreap::new(7);
        let v1 = v0.insert(1, 10);
        let v2 = v1.insert(2, 20);
        let v3 = v2.insert(3, 30);

        assert_eq!(v0.len(), 0);
        assert_eq!(v1.len(), 1);
        assert_eq!(v2.len(), 2);
        assert_eq!(v3.len(), 3);

        assert!(v1.get(&2).is_none());
        assert_eq!(v2.get(&2).copied(), Some(20));
        assert_eq!(v3.get(&3).copied(), Some(30));
        assert!(v1.get(&3).is_none());
    }

    #[test]
    fn remove_leaves_old_version_intact() {
        let v0: PersistentTreap<i32, &'static str> = PersistentTreap::new(7);
        let v1 = v0.insert(1, "one").insert(2, "two").insert(3, "three");
        let v2 = v1.remove(&2);
        assert_eq!(
            v1.get(&2).copied(),
            Some("two"),
            "v1 still has the removed key"
        );
        assert!(v2.get(&2).is_none());
        assert_eq!(v1.len(), 3);
        assert_eq!(v2.len(), 2);
    }

    #[test]
    fn insert_replaces_value_in_new_version_only() {
        let v0: PersistentTreap<i32, &'static str> = PersistentTreap::new(7);
        let v1 = v0.insert(1, "first");
        let v2 = v1.insert(1, "second");
        assert_eq!(v1.get(&1).copied(), Some("first"));
        assert_eq!(v2.get(&1).copied(), Some("second"));
        assert_eq!(v1.len(), 1);
        assert_eq!(v2.len(), 1);
    }

    #[test]
    fn in_order_yields_sorted_keys() {
        let mut t: PersistentTreap<i32, i32> = PersistentTreap::new(123);
        for k in [5, 1, 9, 3, 7, 2, 8] {
            t = t.insert(k, k * 10);
        }
        let keys: Vec<i32> = t.collect_in_order().into_iter().map(|(k, _)| k).collect();
        assert_eq!(keys, vec![1, 2, 3, 5, 7, 8, 9]);
    }

    #[test]
    fn remove_absent_key_returns_clone() {
        let v0: PersistentTreap<i32, i32> = PersistentTreap::new(0);
        let v1 = v0.insert(1, 1).insert(2, 2);
        let v2 = v1.remove(&999);
        assert_eq!(v1.len(), v2.len());
        assert_eq!(v2.get(&1).copied(), Some(1));
        assert_eq!(v2.get(&2).copied(), Some(2));
    }

    #[test]
    fn many_versions_stress() {
        let mut versions: Vec<PersistentTreap<i32, i32>> = vec![PersistentTreap::new(99)];
        for i in 0..200 {
            let next = versions.last().unwrap().insert(i, i * 2);
            versions.push(next);
        }
        // Every version v_i has exactly i entries 0..i.
        for (i, v) in versions.iter().enumerate() {
            assert_eq!(v.len(), i);
            for k in 0..i as i32 {
                assert_eq!(v.get(&k).copied(), Some(k * 2));
            }
        }
    }
}
