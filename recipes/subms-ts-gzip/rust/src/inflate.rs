//! DEFLATE decoder (RFC 1951). Handles all three block types - stored (00),
//! fixed Huffman (01), and dynamic Huffman (10) - so it decodes arbitrary
//! zlib/gzip output, not just what our own encoder emits. zlib emits dynamic
//! blocks by default, so the dynamic path is the one that proves real-world
//! interop.

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InflateError {
    Truncated,
    BadBlockType,
    BadCode,
    BadLength,
    BadDistance,
}

impl std::fmt::Display for InflateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let m = match self {
            InflateError::Truncated => "truncated DEFLATE stream",
            InflateError::BadBlockType => "reserved DEFLATE block type",
            InflateError::BadCode => "invalid Huffman code",
            InflateError::BadLength => "stored-block length check failed",
            InflateError::BadDistance => "back-reference distance out of range",
        };
        write!(f, "{m}")
    }
}

const LEN_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const LEN_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];
// Order in which code-length code lengths appear in a dynamic header.
const CL_ORDER: [usize; 19] = [
    16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
];

struct BitReader<'a> {
    data: &'a [u8],
    pos: usize,
    bit_buf: u32,
    bit_cnt: u32,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            pos: 0,
            bit_buf: 0,
            bit_cnt: 0,
        }
    }

    fn bits(&mut self, n: u32) -> Result<u32, InflateError> {
        while self.bit_cnt < n {
            let byte = *self.data.get(self.pos).ok_or(InflateError::Truncated)?;
            self.pos += 1;
            self.bit_buf |= (byte as u32) << self.bit_cnt;
            self.bit_cnt += 8;
        }
        let v = self.bit_buf & ((1u32 << n) - 1);
        self.bit_buf >>= n;
        self.bit_cnt -= n;
        Ok(v)
    }

    fn align_to_byte(&mut self) {
        let drop = self.bit_cnt % 8;
        self.bit_buf >>= drop;
        self.bit_cnt -= drop;
    }

    fn read_byte_aligned(&mut self) -> Result<u8, InflateError> {
        debug_assert_eq!(self.bit_cnt % 8, 0);
        if self.bit_cnt >= 8 {
            let b = (self.bit_buf & 0xFF) as u8;
            self.bit_buf >>= 8;
            self.bit_cnt -= 8;
            Ok(b)
        } else {
            let b = *self.data.get(self.pos).ok_or(InflateError::Truncated)?;
            self.pos += 1;
            Ok(b)
        }
    }
}

/// Canonical Huffman decode table built from a list of code lengths. Decoding
/// walks one bit at a time over the per-length first-code ranges - simple and
/// correct; speed is fine at the data sizes this codec handles.
struct Huffman {
    counts: [u16; 16],
    symbols: Vec<u16>,
}

impl Huffman {
    fn build(lengths: &[u8]) -> Huffman {
        let mut counts = [0u16; 16];
        for &l in lengths {
            counts[l as usize] += 1;
        }
        counts[0] = 0;
        let mut offsets = [0u16; 16];
        let mut sum = 0u16;
        for len in 1..16 {
            offsets[len] = sum;
            sum += counts[len];
        }
        let mut symbols = vec![0u16; lengths.len()];
        for (sym, &l) in lengths.iter().enumerate() {
            if l != 0 {
                symbols[offsets[l as usize] as usize] = sym as u16;
                offsets[l as usize] += 1;
            }
        }
        Huffman { counts, symbols }
    }

    fn decode(&self, br: &mut BitReader) -> Result<u16, InflateError> {
        let mut code = 0i32;
        let mut first = 0i32;
        let mut index = 0i32;
        for len in 1..16 {
            code |= br.bits(1)? as i32;
            let count = self.counts[len] as i32;
            if code - first < count {
                return Ok(self.symbols[(index + (code - first)) as usize]);
            }
            index += count;
            first += count;
            first <<= 1;
            code <<= 1;
        }
        Err(InflateError::BadCode)
    }
}

fn fixed_tables() -> (Huffman, Huffman) {
    let mut lit = [0u8; 288];
    for (i, l) in lit.iter_mut().enumerate() {
        *l = if i <= 143 {
            8
        } else if i <= 255 {
            9
        } else if i <= 279 {
            7
        } else {
            8
        };
    }
    let dist = [5u8; 30];
    (Huffman::build(&lit), Huffman::build(&dist))
}

/// Inflate a raw DEFLATE stream into the original bytes.
pub fn inflate(data: &[u8]) -> Result<Vec<u8>, InflateError> {
    let mut br = BitReader::new(data);
    let mut out: Vec<u8> = Vec::with_capacity(data.len() * 4 + 64);
    loop {
        let bfinal = br.bits(1)?;
        let btype = br.bits(2)?;
        match btype {
            0 => inflate_stored(&mut br, &mut out)?,
            1 => {
                let (lit, dist) = fixed_tables();
                inflate_huffman(&mut br, &mut out, &lit, &dist)?;
            }
            2 => {
                let (lit, dist) = read_dynamic_tables(&mut br)?;
                inflate_huffman(&mut br, &mut out, &lit, &dist)?;
            }
            _ => return Err(InflateError::BadBlockType),
        }
        if bfinal == 1 {
            break;
        }
    }
    Ok(out)
}

fn inflate_stored(br: &mut BitReader, out: &mut Vec<u8>) -> Result<(), InflateError> {
    br.align_to_byte();
    let lo = br.read_byte_aligned()? as u16;
    let hi = br.read_byte_aligned()? as u16;
    let len = lo | (hi << 8);
    let nlo = br.read_byte_aligned()? as u16;
    let nhi = br.read_byte_aligned()? as u16;
    let nlen = nlo | (nhi << 8);
    if len != !nlen {
        return Err(InflateError::BadLength);
    }
    for _ in 0..len {
        out.push(br.read_byte_aligned()?);
    }
    Ok(())
}

fn read_dynamic_tables(br: &mut BitReader) -> Result<(Huffman, Huffman), InflateError> {
    let hlit = br.bits(5)? as usize + 257;
    let hdist = br.bits(5)? as usize + 1;
    let hclen = br.bits(4)? as usize + 4;

    let mut cl_lengths = [0u8; 19];
    for &slot in CL_ORDER.iter().take(hclen) {
        cl_lengths[slot] = br.bits(3)? as u8;
    }
    let cl_tree = Huffman::build(&cl_lengths);

    let total = hlit + hdist;
    let mut lengths = vec![0u8; total];
    let mut i = 0;
    while i < total {
        let sym = cl_tree.decode(br)?;
        match sym {
            0..=15 => {
                lengths[i] = sym as u8;
                i += 1;
            }
            16 => {
                if i == 0 {
                    return Err(InflateError::BadCode);
                }
                let rep = br.bits(2)? as usize + 3;
                let prev = lengths[i - 1];
                for _ in 0..rep {
                    if i >= total {
                        return Err(InflateError::BadCode);
                    }
                    lengths[i] = prev;
                    i += 1;
                }
            }
            17 => {
                let rep = br.bits(3)? as usize + 3;
                for _ in 0..rep {
                    if i >= total {
                        return Err(InflateError::BadCode);
                    }
                    lengths[i] = 0;
                    i += 1;
                }
            }
            18 => {
                let rep = br.bits(7)? as usize + 11;
                for _ in 0..rep {
                    if i >= total {
                        return Err(InflateError::BadCode);
                    }
                    lengths[i] = 0;
                    i += 1;
                }
            }
            _ => return Err(InflateError::BadCode),
        }
    }

    let lit = Huffman::build(&lengths[..hlit]);
    let dist = Huffman::build(&lengths[hlit..]);
    Ok((lit, dist))
}

fn inflate_huffman(
    br: &mut BitReader,
    out: &mut Vec<u8>,
    lit: &Huffman,
    dist: &Huffman,
) -> Result<(), InflateError> {
    loop {
        let sym = lit.decode(br)?;
        if sym < 256 {
            out.push(sym as u8);
        } else if sym == 256 {
            return Ok(());
        } else {
            let li = (sym - 257) as usize;
            if li >= LEN_BASE.len() {
                return Err(InflateError::BadCode);
            }
            let len = LEN_BASE[li] as usize + br.bits(LEN_EXTRA[li] as u32)? as usize;
            let dsym = dist.decode(br)? as usize;
            if dsym >= DIST_BASE.len() {
                return Err(InflateError::BadDistance);
            }
            let distance = DIST_BASE[dsym] as usize + br.bits(DIST_EXTRA[dsym] as u32)? as usize;
            if distance > out.len() {
                return Err(InflateError::BadDistance);
            }
            let start = out.len() - distance;
            for k in 0..len {
                let b = out[start + k];
                out.push(b);
            }
        }
    }
}
