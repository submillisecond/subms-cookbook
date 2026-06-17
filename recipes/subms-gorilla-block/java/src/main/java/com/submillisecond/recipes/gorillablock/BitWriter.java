package com.submillisecond.recipes.gorillablock;

import java.util.Arrays;

/**
 * MSB-first bit writer. The Gorilla stream packs variable-width fields, so all
 * I/O goes through this. Bit order is most-significant-first within each byte,
 * matching the Rust port, so a block encoded in one language decodes
 * byte-for-byte in the other.
 */
final class BitWriter {
    private byte[] buf;
    private int len;
    private int cur;   // bits accumulated, not yet flushed
    private int nbits; // bits already filled in cur (0..8)

    BitWriter(int cap) {
        this.buf = new byte[Math.max(1, cap)];
        this.len = 0;
        this.cur = 0;
        this.nbits = 0;
    }

    void writeBit(int bit) {
        cur = (cur << 1) | (bit & 1);
        nbits++;
        if (nbits == 8) {
            push((byte) cur);
            cur = 0;
            nbits = 0;
        }
    }

    /** Write the low {@code n} bits of {@code value}, most-significant first. n in 0..=64. */
    void writeBits(long value, int n) {
        int i = n;
        while (i > 0) {
            i--;
            writeBit((int) ((value >>> i) & 1));
        }
    }

    /**
     * Bytes written so far, flushing the partial byte zero-padded. Does not
     * consume; the writer stays appendable so a block can serialize while still
     * accepting points.
     */
    byte[] snapshot() {
        int total = len + (nbits > 0 ? 1 : 0);
        byte[] out = Arrays.copyOf(buf, total);
        if (nbits > 0) {
            out[len] = (byte) (cur << (8 - nbits));
        }
        return out;
    }

    private void push(byte b) {
        if (len == buf.length) {
            buf = Arrays.copyOf(buf, buf.length * 2);
        }
        buf[len++] = b;
    }
}
