package com.submillisecond.recipes.tsgzip;

/**
 * CRC-32 (IEEE 802.3, polynomial 0xEDB88320), the checksum gzip stores in its
 * trailer. Hand-rolled rather than {@code java.util.zip.CRC32} so the gzip
 * layer is fully self-contained and mirrors the Rust port's
 * {@code crc32} module exactly.
 */
final class Crc32 {

    private static final int POLY = 0xEDB88320;
    private static final int[] TABLE = new int[256];

    static {
        for (int n = 0; n < 256; n++) {
            int c = n;
            for (int k = 0; k < 8; k++) {
                c = (c & 1) != 0 ? POLY ^ (c >>> 1) : c >>> 1;
            }
            TABLE[n] = c;
        }
    }

    private Crc32() {
    }

    /** CRC-32 of {@code data}, returned in the low 32 bits of a long. */
    static long crc32(byte[] data) {
        int crc = 0xFFFFFFFF;
        for (byte b : data) {
            crc = TABLE[(crc ^ b) & 0xFF] ^ (crc >>> 8);
        }
        return (crc ^ 0xFFFFFFFF) & 0xFFFFFFFFL;
    }
}
