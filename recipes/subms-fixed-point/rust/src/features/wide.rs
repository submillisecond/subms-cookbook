//! `Fixed128`: the exact product and the exact sum.

use std::fmt;
use std::iter::Sum;

use crate::error::FixedError;
use crate::fixed::{Fixed, SCALE};
use crate::rounding::Rounding;

/// Fractional digits a `Fixed128` carries: the product scale.
pub const WIDE_DECIMALS: u32 = 18;
/// `10^18`.
pub const WIDE_SCALE: i128 = 1_000_000_000_000_000_000;

/// An exact decimal at eighteen places in 128 bits.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Fixed128(i128);

impl Fixed128 {
    pub const ZERO: Fixed128 = Fixed128(0);

    #[inline]
    pub const fn from_raw(raw: i128) -> Fixed128 {
        Fixed128(raw)
    }

    #[inline]
    pub const fn raw(self) -> i128 {
        self.0
    }

    /// Widen a `Fixed`.
    #[inline]
    pub const fn from_fixed(x: Fixed) -> Fixed128 {
        Fixed128(x.raw() as i128 * SCALE as i128)
    }

    /// `a * b` with no rounding at all.
    #[inline]
    pub const fn product(a: Fixed, b: Fixed) -> Fixed128 {
        Fixed128(a.raw() as i128 * b.raw() as i128)
    }

    #[inline]
    pub const fn checked_add(self, rhs: Fixed128) -> Option<Fixed128> {
        match self.0.checked_add(rhs.0) {
            Some(r) => Some(Fixed128(r)),
            None => None,
        }
    }

    #[inline]
    pub const fn checked_sub(self, rhs: Fixed128) -> Option<Fixed128> {
        match self.0.checked_sub(rhs.0) {
            Some(r) => Some(Fixed128(r)),
            None => None,
        }
    }

    /// Add a nine-decimal value without converting it first.
    #[inline]
    pub const fn checked_add_fixed(self, rhs: Fixed) -> Option<Fixed128> {
        self.checked_add(Fixed128::from_fixed(rhs))
    }

    /// Accumulate `a * b` exactly: the fill loop's one call.
    #[inline]
    pub const fn checked_add_product(self, a: Fixed, b: Fixed) -> Option<Fixed128> {
        self.checked_add(Fixed128::product(a, b))
    }

    #[inline]
    pub const fn checked_neg(self) -> Option<Fixed128> {
        match self.0.checked_neg() {
            Some(r) => Some(Fixed128(r)),
            None => None,
        }
    }

    /// Divide by an integer count: the exact average of `n` notionals.
    #[inline]
    pub fn checked_div_int(self, n: i64, mode: Rounding) -> Option<Fixed128> {
        if n == 0 {
            return None;
        }
        Some(Fixed128(mode.div(self.0, n as i128)))
    }

    /// Narrow to nine decimals, rounding the nine dropped digits once.
    #[inline]
    pub fn to_fixed(self, mode: Rounding) -> Result<Fixed, FixedError> {
        let q = mode.div(self.0, SCALE as i128);
        i64::try_from(q)
            .map(Fixed::from_raw)
            .map_err(|_| FixedError::Overflow)
    }

    #[inline]
    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }

    #[inline]
    pub const fn is_negative(self) -> bool {
        self.0 < 0
    }

    #[inline]
    pub const fn signum(self) -> i128 {
        self.0.signum()
    }
}

impl Sum<Fixed128> for Fixed128 {
    fn sum<I: Iterator<Item = Fixed128>>(iter: I) -> Fixed128 {
        iter.fold(Fixed128::ZERO, |a, b| {
            a.checked_add(b)
                .expect("fixed-point overflow in Fixed128 sum")
        })
    }
}

impl fmt::Debug for Fixed128 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Fixed128({self})")
    }
}

/// Shortest exact decimal, like `Fixed`.
impl fmt::Display for Fixed128 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let raw = self.0;
        let int = (raw / WIDE_SCALE).unsigned_abs();
        let mut frac = (raw % WIDE_SCALE).unsigned_abs();
        if raw < 0 {
            f.write_str("-")?;
        }
        write!(f, "{int}")?;
        if frac == 0 {
            return Ok(());
        }
        let mut digits = WIDE_DECIMALS;
        while frac % 10 == 0 {
            frac /= 10;
            digits -= 1;
        }
        write!(f, ".{frac:0width$}", width = digits as usize)
    }
}

#[cfg(test)]
#[path = "wide_tests.rs"]
mod tests;
