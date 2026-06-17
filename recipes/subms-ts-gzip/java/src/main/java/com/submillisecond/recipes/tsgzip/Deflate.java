package com.submillisecond.recipes.tsgzip;

import java.io.ByteArrayOutputStream;

/**
 * DEFLATE encoder (RFC 1951) emitting a single final block: fixed-Huffman
 * (BTYPE=01) when LZ77 finds enough redundancy, or stored (BTYPE=00) for tiny /
 * incompressible input. Match finding is a 32 KiB-window hash-chain over 3-byte
 * prefixes with optional one-step lazy matching.
 *
 * <p>We deliberately do NOT emit dynamic-Huffman blocks (BTYPE=10): fixed
 * Huffman is valid and self-describing, and skipping the per-block code-length
 * tree keeps the encoder small. The cost is a few percent of ratio versus a
 * production zlib. Hand-rolled (no {@code java.util.zip.Deflater}) so the
 * algorithm is the teaching point and matches the Rust port byte-for-byte at
 * the algorithm level (compressed output is not required to be byte-identical
 * across languages, only valid and gunzip-able).
 */
final class Deflate {

    private static final int MIN_MATCH = 3;
    private static final int MAX_MATCH = 258;
    private static final int WINDOW = 32 * 1024;
    private static final int HASH_BITS = 13;
    private static final int HASH_SIZE = 1 << HASH_BITS;
    private static final int HASH_MASK = HASH_SIZE - 1;
    private static final int NIL = -1;

    private static final int[] LEN_BASE = {
        3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115,
        131, 163, 195, 227, 258
    };
    private static final int[] LEN_EXTRA = {
        0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0
    };
    private static final int[] DIST_BASE = {
        1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
        2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577
    };
    private static final int[] DIST_EXTRA = {
        0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12,
        13, 13
    };

    // Precomputed (reversed-code, len) for every fixed-Huffman literal/length
    // symbol, so the hot literal path is a table lookup.
    private static final int[] LIT_REV = new int[288];
    private static final int[] LIT_LEN = new int[288];
    private static final int[] LEN_CODE = new int[256]; // index by (len - 3)

    static {
        for (int sym = 0; sym < 288; sym++) {
            int code;
            int len;
            if (sym <= 143) {
                code = 0b0011_0000 + sym;
                len = 8;
            } else if (sym <= 255) {
                code = 0b1_1001_0000 + (sym - 144);
                len = 9;
            } else if (sym <= 279) {
                code = sym - 256;
                len = 7;
            } else {
                code = 0b1100_0000 + (sym - 280);
                len = 8;
            }
            LIT_REV[sym] = reverseBits(code, len);
            LIT_LEN[sym] = len;
        }
        for (int len = 0; len < 256; len++) {
            int real = len + 3;
            int i = 28;
            while (i > 0 && LEN_BASE[i] > real) {
                i--;
            }
            LEN_CODE[len] = i;
        }
    }

    private Deflate() {
    }

    private static int reverseBits(int code, int len) {
        int rev = 0;
        for (int i = 0; i < len; i++) {
            rev |= ((code >> i) & 1) << (len - 1 - i);
        }
        return rev;
    }

    private static final class BitWriter {
        final ByteArrayOutputStream out;
        long bitBuf;
        int bitCnt;

        BitWriter(int cap) {
            this.out = new ByteArrayOutputStream(cap);
        }

        void writeBits(int bits, int n) {
            bitBuf |= ((long) bits & 0xffffffffL) << bitCnt;
            bitCnt += n;
            while (bitCnt >= 8) {
                out.write((int) (bitBuf & 0xff));
                bitBuf >>>= 8;
                bitCnt -= 8;
            }
        }

        void alignToByte() {
            if (bitCnt > 0) {
                out.write((int) (bitBuf & 0xff));
                bitBuf = 0;
                bitCnt = 0;
            }
        }

        byte[] finish() {
            alignToByte();
            return out.toByteArray();
        }
    }

    private static int lengthCode(int len) {
        return LEN_CODE[len - 3];
    }

    private static int distCode(int dist) {
        int i = 29;
        while (i > 0 && DIST_BASE[i] > dist) {
            i--;
        }
        return i;
    }

    private static int hash3(byte[] data, int i) {
        int h = ((data[i] & 0xff) << 16) | ((data[i + 1] & 0xff) << 8) | (data[i + 2] & 0xff);
        return (h * 0x9E3779B1) >>> (32 - HASH_BITS) & HASH_MASK;
    }

    /**
     * {@code level} controls match effort: 0 = stored only, 1..3 greedy LZ77,
     * 4..9 lazy matching with growing chain depth.
     */
    static byte[] deflate(byte[] data, int level) {
        if (level == 0 || data.length < MIN_MATCH) {
            return storedBlock(data);
        }
        int maxChain = switch (level) {
            case 1 -> 8;
            case 2 -> 16;
            case 3 -> 32;
            case 4 -> 64;
            case 5 -> 128;
            case 6 -> 256;
            case 7 -> 512;
            case 8 -> 1024;
            default -> 2048;
        };
        boolean lazy = level >= 4;
        int niceMatch = switch (level) {
            case 1 -> 16;
            case 2, 3 -> 32;
            case 4, 5 -> 64;
            case 6, 7 -> 128;
            default -> MAX_MATCH;
        };

        BitWriter bw = new BitWriter(data.length / 2 + 16);
        bw.writeBits(1, 1); // BFINAL = 1
        bw.writeBits(1, 2); // BTYPE = 01 (fixed Huffman)

        int[] head = new int[HASH_SIZE];
        java.util.Arrays.fill(head, NIL);
        // prev needs no init: each slot is written (insert) before any read.
        int[] prev = new int[data.length];

        lz77(data, maxChain, niceMatch, lazy, head, prev, bw);

        byte[] compressed = bw.finish();
        byte[] stored = storedBlock(data);
        return stored.length < compressed.length ? stored : compressed;
    }

    private static void emitLiteral(BitWriter bw, int b) {
        bw.writeBits(LIT_REV[b & 0xff], LIT_LEN[b & 0xff]);
    }

    private static void emitMatch(BitWriter bw, int length, int dist) {
        int lc = lengthCode(length);
        bw.writeBits(LIT_REV[257 + lc], LIT_LEN[257 + lc]);
        int lex = LEN_EXTRA[lc];
        if (lex > 0) {
            bw.writeBits(length - LEN_BASE[lc], lex);
        }
        int dc = distCode(dist);
        bw.writeBits(reverseBits(dc, 5), 5);
        int dex = DIST_EXTRA[dc];
        if (dex > 0) {
            bw.writeBits(dist - DIST_BASE[dc], dex);
        }
    }

    private static void insert(byte[] data, int i, int n, int[] head, int[] prev) {
        if (i + MIN_MATCH <= n) {
            int h = hash3(data, i);
            prev[i] = head[h];
            head[h] = i;
        }
    }

    // returns (bestLen << 32) | bestDist; bestLen < MIN_MATCH means no match.
    private static long findMatch(
            byte[] data, int i, int maxChain, int niceMatch, int[] head, int[] prev) {
        int h = hash3(data, i);
        int cand = head[h];
        int bestLen = MIN_MATCH - 1;
        int bestDist = 0;
        int limit = Math.max(0, i - WINDOW);
        int maxLen = Math.min(data.length - i, MAX_MATCH);
        int chain = maxChain;
        while (cand != NIL && cand >= limit && chain > 0) {
            int c = cand;
            if (data[c + bestLen] == data[i + bestLen]) {
                int l = 0;
                while (l < maxLen && data[c + l] == data[i + l]) {
                    l++;
                }
                if (l > bestLen) {
                    bestLen = l;
                    bestDist = i - c;
                    if (l >= maxLen || l >= niceMatch) {
                        break;
                    }
                }
            }
            cand = prev[c];
            chain--;
        }
        prev[i] = head[h];
        head[h] = i;
        if (bestLen >= MIN_MATCH) {
            return ((long) bestLen << 32) | (bestDist & 0xffffffffL);
        }
        return 0;
    }

    private static void lz77(
            byte[] data, int maxChain, int niceMatch, boolean lazy,
            int[] head, int[] prev, BitWriter bw) {
        int n = data.length;
        int i = 0;
        int prevLen = 0;
        int prevDist = 0;
        boolean prevAvail = false;

        while (i < n) {
            int curLen = 0;
            int curDist = 0;
            if (i + MIN_MATCH <= n) {
                long m = findMatch(data, i, maxChain, niceMatch, head, prev);
                curLen = (int) (m >>> 32);
                curDist = (int) m;
            }

            if (!lazy) {
                if (curLen >= MIN_MATCH) {
                    emitMatch(bw, curLen, curDist);
                    int end = i + curLen;
                    i++;
                    while (i < end) {
                        insert(data, i, n, head, prev);
                        i++;
                    }
                } else {
                    emitLiteral(bw, data[i]);
                    i++;
                }
                continue;
            }

            if (prevAvail) {
                if (curLen <= prevLen) {
                    emitMatch(bw, prevLen, prevDist);
                    int end = (i - 1) + prevLen;
                    i++;
                    while (i < end) {
                        insert(data, i, n, head, prev);
                        i++;
                    }
                    prevAvail = false;
                    prevLen = 0;
                    continue;
                } else {
                    emitLiteral(bw, data[i - 1]);
                }
            }

            if (curLen >= MIN_MATCH) {
                prevLen = curLen;
                prevDist = curDist;
                prevAvail = true;
                i++;
            } else {
                prevAvail = false;
                emitLiteral(bw, data[i]);
                i++;
            }
        }

        if (prevAvail) {
            emitMatch(bw, prevLen, prevDist);
        }
        bw.writeBits(LIT_REV[256], LIT_LEN[256]); // end-of-block
    }

    private static byte[] storedBlock(byte[] data) {
        ByteArrayOutputStream out = new ByteArrayOutputStream(data.length + 16);
        if (data.length == 0) {
            out.write(0x01);
            out.write(0);
            out.write(0);
            out.write(0xff);
            out.write(0xff);
            return out.toByteArray();
        }
        int off = 0;
        while (off < data.length) {
            int chunk = Math.min(data.length - off, 0xFFFF);
            boolean last = off + chunk >= data.length;
            out.write(last ? 0x01 : 0x00);
            out.write(chunk & 0xff);
            out.write((chunk >>> 8) & 0xff);
            int nlen = ~chunk & 0xffff;
            out.write(nlen & 0xff);
            out.write((nlen >>> 8) & 0xff);
            out.write(data, off, chunk);
            off += chunk;
        }
        return out.toByteArray();
    }
}
