//! Binary write/read for an ART. Custom format - no serde. A versioned
//! header followed by a node-tagged pre-order stream. Round-trip
//! preserves every insertion.
//!
//! Format (all big-endian):
//!
//! ```text
//! magic:        b"ARTb"                 (4 bytes)
//! version:      u16                     (= 2)
//! reserved:     u16                     (= 0; future flags)
//! len:          u64                     (number of keyed entries)
//! node-stream:  pre-order, see below
//! ```
//!
//! Each node begins with its path-compressed prefix, then a shape tag:
//!
//! ```text
//! prefix_len: u16
//! prefix:     bytes
//! tag:        u8
//!   0x00 -> empty leaf (no value, no children) - terminator
//!   0x01 -> has value, no children:    [value_len:u32][value:bytes]
//!   0x02 -> no value,  has children:   [child_count:u16][(byte:u8, node)...]
//!   0x03 -> has value, has children:   [value_len:u32][value:bytes]
//!                                      [child_count:u16][(byte:u8, node)...]
//! ```
//!
//! Version 2 added the per-node `prefix` (path compression); version 1 streams
//! do not decode.
//!
//! Values are serialised through the `ArtCodec` trait. Codecs ship for
//! `Vec<u8>`, `String`, `u32`, `u64`. Bring your own for other types.

use std::io::{self, Read, Write};

use crate::{Art, Node};

const MAGIC: &[u8; 4] = b"ARTb";
const VERSION: u16 = 2;

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
    // Path-compressed prefix precedes the shape tag.
    out.write_all(&(node.prefix.len() as u16).to_be_bytes())?;
    out.write_all(&node.prefix)?;

    let has_value = node.value.is_some();
    let child_pairs = node.children.sorted_pairs();
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
    let mut plen_buf = [0u8; 2];
    input.read_exact(&mut plen_buf)?;
    let plen = u16::from_be_bytes(plen_buf) as usize;
    let mut prefix = vec![0u8; plen];
    input.read_exact(&mut prefix)?;
    node.prefix = prefix;

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

#[cfg(test)]
#[path = "serialize_tests.rs"]
mod tests;
