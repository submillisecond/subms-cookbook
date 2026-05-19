//! Immutable on-disk sorted run, slurped into memory on [`SsTable::open`].
//!
//! File layout (big-endian):
//! ```text
//! records: (key_len:u32 key:utf-8 flag:u8 value_len:u32 value:bytes)*
//! bloom:   bit_count:u32 k:u32 word_count:u32 (word:u64)*
//! footer:  records_end_offset:u64 magic:u32
//! flag := 0x00 (present) | 0x01 (tombstone, value_len == 0)
//! magic := 0x4C534D54 ("LSMT")
//! ```
//!
//! On open the whole file is read into a `Vec<u8>`, the bloom filter is
//! parsed out of the trailer, and reads operate entirely against memory.
//! A get short-circuits on a bloom miss.

use std::cmp::Ordering;
use std::fs;
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};

use subms_bloom_filter::BloomFilter;

const MAGIC: u32 = 0x4C534D54;
const FLAG_VALUE: u8 = 0x00;
const FLAG_TOMBSTONE: u8 = 0x01;
const FOOTER_BYTES: usize = 8 + 4;

pub(crate) struct SsTable {
    #[allow(dead_code)]
    path: PathBuf,
    buf: Vec<u8>,
    records_end: usize,
    bloom: BloomFilter,
}

impl SsTable {
    pub(crate) fn open(path: impl Into<PathBuf>) -> io::Result<Self> {
        let path = path.into();
        let buf = fs::read(&path)?;
        if buf.len() < FOOTER_BYTES {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "sstable too small"));
        }
        let magic_off = buf.len() - 4;
        let magic = u32::from_be_bytes(buf[magic_off..magic_off + 4].try_into().unwrap());
        if magic != MAGIC {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "bad sstable magic"));
        }
        let footer_off = buf.len() - FOOTER_BYTES;
        let records_end =
            u64::from_be_bytes(buf[footer_off..footer_off + 8].try_into().unwrap()) as usize;
        if records_end > footer_off {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "bad records_end offset"));
        }
        let bloom = BloomFilter::parse(&buf[records_end..footer_off])?;
        Ok(Self { path, buf, records_end, bloom })
    }

    pub(crate) fn write<'a, I>(
        path: impl Into<PathBuf>,
        expected_entries: usize,
        sorted_entries: I,
    ) -> io::Result<Self>
    where
        I: IntoIterator<Item = (&'a str, Option<&'a [u8]>)>,
    {
        let path = path.into();
        let mut bloom = BloomFilter::new(expected_entries);
        let mut records_end: u64 = 0;
        {
            let mut out = BufWriter::new(fs::File::create(&path)?);
            for (key, value) in sorted_entries {
                bloom.add(key);
                let kb = key.as_bytes();
                out.write_all(&(kb.len() as u32).to_be_bytes())?;
                out.write_all(kb)?;
                match value {
                    Some(v) => {
                        out.write_all(&[FLAG_VALUE])?;
                        out.write_all(&(v.len() as u32).to_be_bytes())?;
                        out.write_all(v)?;
                        records_end += (4 + kb.len() + 1 + 4 + v.len()) as u64;
                    }
                    None => {
                        out.write_all(&[FLAG_TOMBSTONE])?;
                        out.write_all(&0u32.to_be_bytes())?;
                        records_end += (4 + kb.len() + 1 + 4) as u64;
                    }
                }
            }
            bloom.write_to(&mut out)?;
            out.write_all(&records_end.to_be_bytes())?;
            out.write_all(&MAGIC.to_be_bytes())?;
            out.flush()?;
        }
        Self::open(path)
    }

    /// `None` - key not in this run.
    /// `Some(None)` - tombstoned in this run.
    /// `Some(Some(v))` - value present in this run.
    ///
    /// `check_bloom = false` skips the bloom probe and goes straight to the
    /// scan - used by [`BloomMode::Off`] to measure the optimisation's value.
    pub(crate) fn get(&self, key: &str, check_bloom: bool) -> Option<Option<Vec<u8>>> {
        if check_bloom && !self.bloom.might_contain(key) {
            return None;
        }
        let kb = key.as_bytes();
        let mut p = 0usize;
        while p < self.records_end {
            let key_len = u32::from_be_bytes(self.buf[p..p + 4].try_into().unwrap()) as usize;
            p += 4;
            let key_slice = &self.buf[p..p + key_len];
            let cmp = key_slice.cmp(kb);
            p += key_len;
            let flag = self.buf[p];
            p += 1;
            let value_len = u32::from_be_bytes(self.buf[p..p + 4].try_into().unwrap()) as usize;
            p += 4;
            match cmp {
                Ordering::Equal => {
                    return Some(if flag == FLAG_TOMBSTONE {
                        None
                    } else {
                        Some(self.buf[p..p + value_len].to_vec())
                    });
                }
                Ordering::Greater => return None,
                Ordering::Less => p += value_len,
            }
        }
        None
    }

    #[allow(dead_code)]
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}
