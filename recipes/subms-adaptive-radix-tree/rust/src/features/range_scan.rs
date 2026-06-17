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

use crate::{Art, Children, Node};

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
    if let Some(v) = node.value.as_ref() {
        if in_bounds(prefix, from, to) {
            out.push((prefix.clone(), v));
        }
    }

    let pairs = sorted_children(&node.children);
    for (byte, child) in pairs {
        prefix.push(byte);
        // Early-skip subtrees that can't contain a key in range.
        if !subtree_can_overlap(prefix, from, to) {
            prefix.pop();
            continue;
        }
        walk(child, prefix, from, to, out);
        prefix.pop();
    }
}

fn sorted_children<V>(children: &Children<V>) -> Vec<(u8, &Node<V>)> {
    let mut out: Vec<(u8, &Node<V>)> = Vec::new();
    match children {
        Children::Small {
            keys,
            children,
            count,
        } => {
            for i in 0..(*count as usize) {
                if let Some(child) = children[i].as_deref() {
                    out.push((keys[i], child));
                }
            }
        }
        Children::Full(map) => {
            for (b, child) in map {
                out.push((*b, child.as_ref()));
            }
        }
    }
    out.sort_by_key(|(b, _)| *b);
    out
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
mod tests {
    use super::*;

    fn build(keys: &[&[u8]]) -> Art<u32> {
        let mut t = Art::new();
        for (i, k) in keys.iter().enumerate() {
            t.insert(k, i as u32);
        }
        t
    }

    #[test]
    fn empty_tree_yields_nothing() {
        let t: Art<u32> = Art::new();
        let out = range(&t, Bound::Unbounded, Bound::Unbounded);
        assert!(out.is_empty());
    }

    #[test]
    fn unbounded_scan_returns_all_keys_sorted() {
        let t = build(&[b"banana", b"apple", b"cherry", b"avocado"]);
        let out = range(&t, Bound::Unbounded, Bound::Unbounded);
        let keys: Vec<Vec<u8>> = out.into_iter().map(|(k, _)| k).collect();
        assert_eq!(
            keys,
            vec![
                b"apple".to_vec(),
                b"avocado".to_vec(),
                b"banana".to_vec(),
                b"cherry".to_vec(),
            ]
        );
    }

    #[test]
    fn inclusive_bounds_both_endpoints_match() {
        let t = build(&[b"a", b"b", b"c", b"d", b"e"]);
        let out = range(&t, Bound::Included(b"b"), Bound::Included(b"d"));
        let keys: Vec<Vec<u8>> = out.into_iter().map(|(k, _)| k).collect();
        assert_eq!(keys, vec![b"b".to_vec(), b"c".to_vec(), b"d".to_vec()]);
    }

    #[test]
    fn exclusive_bounds_drop_endpoints() {
        let t = build(&[b"a", b"b", b"c", b"d", b"e"]);
        let out = range(&t, Bound::Excluded(b"b"), Bound::Excluded(b"d"));
        let keys: Vec<Vec<u8>> = out.into_iter().map(|(k, _)| k).collect();
        assert_eq!(keys, vec![b"c".to_vec()]);
    }

    #[test]
    fn mixed_bounds() {
        let t = build(&[b"a", b"b", b"c", b"d", b"e"]);
        let out = range(&t, Bound::Included(b"b"), Bound::Excluded(b"d"));
        let keys: Vec<Vec<u8>> = out.into_iter().map(|(k, _)| k).collect();
        assert_eq!(keys, vec![b"b".to_vec(), b"c".to_vec()]);
    }

    #[test]
    fn unbounded_from_returns_prefix() {
        let t = build(&[b"a", b"b", b"c", b"d"]);
        let out = range(&t, Bound::Unbounded, Bound::Included(b"b"));
        let keys: Vec<Vec<u8>> = out.into_iter().map(|(k, _)| k).collect();
        assert_eq!(keys, vec![b"a".to_vec(), b"b".to_vec()]);
    }

    #[test]
    fn empty_key_is_minimum() {
        let mut t = build(&[b"a", b"b"]);
        t.insert(b"", 99);
        let out = range(&t, Bound::Unbounded, Bound::Unbounded);
        let keys: Vec<Vec<u8>> = out.into_iter().map(|(k, _)| k).collect();
        assert_eq!(keys, vec![Vec::<u8>::new(), b"a".to_vec(), b"b".to_vec()]);
    }

    #[test]
    fn deep_node_keys_returned_in_order() {
        // Force the root to grow Small -> Full and walks descend deep.
        let mut t = Art::new();
        for i in 0..=255u8 {
            t.insert(&[i, 0, i], i as u32);
        }
        let out = range(&t, Bound::Included(&[10u8]), Bound::Excluded(&[15u8]));
        let starts: Vec<u8> = out.iter().map(|(k, _)| k[0]).collect();
        assert_eq!(starts, vec![10, 11, 12, 13, 14]);
        // Strictly ascending.
        for w in starts.windows(2) {
            assert!(w[0] < w[1]);
        }
    }

    #[test]
    fn out_of_range_bounds_yield_empty() {
        let t = build(&[b"a", b"b", b"c"]);
        let out = range(&t, Bound::Included(b"x"), Bound::Included(b"z"));
        assert!(out.is_empty());
    }
}
