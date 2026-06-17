//! CRC-32 (IEEE 802.3, polynomial 0xEDB88320), the checksum gzip stores in its
//! trailer. Standard reflected-table implementation: the table is built once
//! the first time it is needed and shared via a `OnceLock`.

use std::sync::OnceLock;

const POLY: u32 = 0xEDB8_8320;

fn table() -> &'static [u32; 256] {
    static TABLE: OnceLock<[u32; 256]> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut t = [0u32; 256];
        let mut n = 0usize;
        while n < 256 {
            let mut c = n as u32;
            let mut k = 0;
            while k < 8 {
                c = if c & 1 != 0 { POLY ^ (c >> 1) } else { c >> 1 };
                k += 1;
            }
            t[n] = c;
            n += 1;
        }
        t
    })
}

/// CRC-32 of `data` over the whole buffer in one shot.
pub fn crc32(data: &[u8]) -> u32 {
    let t = table();
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc = t[((crc ^ b as u32) & 0xFF) as usize] ^ (crc >> 8);
    }
    crc ^ 0xFFFF_FFFF
}
