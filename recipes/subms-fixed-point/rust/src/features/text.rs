//! Decimal text without allocation: the wire boundary.

use crate::error::FixedError;
use crate::fixed::{DECIMALS, Fixed, SCALE};

/// Longest text `write_to` can produce: sign, 19 integer digits, point, 9 fractional digits.
pub const MAX_TEXT_LEN: usize = 1 + 19 + 1 + 9;

impl Fixed {
    /// Parse `[-|+]digits[.digits]`.
    pub fn parse(s: &str) -> Result<Fixed, FixedError> {
        Fixed::parse_bytes(s.as_bytes())
    }

    /// [`parse`](Fixed::parse) over raw bytes, for a wire buffer.
    pub fn parse_bytes(b: &[u8]) -> Result<Fixed, FixedError> {
        let (negative, rest) = match b.first() {
            Some(b'-') => (true, &b[1..]),
            Some(b'+') => (false, &b[1..]),
            _ => (false, b),
        };
        if rest.is_empty() {
            return Err(FixedError::InvalidText);
        }
        let mut int: i64 = 0;
        let mut frac: i64 = 0;
        let mut frac_digits: u32 = 0;
        let mut seen_point = false;
        let mut digits = 0usize;
        for &c in rest {
            match c {
                b'0'..=b'9' => {
                    digits += 1;
                    let d = (c - b'0') as i64;
                    if seen_point {
                        if frac_digits == DECIMALS {
                            return Err(FixedError::TooPrecise);
                        }
                        frac = frac * 10 + d;
                        frac_digits += 1;
                    } else {
                        int = int
                            .checked_mul(10)
                            .and_then(|v| v.checked_add(d))
                            .ok_or(FixedError::Overflow)?;
                    }
                }
                b'.' if !seen_point => seen_point = true,
                _ => return Err(FixedError::InvalidText),
            }
        }
        if digits == 0 {
            return Err(FixedError::InvalidText);
        }
        let mut pad = frac_digits;
        while pad < DECIMALS {
            frac *= 10;
            pad += 1;
        }
        let raw = int
            .checked_mul(SCALE)
            .and_then(|v| v.checked_add(frac))
            .ok_or(FixedError::Overflow)?;
        Ok(Fixed::from_raw(if negative { -raw } else { raw }))
    }

    /// Render into `buf` with exactly `decimals` fractional digits (at most nine; zero writes no point).
    pub fn write_to(self, buf: &mut [u8], decimals: u32) -> Result<usize, FixedError> {
        if decimals > DECIMALS {
            return Err(FixedError::TooPrecise);
        }
        let raw = self.raw();
        let int = (raw / SCALE).unsigned_abs();
        let mut frac = (raw % SCALE).unsigned_abs();
        for _ in decimals..DECIMALS {
            frac /= 10;
        }
        let mut tmp = [0u8; MAX_TEXT_LEN];
        let mut n = 0usize;
        if raw < 0 {
            tmp[n] = b'-';
            n += 1;
        }
        n += write_uint(&mut tmp[n..], int);
        if decimals > 0 {
            tmp[n] = b'.';
            n += 1;
            let start = n;
            n += decimals as usize;
            let mut v = frac;
            for slot in tmp[start..n].iter_mut().rev() {
                *slot = b'0' + (v % 10) as u8;
                v /= 10;
            }
        }
        if buf.len() < n {
            return Err(FixedError::BufferTooSmall);
        }
        buf[..n].copy_from_slice(&tmp[..n]);
        Ok(n)
    }

    /// `write_to` into a fresh `String`.
    pub fn to_string_decimals(self, decimals: u32) -> Result<String, FixedError> {
        let mut buf = [0u8; MAX_TEXT_LEN];
        let n = self.write_to(&mut buf, decimals)?;
        Ok(String::from_utf8_lossy(&buf[..n]).into_owned())
    }
}

impl std::str::FromStr for Fixed {
    type Err = FixedError;
    fn from_str(s: &str) -> Result<Fixed, FixedError> {
        Fixed::parse(s)
    }
}

fn write_uint(out: &mut [u8], mut v: u64) -> usize {
    if v == 0 {
        out[0] = b'0';
        return 1;
    }
    let mut n = 0usize;
    let mut digits = [0u8; 20];
    while v > 0 {
        digits[n] = b'0' + (v % 10) as u8;
        v /= 10;
        n += 1;
    }
    for i in 0..n {
        out[i] = digits[n - 1 - i];
    }
    n
}

#[cfg(test)]
#[path = "text_tests.rs"]
mod tests;
