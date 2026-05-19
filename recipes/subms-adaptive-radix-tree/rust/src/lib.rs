//! Adaptive Radix Tree (ART) over byte-string keys.
//!
//! This cookbook take focuses on the load-bearing idea: child-node storage
//! adapts to fan-out. Small nodes (<= 4 children) hold a 4-slot array of
//! `(byte, child)` pairs and scan linearly. Once a node would need a 5th
//! child, it grows to a hash-map-backed full node. The Node16 and Node48
//! variants in the original paper are further memory/speed sweet spots
//! that the recipe omits; the fundamental adaptive principle stays.
//!
//! Path compression is also omitted - each level represents one byte of
//! depth. For typical workloads this means longer paths and more allocations
//! than a fully-tuned ART; in exchange the implementation fits in ~200 LOC
//! and stays correct under arbitrary keys.
//!
//! ```
//! use subms_adaptive_radix_tree::Art;
//! let mut t: Art<i32> = Art::new();
//! t.insert(b"alice", 1);
//! t.insert(b"bob", 2);
//! assert_eq!(t.get(b"alice").copied(), Some(1));
//! assert_eq!(t.get(b"missing"), None);
//! ```

use std::collections::HashMap;

pub struct Art<V> {
    root: Node<V>,
    len: usize,
}

struct Node<V> {
    value: Option<V>,
    children: Children<V>,
}

enum Children<V> {
    Small {
        keys: [u8; 4],
        children: [Option<Box<Node<V>>>; 4],
        count: u8,
    },
    Full(HashMap<u8, Box<Node<V>>>),
}

impl<V> Default for Art<V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<V> Art<V> {
    pub fn new() -> Self {
        Self {
            root: Node::new(),
            len: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Insert or replace. Returns the prior value if any.
    pub fn insert(&mut self, key: &[u8], value: V) -> Option<V> {
        let (prior, added) = insert_at(&mut self.root, key, value);
        if added {
            self.len += 1;
        }
        prior
    }

    pub fn get(&self, key: &[u8]) -> Option<&V> {
        let mut cur = &self.root;
        for &b in key {
            cur = cur.children.get(b)?;
        }
        cur.value.as_ref()
    }
}

impl<V> Node<V> {
    fn new() -> Self {
        Self {
            value: None,
            children: Children::Small {
                keys: [0u8; 4],
                children: [None, None, None, None],
                count: 0,
            },
        }
    }
}

fn insert_at<V>(node: &mut Node<V>, key: &[u8], value: V) -> (Option<V>, bool) {
    if key.is_empty() {
        let prior = node.value.replace(value);
        let added = prior.is_none();
        return (prior, added);
    }
    let b = key[0];
    let rest = &key[1..];
    let child = node.children.get_or_insert(b);
    insert_at(child, rest, value)
}

impl<V> Children<V> {
    fn get(&self, byte: u8) -> Option<&Node<V>> {
        match self {
            Children::Small {
                keys,
                children,
                count,
            } => {
                for i in 0..(*count as usize) {
                    if keys[i] == byte {
                        return children[i].as_deref();
                    }
                }
                None
            }
            Children::Full(map) => map.get(&byte).map(|b| b.as_ref()),
        }
    }

    fn get_or_insert(&mut self, byte: u8) -> &mut Node<V> {
        // Compute existence + room availability with only a short borrow.
        let (exists, has_room) = match self {
            Children::Small { keys, count, .. } => {
                let mut found = false;
                for i in 0..(*count as usize) {
                    if keys[i] == byte {
                        found = true;
                        break;
                    }
                }
                (found, (*count as usize) < 4)
            }
            Children::Full(_) => (true, true),
        };

        // Promote Small -> Full if we need a new slot and there is no room.
        if !exists && !has_room {
            let prev = std::mem::replace(self, Children::Full(HashMap::with_capacity(8)));
            if let Children::Small {
                keys, mut children, ..
            } = prev
            {
                if let Children::Full(map) = self {
                    for i in 0..4 {
                        if let Some(child) = children[i].take() {
                            map.insert(keys[i], child);
                        }
                    }
                }
            }
        }

        // Dispatch.
        match self {
            Children::Small {
                keys,
                children,
                count,
            } => {
                for i in 0..(*count as usize) {
                    if keys[i] == byte {
                        return children[i].as_deref_mut().unwrap();
                    }
                }
                let idx = *count as usize;
                keys[idx] = byte;
                children[idx] = Some(Box::new(Node::new()));
                *count += 1;
                children[idx].as_deref_mut().unwrap()
            }
            Children::Full(map) => map.entry(byte).or_insert_with(|| Box::new(Node::new())),
        }
    }
}

#[cfg(feature = "harness")]
pub mod recipe;
