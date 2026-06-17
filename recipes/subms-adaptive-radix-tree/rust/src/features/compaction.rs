//! Memory-recovery passes for an ART that has been through a bulk
//! delete. The base tree doesn't expose deletion in its public API, so
//! this module pairs both halves:
//!
//! - `delete(tree, key)` clears the value at `key`, leaving the path
//!   intact (the byte path may still be costly).
//! - `compact(tree)` walks the tree post-delete, shrinks any Full node
//!   whose occupancy fits into a Small (<= 4 children), and prunes
//!   subtrees that hold no remaining values.
//!
//! Idempotent: a second `compact()` does nothing visible.

use crate::{Art, Children, Node};

pub fn delete<V>(tree: &mut Art<V>, key: &[u8]) -> Option<V> {
    tree.delete_value(key)
}

/// Walk the tree, shrinking over-allocated nodes and removing empty
/// subtrees. Returns the number of node-shape changes (Full->Small) +
/// pruned subtrees, for tests and observability.
pub fn compact<V>(tree: &mut Art<V>) -> usize {
    let mut changes = 0usize;
    compact_node(tree.root_mut(), &mut changes);
    changes
}

fn compact_node<V>(node: &mut Node<V>, changes: &mut usize) {
    // Compact children depth-first so leaf-level pruning bubbles up.
    match &mut node.children {
        Children::Small {
            children, count, ..
        } => {
            for slot in children.iter_mut().take(*count as usize) {
                if let Some(c) = slot.as_deref_mut() {
                    compact_node(c, changes);
                }
            }
        }
        Children::Full(map) => {
            for child in map.values_mut() {
                compact_node(child, changes);
            }
        }
    }

    // After children are compacted, prune any empty subtrees here. An
    // empty subtree = no value + no children. The base delete only
    // clears values, so subtrees can become empty after bulk delete.
    let removed = prune_empty(&mut node.children);
    if removed > 0 {
        *changes += removed;
    }

    // Now shrink shape: Full -> Small if occupancy <= 4. Skip if there
    // are no occupants at all (Small with count 0 is already minimal).
    if maybe_shrink(&mut node.children) {
        *changes += 1;
    }
}

fn prune_empty<V>(children: &mut Children<V>) -> usize {
    let mut removed = 0;
    match children {
        Children::Small {
            keys,
            children,
            count,
        } => {
            let mut i = 0;
            while i < (*count as usize) {
                let drop_it = children[i]
                    .as_deref()
                    .map(|c| c.value.is_none() && child_count(&c.children) == 0)
                    .unwrap_or(true);
                if drop_it {
                    let last = (*count as usize) - 1;
                    keys.swap(i, last);
                    children.swap(i, last);
                    children[last] = None;
                    keys[last] = 0;
                    *count -= 1;
                    removed += 1;
                } else {
                    i += 1;
                }
            }
        }
        Children::Full(map) => {
            let drop_keys: Vec<u8> = map
                .iter()
                .filter_map(|(k, child)| {
                    if child.value.is_none() && child_count(&child.children) == 0 {
                        Some(*k)
                    } else {
                        None
                    }
                })
                .collect();
            for k in drop_keys {
                map.remove(&k);
                removed += 1;
            }
        }
    }
    removed
}

fn maybe_shrink<V>(children: &mut Children<V>) -> bool {
    let occupancy = child_count(children);
    if occupancy > 4 {
        return false;
    }
    // Already Small? Nothing to do.
    if let Children::Small { .. } = children {
        return false;
    }

    let prev = std::mem::replace(
        children,
        Children::Small {
            keys: [0u8; 4],
            children: [None, None, None, None],
            count: 0,
        },
    );

    if let Children::Full(map) = prev {
        // Sorted-byte order keeps the shrunk layout stable across runs.
        let mut pairs: Vec<(u8, Box<Node<V>>)> = map.into_iter().collect();
        pairs.sort_by_key(|(b, _)| *b);
        if let Children::Small {
            keys,
            children: arr,
            count,
        } = children
        {
            for (i, (b, child)) in pairs.into_iter().enumerate() {
                keys[i] = b;
                arr[i] = Some(child);
                *count = (i + 1) as u8;
            }
        }
        true
    } else {
        false
    }
}

fn child_count<V>(children: &Children<V>) -> usize {
    match children {
        Children::Small { count, .. } => *count as usize,
        Children::Full(map) => map.len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delete_returns_prior_value() {
        let mut t: Art<u32> = Art::new();
        t.insert(b"alpha", 1);
        t.insert(b"beta", 2);
        assert_eq!(delete(&mut t, b"alpha"), Some(1));
        assert_eq!(delete(&mut t, b"alpha"), None, "second delete is a no-op");
        assert_eq!(t.len(), 1);
        assert_eq!(t.get(b"beta").copied(), Some(2));
    }

    #[test]
    fn compact_shrinks_full_back_to_small() {
        let mut t: Art<u32> = Art::new();
        // Grow the root to Full (5+ distinct first bytes).
        for i in 0..10u8 {
            t.insert(&[i], i as u32);
        }
        // Delete down to 3 occupants.
        for i in 0..7u8 {
            delete(&mut t, &[i]);
        }
        // Pre-compact: root is Full, has 3 children each with values.
        let changes = compact(&mut t);
        assert!(changes >= 1, "expected at least one shape change");

        // Post-compact: surviving keys still resolvable.
        for i in 7..10u8 {
            assert_eq!(t.get(&[i]).copied(), Some(i as u32));
        }
        // The deleted keys no longer return.
        for i in 0..7u8 {
            assert!(t.get(&[i]).is_none());
        }
    }

    #[test]
    fn compact_is_idempotent() {
        let mut t: Art<u32> = Art::new();
        for i in 0..10u8 {
            t.insert(&[i], i as u32);
        }
        for i in 0..7u8 {
            delete(&mut t, &[i]);
        }
        let first = compact(&mut t);
        let second = compact(&mut t);
        assert!(first >= 1);
        assert_eq!(second, 0, "no further compaction; got {second} changes");
        for i in 7..10u8 {
            assert_eq!(t.get(&[i]).copied(), Some(i as u32));
        }
    }

    #[test]
    fn compact_prunes_empty_subtrees() {
        let mut t: Art<u32> = Art::new();
        // Insert a deep path then delete its terminal value. The
        // intermediate nodes have no value and no other children, so
        // compact() should prune them.
        t.insert(b"hello", 1);
        t.insert(b"world", 2);
        assert_eq!(delete(&mut t, b"hello"), Some(1));

        // Before compact the path "h-e-l-l-o" still exists (no values).
        let changes = compact(&mut t);
        assert!(changes > 0, "pruning should report changes");

        // World survived.
        assert_eq!(t.get(b"world").copied(), Some(2));
        // Hello path is gone.
        assert!(t.get(b"hello").is_none());

        // A second compact is a no-op.
        assert_eq!(compact(&mut t), 0);
    }

    #[test]
    fn compact_on_empty_tree_is_noop() {
        let mut t: Art<u32> = Art::new();
        let changes = compact(&mut t);
        assert_eq!(changes, 0);
        assert_eq!(t.len(), 0);
    }

    #[test]
    fn compact_keeps_full_when_occupancy_above_four() {
        let mut t: Art<u32> = Art::new();
        for i in 0..10u8 {
            t.insert(&[i], i as u32);
        }
        // Delete just two, leaving 8 occupants - still Full.
        delete(&mut t, &[0u8]);
        delete(&mut t, &[1u8]);
        compact(&mut t);
        // Surviving keys still queryable.
        for i in 2..10u8 {
            assert_eq!(t.get(&[i]).copied(), Some(i as u32));
        }
    }

    #[test]
    fn delete_of_missing_key_returns_none() {
        let mut t: Art<u32> = Art::new();
        t.insert(b"present", 1);
        assert!(delete(&mut t, b"absent").is_none());
        assert_eq!(t.len(), 1);
        assert!(delete(&mut t, b"nope_no_path").is_none());
    }
}
