//! MSB-first bit writer + reader. The Gorilla stream packs variable-width
//! fields, so all I/O goes through these. Bit order is most-significant-first
//! within each byte - the same order the Java port writes, so a block encoded
//! in one language decodes byte-for-byte in the other.

#[derive(Clone)]
pub(crate) struct BitWriter {
    buf: Vec<u8>,
    cur: u8,
    nbits: u8, // bits already filled in `cur` (0..8)
}

impl BitWriter {
    pub fn with_capacity(cap: usize) -> Self {
        Self {
            buf: Vec::with_capacity(cap),
            cur: 0,
            nbits: 0,
        }
    }

    pub fn write_bit(&mut self, bit: u32) {
        self.cur = (self.cur << 1) | (bit as u8 & 1);
        self.nbits += 1;
        if self.nbits == 8 {
            self.buf.push(self.cur);
            self.cur = 0;
            self.nbits = 0;
        }
    }

    /// Write the low `n` bits of `value`, most-significant first. `n` in 0..=64.
    pub fn write_bits(&mut self, value: u64, n: u32) {
        let mut i = n;
        while i > 0 {
            i -= 1;
            self.write_bit(((value >> i) & 1) as u32);
        }
    }

    /// Bytes written so far, flushing the partial byte zero-padded. Does not
    /// consume - the writer stays appendable, so a block can serialize while
    /// still accepting points.
    pub fn snapshot(&self) -> Vec<u8> {
        let mut out = self.buf.clone();
        if self.nbits > 0 {
            out.push(self.cur << (8 - self.nbits));
        }
        out
    }
}

pub(crate) struct BitReader<'a> {
    buf: &'a [u8],
    byte: usize,
    bit: u8, // next bit to read in current byte, 0 = MSB
}

impl<'a> BitReader<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Self {
            buf,
            byte: 0,
            bit: 0,
        }
    }

    pub fn read_bit(&mut self) -> Option<u32> {
        if self.byte >= self.buf.len() {
            return None;
        }
        let b = self.buf[self.byte];
        let v = (b >> (7 - self.bit)) & 1;
        self.bit += 1;
        if self.bit == 8 {
            self.bit = 0;
            self.byte += 1;
        }
        Some(v as u32)
    }

    pub fn read_bits(&mut self, n: u32) -> Option<u64> {
        let mut v: u64 = 0;
        for _ in 0..n {
            v = (v << 1) | self.read_bit()? as u64;
        }
        Some(v)
    }
}
