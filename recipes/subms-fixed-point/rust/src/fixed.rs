use std::fmt;
use std::iter::Sum;
use std::ops::{Add, AddAssign, Neg, Sub, SubAssign};

use crate::error::FixedError;
use crate::rounding::Rounding;

/// Fractional digits every `Fixed` carries.
pub const DECIMALS: u32 = 9;
/// `10^DECIMALS`: the raw value of `1`.
pub const SCALE: i64 = 1_000_000_000;

const POW10: [i64; 19] = [
    1,
    10,
    100,
    1_000,
    10_000,
    100_000,
    1_000_000,
    10_000_000,
    100_000_000,
    1_000_000_000,
    10_000_000_000,
    100_000_000_000,
    1_000_000_000_000,
    10_000_000_000_000,
    100_000_000_000_000,
    1_000_000_000_000_000,
    10_000_000_000_000_000,
    100_000_000_000_000_000,
    1_000_000_000_000_000_000,
];

/// An exact decimal at nine fixed places, stored as a signed 64-bit integer.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Fixed(i64);

impl Fixed {
    pub const ZERO: Fixed = Fixed(0);
    pub const ONE: Fixed = Fixed(SCALE);
    pub const MAX: Fixed = Fixed(i64::MAX);
    pub const MIN: Fixed = Fixed(i64::MIN);

    /// Wrap a raw value already at nine decimals.
    #[inline]
    pub const fn from_raw(raw: i64) -> Fixed {
        Fixed(raw)
    }

    /// The underlying integer, at nine decimals.
    #[inline]
    pub const fn raw(self) -> i64 {
        self.0
    }

    /// A whole number.
    #[inline]
    pub const fn from_int(n: i64) -> Option<Fixed> {
        match n.checked_mul(SCALE) {
            Some(r) => Some(Fixed(r)),
            None => None,
        }
    }

    /// A whole part plus a count of billionths.
    #[inline]
    pub const fn from_parts(int: i64, nanos: u32) -> Option<Fixed> {
        if nanos as i64 >= SCALE {
            return None;
        }
        let Some(whole) = int.checked_mul(SCALE) else {
            return None;
        };
        let frac = if int < 0 {
            -(nanos as i64)
        } else {
            nanos as i64
        };
        match whole.checked_add(frac) {
            Some(r) => Some(Fixed(r)),
            None => None,
        }
    }

    /// Rescale an integer quoted at `decimals` places into nine.
    pub fn from_scaled(value: i64, decimals: u32, mode: Rounding) -> Option<Fixed> {
        if decimals > 18 {
            return None;
        }
        if decimals <= DECIMALS {
            let f = POW10[(DECIMALS - decimals) as usize];
            return value.checked_mul(f).map(Fixed);
        }
        let d = POW10[(decimals - DECIMALS) as usize] as i128;
        narrow(mode.div(value as i128, d))
    }

    /// The value as an integer at `decimals` places.
    pub fn to_scaled(self, decimals: u32, mode: Rounding) -> Option<i64> {
        if decimals > 18 {
            return None;
        }
        if decimals >= DECIMALS {
            let f = POW10[(decimals - DECIMALS) as usize];
            return self.0.checked_mul(f);
        }
        let d = POW10[(DECIMALS - decimals) as usize] as i128;
        i64::try_from(mode.div(self.0 as i128, d)).ok()
    }

    /// Convert a float at the boundary.
    pub fn from_f64(x: f64, mode: Rounding) -> Result<Fixed, FixedError> {
        if !x.is_finite() {
            return Err(FixedError::NotFinite);
        }
        // Round the exact rational m * 10^9 / 2^k as integers; x * 1e9 would round twice.
        let bits = x.to_bits();
        let negative = (bits >> 63) == 1;
        let exp_bits = ((bits >> 52) & 0x7ff) as i32;
        let frac_bits = bits & ((1u64 << 52) - 1);
        let (m, e) = if exp_bits == 0 {
            (frac_bits, -1074)
        } else {
            (frac_bits | (1u64 << 52), exp_bits - 1075)
        };
        if m == 0 {
            return Ok(Fixed::ZERO);
        }
        let n = (m as i128) * (SCALE as i128);
        let n = if negative { -n } else { n };
        let raw = if e >= 0 {
            if e > 40 {
                return Err(FixedError::Overflow);
            }
            n.checked_shl(e as u32).ok_or(FixedError::Overflow)?
        } else {
            // Past 2^120 only the remainder's sign matters, so the clamp changes no mode's answer.
            let k = (-e).min(120) as u32;
            mode.div(n, 1i128 << k)
        };
        narrow(raw).ok_or(FixedError::Overflow)
    }

    /// The value as a float.
    #[inline]
    pub fn to_f64(self) -> f64 {
        self.0 as f64 / SCALE as f64
    }

    /// The whole part, truncated toward zero.
    #[inline]
    pub const fn int_part(self) -> i64 {
        self.0 / SCALE
    }

    /// The fractional part in billionths, carrying the value's sign.
    #[inline]
    pub const fn frac_part(self) -> i64 {
        self.0 % SCALE
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
    pub const fn is_positive(self) -> bool {
        self.0 > 0
    }

    /// `-1`, `0` or `1`.
    #[inline]
    pub const fn signum(self) -> i64 {
        self.0.signum()
    }

    /// `None` only for `MIN`, whose magnitude has no positive twin.
    #[inline]
    pub const fn checked_abs(self) -> Option<Fixed> {
        match self.0.checked_abs() {
            Some(r) => Some(Fixed(r)),
            None => None,
        }
    }

    #[inline]
    pub const fn checked_neg(self) -> Option<Fixed> {
        match self.0.checked_neg() {
            Some(r) => Some(Fixed(r)),
            None => None,
        }
    }

    #[inline]
    pub const fn checked_add(self, rhs: Fixed) -> Option<Fixed> {
        match self.0.checked_add(rhs.0) {
            Some(r) => Some(Fixed(r)),
            None => None,
        }
    }

    #[inline]
    pub const fn checked_sub(self, rhs: Fixed) -> Option<Fixed> {
        match self.0.checked_sub(rhs.0) {
            Some(r) => Some(Fixed(r)),
            None => None,
        }
    }

    #[inline]
    pub const fn saturating_add(self, rhs: Fixed) -> Fixed {
        Fixed(self.0.saturating_add(rhs.0))
    }

    #[inline]
    pub const fn saturating_sub(self, rhs: Fixed) -> Fixed {
        Fixed(self.0.saturating_sub(rhs.0))
    }

    /// `self * rhs`, the 128-bit product rounded back to nine decimals once.
    #[inline]
    pub fn checked_mul(self, rhs: Fixed, mode: Rounding) -> Option<Fixed> {
        let p = (self.0 as i128) * (rhs.0 as i128);
        narrow(mode.div(p, SCALE as i128))
    }

    /// `self * n` for a plain integer count.
    #[inline]
    pub const fn checked_mul_int(self, n: i64) -> Option<Fixed> {
        match self.0.checked_mul(n) {
            Some(r) => Some(Fixed(r)),
            None => None,
        }
    }

    /// `self / rhs`, rounded once.
    #[inline]
    pub fn checked_div(self, rhs: Fixed, mode: Rounding) -> Option<Fixed> {
        if rhs.0 == 0 {
            return None;
        }
        let n = (self.0 as i128) * (SCALE as i128);
        narrow(mode.div(n, rhs.0 as i128))
    }

    /// `self / n` for a plain integer count, rounded once.
    #[inline]
    pub fn checked_div_int(self, n: i64, mode: Rounding) -> Option<Fixed> {
        if n == 0 {
            return None;
        }
        narrow(mode.div(self.0 as i128, n as i128))
    }

    /// Round to `decimals` places (at most nine) under `mode`.
    pub fn round(self, decimals: u32, mode: Rounding) -> Fixed {
        if decimals >= DECIMALS {
            return self;
        }
        let d = POW10[(DECIMALS - decimals) as usize] as i128;
        // The rounded quotient times d is at most |raw| + d, which fits.
        Fixed((mode.div(self.0 as i128, d) * d) as i64)
    }

    /// Snap onto a price grid: the nearest multiple of `tick` under `mode`.
    #[inline]
    pub fn round_to_tick(self, tick: Fixed, mode: Rounding) -> Option<Fixed> {
        if tick.0 <= 0 {
            return None;
        }
        let q = mode.div(self.0 as i128, tick.0 as i128);
        narrow(q * tick.0 as i128)
    }

    /// Whether the value is an exact multiple of `tick`.
    #[inline]
    pub const fn is_on_tick(self, tick: Fixed) -> bool {
        tick.0 > 0 && self.0 % tick.0 == 0
    }

    /// How many ticks the value is.
    #[inline]
    pub const fn ticks(self, tick: Fixed) -> Result<i64, FixedError> {
        if tick.0 <= 0 {
            return Err(FixedError::InvalidTick);
        }
        if self.0 % tick.0 != 0 {
            return Err(FixedError::OffTick);
        }
        Ok(self.0 / tick.0)
    }
}

#[inline]
fn narrow(v: i128) -> Option<Fixed> {
    i64::try_from(v).ok().map(Fixed)
}

impl Add for Fixed {
    type Output = Fixed;
    #[inline]
    fn add(self, rhs: Fixed) -> Fixed {
        self.checked_add(rhs).expect("fixed-point overflow in add")
    }
}

impl Sub for Fixed {
    type Output = Fixed;
    #[inline]
    fn sub(self, rhs: Fixed) -> Fixed {
        self.checked_sub(rhs).expect("fixed-point overflow in sub")
    }
}

impl Neg for Fixed {
    type Output = Fixed;
    #[inline]
    fn neg(self) -> Fixed {
        self.checked_neg().expect("fixed-point overflow in neg")
    }
}

impl AddAssign for Fixed {
    #[inline]
    fn add_assign(&mut self, rhs: Fixed) {
        *self = *self + rhs;
    }
}

impl SubAssign for Fixed {
    #[inline]
    fn sub_assign(&mut self, rhs: Fixed) {
        *self = *self - rhs;
    }
}

impl Sum for Fixed {
    fn sum<I: Iterator<Item = Fixed>>(iter: I) -> Fixed {
        iter.fold(Fixed::ZERO, |a, b| a + b)
    }
}

impl<'a> Sum<&'a Fixed> for Fixed {
    fn sum<I: Iterator<Item = &'a Fixed>>(iter: I) -> Fixed {
        iter.fold(Fixed::ZERO, |a, b| a + *b)
    }
}

impl fmt::Debug for Fixed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Fixed({self})")
    }
}

/// Shortest exact decimal: `100.5`, not `100.500000000`; `-0.000000001` keeps its sign.
impl fmt::Display for Fixed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let raw = self.0;
        let int = (raw / SCALE).unsigned_abs();
        let mut frac = (raw % SCALE).unsigned_abs();
        if raw < 0 {
            f.write_str("-")?;
        }
        write!(f, "{int}")?;
        if frac == 0 {
            return Ok(());
        }
        let mut digits = DECIMALS;
        while frac % 10 == 0 {
            frac /= 10;
            digits -= 1;
        }
        write!(f, ".{frac:0width$}", width = digits as usize)
    }
}

#[cfg(test)]
#[path = "fixed_tests.rs"]
mod tests;
