//! DEFLATE encoder (RFC 1951) emitting a single final block. The block is
//! either fixed-Huffman (BTYPE=01) when LZ77 finds enough redundancy to pay
//! for itself, or stored (BTYPE=00) for tiny / incompressible input.
//!
//! Match finding is a 32 KiB-window hash-chain over 3-byte prefixes, with
//! optional one-step lazy matching. We deliberately do NOT emit dynamic
//! Huffman blocks (BTYPE=10): fixed Huffman is a valid, self-describing block
//! type, and skipping the per-block code-length tree keeps the encoder small
//! and predictable. The cost is a few percent of ratio versus a production
//! zlib, which the writeup states plainly.

const MIN_MATCH: usize = 3;
const MAX_MATCH: usize = 258;
const WINDOW: usize = 32 * 1024;
const HASH_BITS: usize = 13;
const HASH_SIZE: usize = 1 << HASH_BITS;
const HASH_MASK: u32 = (HASH_SIZE as u32) - 1;
const NIL: i32 = -1;

/// Bit sink writing DEFLATE's LSB-first bit order into a byte buffer. A 64-bit
/// accumulator lets each symbol (<= ~32 bits across its parts) land without an
/// inner drain loop; bytes spill in a short fixed sequence.
struct BitWriter {
    out: Vec<u8>,
    bit_buf: u64,
    bit_cnt: u32,
}

impl BitWriter {
    fn new(cap: usize) -> Self {
        Self {
            out: Vec::with_capacity(cap),
            bit_buf: 0,
            bit_cnt: 0,
        }
    }

    /// Write the low `n` bits of `bits`, LSB first (`n <= 32`). Spills whole
    /// bytes whenever the accumulator holds at least 8.
    fn write_bits(&mut self, bits: u32, n: u32) {
        self.bit_buf |= (bits as u64) << self.bit_cnt;
        self.bit_cnt += n;
        while self.bit_cnt >= 8 {
            self.out.push((self.bit_buf & 0xFF) as u8);
            self.bit_buf >>= 8;
            self.bit_cnt -= 8;
        }
    }

    /// Emit an already-reversed Huffman code. Codes are defined MSB-first but
    /// land LSB-first in the stream, so callers pass the bit-reversed value.
    fn write_rev(&mut self, rev_code: u32, len: u32) {
        self.write_bits(rev_code, len);
    }

    fn align_to_byte(&mut self) {
        if self.bit_cnt > 0 {
            self.out.push((self.bit_buf & 0xFF) as u8);
            self.bit_buf = 0;
            self.bit_cnt = 0;
        }
    }

    fn finish(mut self) -> Vec<u8> {
        self.align_to_byte();
        self.out
    }
}

// Fixed-Huffman literal/length code lengths (RFC 1951 3.2.6):
// 0..=143 -> 8 bits, 144..=255 -> 9, 256..=279 -> 7, 280..=287 -> 8.
fn fixed_lit_code(sym: u32) -> (u32, u32) {
    if sym <= 143 {
        (0b0011_0000 + sym, 8)
    } else if sym <= 255 {
        (0b1_1001_0000 + (sym - 144), 9)
    } else if sym <= 279 {
        (sym - 256, 7)
    } else {
        (0b1100_0000 + (sym - 280), 8)
    }
}

fn reverse_bits(code: u32, len: u32) -> u32 {
    let mut rev = 0u32;
    for i in 0..len {
        rev |= ((code >> i) & 1) << (len - 1 - i);
    }
    rev
}

/// Precomputed (reversed-code, len) for every fixed-Huffman literal/length
/// symbol, so the hot literal path is a table lookup, not a per-symbol reverse.
fn fixed_lit_table() -> &'static [(u16, u8); 288] {
    use std::sync::OnceLock;
    static T: OnceLock<[(u16, u8); 288]> = OnceLock::new();
    T.get_or_init(|| {
        let mut t = [(0u16, 0u8); 288];
        for (sym, slot) in t.iter_mut().enumerate() {
            let (code, len) = fixed_lit_code(sym as u32);
            *slot = (reverse_bits(code, len) as u16, len as u8);
        }
        t
    })
}

// Length codes 257..=285: base length + extra-bit count (RFC 1951 3.2.5).
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

// O(1) length-code lookup: index by (len - 3), 0..=255 covers lengths 3..=258.
fn length_code_table() -> &'static [u8; 256] {
    use std::sync::OnceLock;
    static T: OnceLock<[u8; 256]> = OnceLock::new();
    T.get_or_init(|| {
        let mut t = [0u8; 256];
        for (len, slot) in t.iter_mut().enumerate() {
            let real_len = len + 3;
            let mut i = 28;
            while i > 0 && LEN_BASE[i] as usize > real_len {
                i -= 1;
            }
            *slot = i as u8;
        }
        t
    })
}

fn length_code(len: usize) -> usize {
    length_code_table()[len - 3] as usize
}

fn dist_code(dist: usize) -> usize {
    // Two-level: distances 1..=256 index directly; above that, the high half of
    // the distance picks the code. Cheap and avoids the 30-entry scan.
    let mut i = 29;
    while i > 0 && DIST_BASE[i] as usize > dist {
        i -= 1;
    }
    i
}

fn hash3(data: &[u8], i: usize) -> u32 {
    let h = (data[i] as u32) << 16 | (data[i + 1] as u32) << 8 | (data[i + 2] as u32);
    (h.wrapping_mul(2654435761) >> (32 - HASH_BITS)) & HASH_MASK
}

/// `level` controls match effort:
/// - 0: stored block only, no LZ77.
/// - 1..=3: greedy matching, max-chain probes scale with level.
/// - 4..=9: lazy matching on (defer to the longer of cur/next match).
pub fn deflate(data: &[u8], level: u32) -> Vec<u8> {
    if level == 0 || data.len() < MIN_MATCH {
        return stored_block(data);
    }
    let max_chain = match level {
        1 => 8,
        2 => 16,
        3 => 32,
        4 => 64,
        5 => 128,
        6 => 256,
        7 => 512,
        8 => 1024,
        _ => 2048,
    };
    let lazy = level >= 4;
    // Stop scanning the chain the moment a match this long turns up - the
    // marginal gain from a longer match rarely pays for the extra probes.
    let nice_match = match level {
        1 => 16,
        2 | 3 => 32,
        4 | 5 => 64,
        6 | 7 => 128,
        _ => MAX_MATCH,
    };

    let mut bw = BitWriter::new(data.len() / 2 + 16);
    bw.write_bits(1, 1); // BFINAL = 1 (single block)
    bw.write_bits(1, 2); // BTYPE = 01 (fixed Huffman)

    let cfg = LzConfig {
        lazy,
        max_chain,
        nice_match,
    };
    let mut head = vec![NIL; HASH_SIZE];
    // prev needs no zeroing: every slot is written (during insert) before read.
    let mut prev = vec![0i32; data.len()];
    lz77(data, &cfg, &mut head, &mut prev, &mut bw);

    let compressed = bw.finish();
    // If fixed-Huffman somehow lost (pathological incompressible input),
    // fall back to the smaller stored framing.
    let stored = stored_block(data);
    if stored.len() < compressed.len() {
        stored
    } else {
        compressed
    }
}

struct LzConfig {
    lazy: bool,
    max_chain: u32,
    nice_match: usize,
}

fn emit_literal(bw: &mut BitWriter, b: u8) {
    let (rev, len) = fixed_lit_table()[b as usize];
    bw.write_rev(rev as u32, len as u32);
}

fn emit_match(bw: &mut BitWriter, length: usize, dist: usize) {
    let lc = length_code(length);
    let (rev, clen) = fixed_lit_table()[257 + lc];
    bw.write_rev(rev as u32, clen as u32);
    let lex = LEN_EXTRA[lc] as u32;
    if lex > 0 {
        bw.write_bits((length - LEN_BASE[lc] as usize) as u32, lex);
    }
    let dc = dist_code(dist);
    // Distance codes are a fixed 5-bit reversed code in fixed Huffman.
    bw.write_rev(reverse_bits(dc as u32, 5), 5);
    let dex = DIST_EXTRA[dc] as u32;
    if dex > 0 {
        bw.write_bits((dist - DIST_BASE[dc] as usize) as u32, dex);
    }
}

#[inline]
fn insert(data: &[u8], i: usize, n: usize, head: &mut [i32], prev: &mut [i32]) {
    if i + MIN_MATCH <= n {
        let h = hash3(data, i) as usize;
        prev[i] = head[h];
        head[h] = i as i32;
    }
}

/// Find the longest match for position `i`, inserting `i` into the chain.
/// Returns (best_len, best_dist); best_len < MIN_MATCH means no usable match.
fn find_match(
    data: &[u8],
    i: usize,
    cfg: &LzConfig,
    head: &mut [i32],
    prev: &mut [i32],
) -> (usize, usize) {
    let h = hash3(data, i) as usize;
    let mut cand = head[h];
    let mut best_len = MIN_MATCH - 1;
    let mut best_dist = 0;
    let limit = i.saturating_sub(WINDOW);
    let max_len = (data.len() - i).min(MAX_MATCH);
    let mut chain = cfg.max_chain;
    while cand != NIL && (cand as usize) >= limit && chain > 0 {
        let c = cand as usize;
        if data[c + best_len] == data[i + best_len] {
            let mut l = 0;
            while l < max_len && data[c + l] == data[i + l] {
                l += 1;
            }
            if l > best_len {
                best_len = l;
                best_dist = i - c;
                if l >= max_len || l >= cfg.nice_match {
                    break;
                }
            }
        }
        cand = prev[c];
        chain -= 1;
    }
    prev[i] = head[h];
    head[h] = i as i32;
    if best_len >= MIN_MATCH {
        (best_len, best_dist)
    } else {
        (0, 0)
    }
}

fn lz77(data: &[u8], cfg: &LzConfig, head: &mut [i32], prev: &mut [i32], bw: &mut BitWriter) {
    let n = data.len();
    let mut i = 0usize;
    let mut prev_len = 0usize;
    let mut prev_dist = 0usize;
    let mut prev_avail = false;

    while i < n {
        let (cur_len, cur_dist) = if i + MIN_MATCH <= n {
            find_match(data, i, cfg, head, prev)
        } else {
            (0, 0)
        };

        if !cfg.lazy {
            if cur_len >= MIN_MATCH {
                emit_match(bw, cur_len, cur_dist);
                let end = i + cur_len;
                i += 1;
                while i < end {
                    insert(data, i, n, head, prev);
                    i += 1;
                }
            } else {
                emit_literal(bw, data[i]);
                i += 1;
            }
            continue;
        }

        // Lazy path: hold the current match, look one byte ahead, keep the
        // longer of the two.
        if prev_avail {
            if cur_len <= prev_len {
                emit_match(bw, prev_len, prev_dist);
                let end = (i - 1) + prev_len;
                i += 1;
                while i < end {
                    insert(data, i, n, head, prev);
                    i += 1;
                }
                prev_avail = false;
                prev_len = 0;
                continue;
            } else {
                emit_literal(bw, data[i - 1]);
            }
        }

        if cur_len >= MIN_MATCH {
            prev_len = cur_len;
            prev_dist = cur_dist;
            prev_avail = true;
            i += 1;
        } else {
            prev_avail = false;
            emit_literal(bw, data[i]);
            i += 1;
        }
    }

    if prev_avail {
        emit_match(bw, prev_len, prev_dist);
    }

    let (rev, len) = fixed_lit_table()[256];
    bw.write_rev(rev as u32, len as u32);
}

/// One or more stored (uncompressed) blocks, RFC 1951 3.2.4. Each block holds
/// at most 65535 bytes; the last carries BFINAL.
fn stored_block(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + 16);
    if data.is_empty() {
        // A single empty final stored block: BFINAL=1, BTYPE=00, then LEN=0.
        out.push(0x01);
        out.extend_from_slice(&[0, 0, 0xFF, 0xFF]);
        return out;
    }
    let mut off = 0;
    while off < data.len() {
        let chunk = (data.len() - off).min(0xFFFF);
        let final_block = off + chunk >= data.len();
        out.push(if final_block { 0x01 } else { 0x00 });
        let len = chunk as u16;
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&(!len).to_le_bytes());
        out.extend_from_slice(&data[off..off + chunk]);
        off += chunk;
    }
    out
}
