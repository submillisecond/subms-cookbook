//! Memory-recovery passes for an ART that has been through a bulk
//! delete. The base tree doesn't expose deletion in its public API, so
//! this module pairs both halves:
//!
//! - `delete(tree, key)` clears the value at `key`, leaving the path
//!   intact (the byte path may still be costly).
//! - `compact(tree)` walks the tree post-delete: prunes value-less empty
//!   subtrees, collapses a value-less single-child node into its child
//!   (re-extending the path-compressed prefix), and demotes each node to the
//!   smallest adaptive layout its occupancy fits (Node256 -> 48 -> 16 -> 4).
//!
//! Idempotent: a second `compact()` does nothing visible.

use crate::{Art, Children, Node, NodeKind};

pub fn delete<V>(tree: &mut Art<V>, key: &[u8]) -> Option<V> {
    tree.delete_value(key)
}

/// Walk the tree, pruning empty subtrees, collapsing single-child chains, and
/// demoting over-sized nodes. Returns the number of prunes + merges + shape
/// changes, for tests and observability.
pub fn compact<V>(tree: &mut Art<V>) -> usize {
    let mut changes = 0usize;
    compact_node(tree.root_mut(), &mut changes);
    changes
}

fn compact_node<V>(node: &mut Node<V>, changes: &mut usize) {
    // Depth-first, so leaf-level pruning + merging settles before this node.
    node.children.each_child_mut(|c| compact_node(c, changes));

    // Drop children that hold no value and no descendants.
    *changes += prune_empty(&mut node.children);

    // Collapse a value-less single-child node into its child, re-extending the
    // compressed prefix - the delete-side counterpart to insert's split.
    if node.value.is_none() && node.children.len() == 1 && merge_single_child(node) {
        *changes += 1;
    }

    // Demote to the smallest layout the remaining occupancy fits.
    if maybe_shrink(&mut node.children) {
        *changes += 1;
    }
}

fn prune_empty<V>(children: &mut Children<V>) -> usize {
    let empties: Vec<u8> = children
        .sorted_pairs()
        .iter()
        .filter(|(_, c)| c.value.is_none() && c.children.is_empty())
        .map(|(b, _)| *b)
        .collect();
    for b in &empties {
        children.remove(*b);
    }
    empties.len()
}

fn merge_single_child<V>(node: &mut Node<V>) -> bool {
    let mut pairs = node.children.take_all();
    let (edge, child) = match pairs.pop() {
        Some(p) => p,
        None => return false,
    };
    let mut child = *child;
    // new prefix = node.prefix ++ edge ++ child.prefix
    let mut new_prefix = std::mem::take(&mut node.prefix);
    new_prefix.push(edge);
    new_prefix.extend_from_slice(&child.prefix);
    child.prefix = new_prefix;
    *node = child;
    true
}

fn maybe_shrink<V>(children: &mut Children<V>) -> bool {
    if rank(children.kind()) <= needed_rank(children.len()) {
        return false;
    }
    // Re-inserting into a fresh Node4 auto-grows to the minimal layout for the
    // occupancy, so the drained node lands at exactly the right size.
    for (b, c) in children.take_all() {
        children.insert(b, c);
    }
    true
}

fn rank(kind: NodeKind) -> u8 {
    match kind {
        NodeKind::Node4 => 0,
        NodeKind::Node16 => 1,
        NodeKind::Node48 => 2,
        NodeKind::Node256 => 3,
    }
}

fn needed_rank(occupancy: usize) -> u8 {
    match occupancy {
        0..=4 => 0,
        5..=16 => 1,
        17..=48 => 2,
        _ => 3,
    }
}

#[cfg(test)]
#[path = "compaction_tests.rs"]
mod tests;
