//! In-order scan between two byte-lex bounds.
//!
//! Ordering is byte-lexicographic. Two consequences worth naming up
//! front:
//!
//! - UTF-8 strings compare the same way as their UTF-8 byte sequences
//!   in this scheme. For ASCII-only keys the order matches what a
//!   reader would expect from a dictionary. Non-ASCII multi-byte
//!   sequences sort by their codepoint encoding, which is NOT a
//!   locale-aware collation - if you need collation, sort the result
//!   externally.
//! - Empty key is the minimum element.
//!
//! Bounds are independently inclusive / exclusive / unbounded.

use crate::{Art, Node};

#[derive(Clone)]
pub enum Bound<'a> {
    Included(&'a [u8]),
    Excluded(&'a [u8]),
    Unbounded,
}

/// In-order scan. Returns `(key, &value)` pairs sorted by key (byte-lex).
///
/// Allocates one `Vec` for the result and one for each visited path,
/// which is acceptable for cookbook clarity. A streaming iterator would
/// avoid the result Vec at the cost of significantly hairier lifetimes.
pub fn range<'a, V>(tree: &'a Art<V>, from: Bound<'a>, to: Bound<'a>) -> Vec<(Vec<u8>, &'a V)> {
    let mut out: Vec<(Vec<u8>, &'a V)> = Vec::new();
    let mut prefix: Vec<u8> = Vec::new();
    walk(tree.root(), &mut prefix, &from, &to, &mut out);
    out
}

fn walk<'a, V>(
    node: &'a Node<V>,
    prefix: &mut Vec<u8>,
    from: &Bound<'a>,
    to: &Bound<'a>,
    out: &mut Vec<(Vec<u8>, &'a V)>,
) {
    // Path compression: the node's prefix bytes precede its value + edges.
    prefix.extend_from_slice(&node.prefix);
    if let Some(v) = node.value.as_ref() {
        if in_bounds(prefix, from, to) {
            out.push((prefix.clone(), v));
        }
    }

    for (byte, child) in node.children.sorted_pairs() {
        prefix.push(byte);
        // Early-skip subtrees that can't contain a key in range. `prefix` is a
        // lower bound on every key below, so the prune stays sound with
        // compression (the child adds only more bytes).
        if !subtree_can_overlap(prefix, from, to) {
            prefix.pop();
            continue;
        }
        walk(child, prefix, from, to, out);
        prefix.pop();
    }
    prefix.truncate(prefix.len() - node.prefix.len());
}

fn in_bounds(key: &[u8], from: &Bound, to: &Bound) -> bool {
    let from_ok = match from {
        Bound::Unbounded => true,
        Bound::Included(b) => key >= *b,
        Bound::Excluded(b) => key > *b,
    };
    let to_ok = match to {
        Bound::Unbounded => true,
        Bound::Included(b) => key <= *b,
        Bound::Excluded(b) => key < *b,
    };
    from_ok && to_ok
}

/// Could the subtree rooted at `prefix` contain a key in `[from, to]`?
/// We can prune by the *upper* bound (no key in the subtree can be less
/// than `prefix`, but they can grow arbitrarily larger). For the lower
/// bound we can NOT prune via the prefix alone because a longer
/// descendant key may still satisfy `key >= from`. So we only check the
/// upper bound here.
fn subtree_can_overlap(prefix: &[u8], _from: &Bound, to: &Bound) -> bool {
    match to {
        Bound::Unbounded => true,
        Bound::Included(b) => prefix <= as_slice(b, prefix.len()),
        Bound::Excluded(b) => {
            let trimmed = as_slice(b, prefix.len());
            // For Excluded(b) the subtree is still in range as long as
            // prefix isn't already past b's truncation; we allow ==.
            prefix <= trimmed
        }
    }
}

fn as_slice(b: &[u8], len: usize) -> &[u8] {
    if b.len() >= len { &b[..len] } else { b }
}

#[cfg(test)]
#[path = "range_scan_tests.rs"]
mod tests;
