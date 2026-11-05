//! A currency-tagged amount.

use std::fmt;

use crate::error::FixedError;
use crate::fixed::Fixed;
use crate::rounding::Rounding;

/// A three-letter ISO 4217 code packed into a `u32`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Currency(u32);

impl Currency {
    pub const USD: Currency = Currency::from_ascii(*b"USD");
    pub const EUR: Currency = Currency::from_ascii(*b"EUR");
    pub const GBP: Currency = Currency::from_ascii(*b"GBP");
    pub const JPY: Currency = Currency::from_ascii(*b"JPY");
    pub const CHF: Currency = Currency::from_ascii(*b"CHF");
    pub const AUD: Currency = Currency::from_ascii(*b"AUD");
    pub const CAD: Currency = Currency::from_ascii(*b"CAD");
    pub const HKD: Currency = Currency::from_ascii(*b"HKD");
    pub const SGD: Currency = Currency::from_ascii(*b"SGD");
    pub const CNH: Currency = Currency::from_ascii(*b"CNH");
    pub const KRW: Currency = Currency::from_ascii(*b"KRW");
    pub const BHD: Currency = Currency::from_ascii(*b"BHD");
    pub const KWD: Currency = Currency::from_ascii(*b"KWD");

    const fn from_ascii(code: [u8; 3]) -> Currency {
        Currency(((code[0] as u32) << 16) | ((code[1] as u32) << 8) | code[2] as u32)
    }

    /// Validate and upper-case a three-letter code.
    pub fn new(code: &str) -> Result<Currency, FixedError> {
        let b = code.as_bytes();
        if b.len() != 3 || !b.iter().all(u8::is_ascii_alphabetic) {
            return Err(FixedError::InvalidCurrency);
        }
        Ok(Currency::from_ascii([
            b[0].to_ascii_uppercase(),
            b[1].to_ascii_uppercase(),
            b[2].to_ascii_uppercase(),
        ]))
    }

    /// The three ASCII letters.
    pub const fn code(self) -> [u8; 3] {
        [(self.0 >> 16) as u8, (self.0 >> 8) as u8, self.0 as u8]
    }

    /// The ISO 4217 minor-unit exponent: 0 for JPY and KRW, 3 for BHD and KWD, 2 for everything else.
    pub const fn minor_unit_decimals(self) -> u32 {
        match &self.code() {
            b"JPY" | b"KRW" => 0,
            b"BHD" | b"KWD" => 3,
            _ => 2,
        }
    }
}

impl fmt::Debug for Currency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Currency({self})")
    }
}

impl fmt::Display for Currency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let c = self.code();
        write!(f, "{}{}{}", c[0] as char, c[1] as char, c[2] as char)
    }
}

/// An amount in one currency.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Money {
    amount: Fixed,
    currency: Currency,
}

impl Money {
    #[inline]
    pub const fn new(amount: Fixed, currency: Currency) -> Money {
        Money { amount, currency }
    }

    #[inline]
    pub const fn zero(currency: Currency) -> Money {
        Money {
            amount: Fixed::ZERO,
            currency,
        }
    }

    #[inline]
    pub const fn amount(self) -> Fixed {
        self.amount
    }

    #[inline]
    pub const fn currency(self) -> Currency {
        self.currency
    }

    /// Same-currency sum.
    #[inline]
    pub fn checked_add(self, rhs: Money) -> Result<Money, FixedError> {
        if self.currency != rhs.currency {
            return Err(FixedError::CurrencyMismatch);
        }
        self.amount
            .checked_add(rhs.amount)
            .map(|a| Money::new(a, self.currency))
            .ok_or(FixedError::Overflow)
    }

    #[inline]
    pub fn checked_sub(self, rhs: Money) -> Result<Money, FixedError> {
        if self.currency != rhs.currency {
            return Err(FixedError::CurrencyMismatch);
        }
        self.amount
            .checked_sub(rhs.amount)
            .map(|a| Money::new(a, self.currency))
            .ok_or(FixedError::Overflow)
    }

    /// Scale by a unitless factor: a quantity, a fee rate, a haircut.
    #[inline]
    pub fn checked_mul(self, factor: Fixed, mode: Rounding) -> Result<Money, FixedError> {
        self.amount
            .checked_mul(factor, mode)
            .map(|a| Money::new(a, self.currency))
            .ok_or(FixedError::Overflow)
    }

    /// Convert at `rate` units of `to` per unit of this currency.
    #[inline]
    pub fn convert(self, rate: Fixed, to: Currency, mode: Rounding) -> Result<Money, FixedError> {
        self.amount
            .checked_mul(rate, mode)
            .map(|a| Money::new(a, to))
            .ok_or(FixedError::Overflow)
    }

    /// Round to the currency's minor unit: cents, yen, fils.
    #[inline]
    pub fn round_to_minor(self, mode: Rounding) -> Money {
        Money::new(
            self.amount.round(self.currency.minor_unit_decimals(), mode),
            self.currency,
        )
    }

    /// Whether the amount already sits on the minor-unit grid.
    #[inline]
    pub fn is_settleable(self) -> bool {
        self.amount
            .round(self.currency.minor_unit_decimals(), Rounding::Down)
            == self.amount
    }

    #[inline]
    pub const fn is_zero(self) -> bool {
        self.amount.is_zero()
    }

    #[inline]
    pub const fn is_negative(self) -> bool {
        self.amount.is_negative()
    }
}

/// `100.5 USD`.
impl fmt::Display for Money {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.amount, self.currency)
    }
}

#[cfg(test)]
#[path = "money_tests.rs"]
mod tests;
