package com.submillisecond.recipes.gorillablock;

/**
 * MSB-first bit reader, the decode counterpart to {@link BitWriter}. Returns
 * -1 from {@link #readBit()} past the end of the buffer; callers treat that as
 * a truncated stream.
 */
final class BitReader {
    private final byte[] buf;
    private int byteIdx;
    private int bit; // next bit to read in current byte, 0 = MSB

    BitReader(byte[] buf) {
        this.buf = buf;
        this.byteIdx = 0;
        this.bit = 0;
    }

    int readBit() {
        if (byteIdx >= buf.length) {
            return -1;
        }
        int b = buf[byteIdx] & 0xff;
        int v = (b >>> (7 - bit)) & 1;
        bit++;
        if (bit == 8) {
            bit = 0;
            byteIdx++;
        }
        return v;
    }

    /**
     * Read {@code n} bits MSB-first into the low bits of a long. Throws when
     * the stream runs short. A full 64-bit read may return a negative long;
     * callers treat the result as raw bits, not a signed quantity.
     */
    long readBits(int n) {
        long v = 0;
        for (int i = 0; i < n; i++) {
            int b = readBit();
            if (b < 0) {
                throw TsBlockException.truncated();
            }
            v = (v << 1) | b;
        }
        return v;
    }
}
