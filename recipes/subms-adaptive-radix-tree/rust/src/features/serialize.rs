//! Binary write/read for an ART. Custom format - no serde. A versioned
//! header followed by a node-tagged pre-order stream. Round-trip
//! preserves every insertion.
//!
//! Format (all big-endian):
//!
//! ```text
//! magic:        b"ARTb"                 (4 bytes)
//! version:      u16                     (= 1)
//! reserved:     u16                     (= 0; future flags)
//! len:          u64                     (number of keyed entries)
//! node-stream:  pre-order, see below
//! ```
//!
//! Each node is one of:
//!
//! ```text
//! tag: u8
//!   0x00 -> empty leaf (no value, no children) - terminator
//!   0x01 -> has value, no children:    [value_len:u32][value:bytes]
//!   0x02 -> no value,  has children:   [child_count:u16][(byte:u8, node)...]
//!   0x03 -> has value, has children:   [value_len:u32][value:bytes]
//!                                      [child_count:u16][(byte:u8, node)...]
//! ```
//!
//! Values are serialised through the `ArtCodec` trait. Codecs ship for
//! `Vec<u8>`, `String`, `u32`, `u64`. Bring your own for other types.

use std::io::{self, Read, Write};

use crate::{Art, Children, Node};

const MAGIC: &[u8; 4] = b"ARTb";
const VERSION: u16 = 1;

const TAG_EMPTY: u8 = 0x00;
const TAG_VALUE: u8 = 0x01;
const TAG_CHILDREN: u8 = 0x02;
const TAG_BOTH: u8 = 0x03;

/// User-supplied value codec. Implement for any `V` you want to round-trip.
pub trait ArtCodec: Sized {
    fn write_value<W: Write>(&self, out: &mut W) -> io::Result<()>;
    fn read_value<R: Read>(input: &mut R, len: usize) -> io::Result<Self>;
}

impl ArtCodec for Vec<u8> {
    fn write_value<W: Write>(&self, out: &mut W) -> io::Result<()> {
        out.write_all(self)
    }
    fn read_value<R: Read>(input: &mut R, len: usize) -> io::Result<Self> {
        let mut buf = vec![0u8; len];
        input.read_exact(&mut buf)?;
        Ok(buf)
    }
}

impl ArtCodec for String {
    fn write_value<W: Write>(&self, out: &mut W) -> io::Result<()> {
        out.write_all(self.as_bytes())
    }
    fn read_value<R: Read>(input: &mut R, len: usize) -> io::Result<Self> {
        let mut buf = vec![0u8; len];
        input.read_exact(&mut buf)?;
        String::from_utf8(buf).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }
}

impl ArtCodec for u32 {
    fn write_value<W: Write>(&self, out: &mut W) -> io::Result<()> {
        out.write_all(&self.to_be_bytes())
    }
    fn read_value<R: Read>(input: &mut R, len: usize) -> io::Result<Self> {
        if len != 4 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "u32 value not 4 bytes",
            ));
        }
        let mut buf = [0u8; 4];
        input.read_exact(&mut buf)?;
        Ok(u32::from_be_bytes(buf))
    }
}

impl ArtCodec for u64 {
    fn write_value<W: Write>(&self, out: &mut W) -> io::Result<()> {
        out.write_all(&self.to_be_bytes())
    }
    fn read_value<R: Read>(input: &mut R, len: usize) -> io::Result<Self> {
        if len != 8 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "u64 value not 8 bytes",
            ));
        }
        let mut buf = [0u8; 8];
        input.read_exact(&mut buf)?;
        Ok(u64::from_be_bytes(buf))
    }
}

pub fn write_to<V: ArtCodec, W: Write>(tree: &Art<V>, out: &mut W) -> io::Result<()> {
    out.write_all(MAGIC)?;
    out.write_all(&VERSION.to_be_bytes())?;
    out.write_all(&0u16.to_be_bytes())?;
    out.write_all(&(tree.len() as u64).to_be_bytes())?;
    write_node(tree.root(), out)
}

pub fn parse<V: ArtCodec, R: Read>(input: &mut R) -> io::Result<Art<V>> {
    let mut header = [0u8; 4 + 2 + 2 + 8];
    input.read_exact(&mut header)?;
    if &header[0..4] != MAGIC {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "bad magic"));
    }
    let version = u16::from_be_bytes(header[4..6].try_into().unwrap());
    if version != VERSION {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("unsupported version {version}"),
        ));
    }
    let _reserved = u16::from_be_bytes(header[6..8].try_into().unwrap());
    let len = u64::from_be_bytes(header[8..16].try_into().unwrap()) as usize;

    let mut tree: Art<V> = Art::new();
    read_node(tree.root_mut(), input)?;
    tree.set_len(len);
    Ok(tree)
}

fn write_node<V: ArtCodec, W: Write>(node: &Node<V>, out: &mut W) -> io::Result<()> {
    let has_value = node.value.is_some();
    let child_pairs = collect_children(&node.children);
    let has_children = !child_pairs.is_empty();

    let tag = match (has_value, has_children) {
        (false, false) => TAG_EMPTY,
        (true, false) => TAG_VALUE,
        (false, true) => TAG_CHILDREN,
        (true, true) => TAG_BOTH,
    };
    out.write_all(&[tag])?;

    if has_value {
        let mut buf: Vec<u8> = Vec::new();
        node.value.as_ref().unwrap().write_value(&mut buf)?;
        out.write_all(&(buf.len() as u32).to_be_bytes())?;
        out.write_all(&buf)?;
    }

    if has_children {
        out.write_all(&(child_pairs.len() as u16).to_be_bytes())?;
        for (byte, child) in &child_pairs {
            out.write_all(&[*byte])?;
            write_node(child, out)?;
        }
    }

    Ok(())
}

fn read_node<V: ArtCodec, R: Read>(node: &mut Node<V>, input: &mut R) -> io::Result<()> {
    let mut tag_buf = [0u8; 1];
    input.read_exact(&mut tag_buf)?;
    let tag = tag_buf[0];
    if !matches!(tag, TAG_EMPTY | TAG_VALUE | TAG_CHILDREN | TAG_BOTH) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("bad tag {tag}"),
        ));
    }

    if tag == TAG_VALUE || tag == TAG_BOTH {
        let mut lbuf = [0u8; 4];
        input.read_exact(&mut lbuf)?;
        let vlen = u32::from_be_bytes(lbuf) as usize;
        node.value = Some(V::read_value(input, vlen)?);
    }

    if tag == TAG_CHILDREN || tag == TAG_BOTH {
        let mut cbuf = [0u8; 2];
        input.read_exact(&mut cbuf)?;
        let count = u16::from_be_bytes(cbuf) as usize;
        for _ in 0..count {
            let mut byte_buf = [0u8; 1];
            input.read_exact(&mut byte_buf)?;
            let byte = byte_buf[0];
            let child = node.children.get_or_insert_for_load(byte);
            read_node(child, input)?;
        }
    }

    Ok(())
}

fn collect_children<V>(children: &Children<V>) -> Vec<(u8, &Node<V>)> {
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
    // Deterministic byte-order for stable round-trips.
    out.sort_by_key(|(b, _)| *b);
    out
}

impl<V> Children<V> {
    fn get_or_insert_for_load(&mut self, byte: u8) -> &mut Node<V> {
        // Same shape as `get_or_insert` in lib.rs but kept private to
        // the serialize module so the base API stays unchanged. We
        // can't call lib.rs's private fn from a sibling module.
        let (exists, has_room) = match self {
            Children::Small { keys, count, .. } => {
                let found = keys.iter().take(*count as usize).any(|&k| k == byte);
                (found, (*count as usize) < 4)
            }
            Children::Full(_) => (true, true),
        };

        if !exists && !has_room {
            let prev = std::mem::replace(
                self,
                Children::Full(std::collections::HashMap::with_capacity(8)),
            );
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
                children[idx] = Some(Box::new(empty_node()));
                *count += 1;
                children[idx].as_deref_mut().unwrap()
            }
            Children::Full(map) => map.entry(byte).or_insert_with(|| Box::new(empty_node())),
        }
    }
}

fn empty_node<V>() -> Node<V> {
    Node {
        value: None,
        children: Children::Small {
            keys: [0u8; 4],
            children: [None, None, None, None],
            count: 0,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_empty_tree() {
        let t: Art<u32> = Art::new();
        let mut buf = Vec::new();
        write_to(&t, &mut buf).unwrap();
        let mut cursor = &buf[..];
        let restored: Art<u32> = parse(&mut cursor).unwrap();
        assert_eq!(restored.len(), 0);
        assert!(restored.is_empty());
    }

    #[test]
    fn round_trip_preserves_all_insertions() {
        let mut t: Art<u32> = Art::new();
        for i in 0..200u32 {
            let key = format!("key{i:04}");
            t.insert(key.as_bytes(), i);
        }
        let mut buf = Vec::new();
        write_to(&t, &mut buf).unwrap();
        let mut cursor = &buf[..];
        let restored: Art<u32> = parse(&mut cursor).unwrap();
        assert_eq!(restored.len(), 200);
        for i in 0..200u32 {
            let key = format!("key{i:04}");
            assert_eq!(restored.get(key.as_bytes()).copied(), Some(i), "key {key}");
        }
    }

    #[test]
    fn round_trip_with_node_growth() {
        // 256 distinct first bytes force the root to grow Small -> Full.
        let mut t: Art<u32> = Art::new();
        for i in 0..=255u8 {
            t.insert(&[i, 7, 9], i as u32);
        }
        let mut buf = Vec::new();
        write_to(&t, &mut buf).unwrap();
        let mut cursor = &buf[..];
        let restored: Art<u32> = parse(&mut cursor).unwrap();
        assert_eq!(restored.len(), 256);
        for i in 0..=255u8 {
            assert_eq!(restored.get(&[i, 7, 9]).copied(), Some(i as u32));
        }
    }

    #[test]
    fn round_trip_with_string_values() {
        let mut t: Art<String> = Art::new();
        t.insert(b"a", "alpha".to_string());
        t.insert(b"b", "beta".to_string());
        t.insert(b"long-key-with-many-bytes", "gamma".to_string());
        let mut buf = Vec::new();
        write_to(&t, &mut buf).unwrap();
        let mut cursor = &buf[..];
        let restored: Art<String> = parse(&mut cursor).unwrap();
        assert_eq!(restored.get(b"a").map(|s| s.as_str()), Some("alpha"));
        assert_eq!(restored.get(b"b").map(|s| s.as_str()), Some("beta"));
        assert_eq!(
            restored
                .get(b"long-key-with-many-bytes")
                .map(|s| s.as_str()),
            Some("gamma")
        );
    }

    #[test]
    fn round_trip_binary_keys_with_zeros() {
        let mut t: Art<u32> = Art::new();
        t.insert(&[0, 0, 0], 1);
        t.insert(&[0, 1, 2], 2);
        t.insert(b"", 99); // empty-key carries a value at the root
        let mut buf = Vec::new();
        write_to(&t, &mut buf).unwrap();
        let mut cursor = &buf[..];
        let restored: Art<u32> = parse(&mut cursor).unwrap();
        assert_eq!(restored.get(&[0, 0, 0]).copied(), Some(1));
        assert_eq!(restored.get(&[0, 1, 2]).copied(), Some(2));
        assert_eq!(restored.get(b"").copied(), Some(99));
        assert_eq!(restored.len(), 3);
    }

    #[test]
    fn bad_magic_is_rejected() {
        let bad = b"\x00\x00\x00\x00\x00\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00".to_vec();
        let mut cursor = &bad[..];
        match parse::<u32, _>(&mut cursor) {
            Err(e) => assert_eq!(e.kind(), io::ErrorKind::InvalidData),
            Ok(_) => panic!("parse should have rejected bad magic"),
        }
    }

    #[test]
    fn bad_version_is_rejected() {
        let mut buf = Vec::new();
        buf.extend_from_slice(MAGIC);
        buf.extend_from_slice(&999u16.to_be_bytes());
        buf.extend_from_slice(&0u16.to_be_bytes());
        buf.extend_from_slice(&0u64.to_be_bytes());
        let mut cursor = &buf[..];
        match parse::<u32, _>(&mut cursor) {
            Err(e) => assert_eq!(e.kind(), io::ErrorKind::InvalidData),
            Ok(_) => panic!("parse should have rejected bad version"),
        }
    }
}
