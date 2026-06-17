package com.submillisecond.recipes.tsgzip;

/**
 * DEFLATE decoder (RFC 1951). Handles all three block types - stored (00),
 * fixed Huffman (01), and dynamic Huffman (10) - so it decodes arbitrary
 * zlib/gzip output, not just what our own encoder emits. zlib emits dynamic
 * blocks by default, so the dynamic path is the one that proves real-world
 * interop. Hand-rolled (no {@code java.util.zip.Inflater}).
 */
final class Inflate {

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
    private static final int[] CL_ORDER = {
        16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15
    };

    private Inflate() {
    }

    private static final class BitReader {
        final byte[] data;
        final int end;
        int pos;
        long bitBuf;
        int bitCnt;

        BitReader(byte[] data, int off, int len) {
            this.data = data;
            this.pos = off;
            this.end = off + len;
        }

        int bits(int n) {
            while (bitCnt < n) {
                if (pos >= end) {
                    throw TsGzipException.inflate("truncated DEFLATE stream");
                }
                bitBuf |= ((long) (data[pos++] & 0xff)) << bitCnt;
                bitCnt += 8;
            }
            int v = (int) (bitBuf & ((1L << n) - 1));
            bitBuf >>>= n;
            bitCnt -= n;
            return v;
        }

        void alignToByte() {
            int drop = bitCnt % 8;
            bitBuf >>>= drop;
            bitCnt -= drop;
        }

        int readByteAligned() {
            if (bitCnt >= 8) {
                int b = (int) (bitBuf & 0xff);
                bitBuf >>>= 8;
                bitCnt -= 8;
                return b;
            }
            if (pos >= end) {
                throw TsGzipException.inflate("truncated DEFLATE stream");
            }
            return data[pos++] & 0xff;
        }
    }

    // Canonical Huffman decode table built from a list of code lengths.
    private static final class Huffman {
        final int[] counts = new int[16];
        final int[] symbols;

        Huffman(int[] lengths) {
            for (int l : lengths) {
                counts[l]++;
            }
            counts[0] = 0;
            int[] offsets = new int[16];
            int sum = 0;
            for (int len = 1; len < 16; len++) {
                offsets[len] = sum;
                sum += counts[len];
            }
            symbols = new int[lengths.length];
            for (int sym = 0; sym < lengths.length; sym++) {
                int l = lengths[sym];
                if (l != 0) {
                    symbols[offsets[l]++] = sym;
                }
            }
        }

        int decode(BitReader br) {
            int code = 0;
            int first = 0;
            int index = 0;
            for (int len = 1; len < 16; len++) {
                code |= br.bits(1);
                int count = counts[len];
                if (code - first < count) {
                    return symbols[index + (code - first)];
                }
                index += count;
                first += count;
                first <<= 1;
                code <<= 1;
            }
            throw TsGzipException.inflate("invalid Huffman code");
        }
    }

    private static Huffman[] fixedTables() {
        int[] lit = new int[288];
        for (int i = 0; i < 288; i++) {
            lit[i] = i <= 143 ? 8 : i <= 255 ? 9 : i <= 279 ? 7 : 8;
        }
        int[] dist = new int[30];
        java.util.Arrays.fill(dist, 5);
        return new Huffman[] {new Huffman(lit), new Huffman(dist)};
    }

    static byte[] inflate(byte[] data) {
        return inflate(data, 0, data.length);
    }

    static byte[] inflate(byte[] data, int off, int len) {
        BitReader br = new BitReader(data, off, len);
        // A growable byte[] (not ByteArrayOutputStream) so back-references can
        // read already-emitted bytes in place. Sized at ~4x the compressed body
        // to skip most doublings on the common DEFLATE ratio.
        Out o = new Out(Math.max(64, len * 4));
        boolean finalBlock;
        do {
            finalBlock = br.bits(1) == 1;
            int btype = br.bits(2);
            switch (btype) {
                case 0 -> inflateStored(br, o);
                case 1 -> {
                    Huffman[] t = fixedTables();
                    inflateHuffman(br, o, t[0], t[1]);
                }
                case 2 -> {
                    Huffman[] t = readDynamicTables(br);
                    inflateHuffman(br, o, t[0], t[1]);
                }
                default -> throw TsGzipException.inflate("reserved DEFLATE block type");
            }
        } while (!finalBlock);
        return o.toBytes();
    }

    // Growable byte sink with O(1) indexed reads for back-references.
    private static final class Out {
        byte[] buf;
        int len;

        Out(int cap) {
            this.buf = new byte[cap];
        }

        void push(int b) {
            if (len == buf.length) {
                buf = java.util.Arrays.copyOf(buf, buf.length * 2);
            }
            buf[len++] = (byte) b;
        }

        byte[] toBytes() {
            return java.util.Arrays.copyOf(buf, len);
        }
    }

    private static void inflateStored(BitReader br, Out out) {
        br.alignToByte();
        int lo = br.readByteAligned();
        int hi = br.readByteAligned();
        int len = lo | (hi << 8);
        int nlo = br.readByteAligned();
        int nhi = br.readByteAligned();
        int nlen = nlo | (nhi << 8);
        if (len != ((~nlen) & 0xffff)) {
            throw TsGzipException.inflate("stored-block length check failed");
        }
        for (int k = 0; k < len; k++) {
            out.push(br.readByteAligned());
        }
    }

    private static Huffman[] readDynamicTables(BitReader br) {
        int hlit = br.bits(5) + 257;
        int hdist = br.bits(5) + 1;
        int hclen = br.bits(4) + 4;

        int[] clLengths = new int[19];
        for (int k = 0; k < hclen; k++) {
            clLengths[CL_ORDER[k]] = br.bits(3);
        }
        Huffman clTree = new Huffman(clLengths);

        int total = hlit + hdist;
        int[] lengths = new int[total];
        int i = 0;
        while (i < total) {
            int sym = clTree.decode(br);
            if (sym <= 15) {
                lengths[i++] = sym;
            } else if (sym == 16) {
                if (i == 0) {
                    throw TsGzipException.inflate("invalid Huffman code");
                }
                int rep = br.bits(2) + 3;
                int prevLen = lengths[i - 1];
                for (int r = 0; r < rep; r++) {
                    if (i >= total) {
                        throw TsGzipException.inflate("invalid Huffman code");
                    }
                    lengths[i++] = prevLen;
                }
            } else if (sym == 17) {
                int rep = br.bits(3) + 3;
                for (int r = 0; r < rep; r++) {
                    if (i >= total) {
                        throw TsGzipException.inflate("invalid Huffman code");
                    }
                    lengths[i++] = 0;
                }
            } else if (sym == 18) {
                int rep = br.bits(7) + 11;
                for (int r = 0; r < rep; r++) {
                    if (i >= total) {
                        throw TsGzipException.inflate("invalid Huffman code");
                    }
                    lengths[i++] = 0;
                }
            } else {
                throw TsGzipException.inflate("invalid Huffman code");
            }
        }
        int[] lit = java.util.Arrays.copyOfRange(lengths, 0, hlit);
        int[] dist = java.util.Arrays.copyOfRange(lengths, hlit, total);
        return new Huffman[] {new Huffman(lit), new Huffman(dist)};
    }

    private static void inflateHuffman(BitReader br, Out out, Huffman lit, Huffman dist) {
        while (true) {
            int sym = lit.decode(br);
            if (sym < 256) {
                out.push(sym);
            } else if (sym == 256) {
                return;
            } else {
                int li = sym - 257;
                if (li >= LEN_BASE.length) {
                    throw TsGzipException.inflate("invalid Huffman code");
                }
                int len = LEN_BASE[li] + br.bits(LEN_EXTRA[li]);
                int dsym = dist.decode(br);
                if (dsym >= DIST_BASE.length) {
                    throw TsGzipException.inflate("back-reference distance out of range");
                }
                int distance = DIST_BASE[dsym] + br.bits(DIST_EXTRA[dsym]);
                if (distance > out.len) {
                    throw TsGzipException.inflate("back-reference distance out of range");
                }
                int start = out.len - distance;
                for (int k = 0; k < len; k++) {
                    out.push(out.buf[start + k]);
                }
            }
        }
    }
}
