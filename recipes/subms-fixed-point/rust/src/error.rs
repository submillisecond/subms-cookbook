use std::fmt;

/// The typed failure surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FixedError {
    /// The result does not fit in the 64-bit raw, or an intermediate does not fit in 128 bits.
    Overflow,
    /// A divisor or a tick of zero.
    DivisionByZero,
    /// The value is not an integer multiple of the tick.
    OffTick,
    /// A tick that is zero or negative.
    InvalidTick,
    /// Text that is not `[-]digits[.digits]`, or has no digits at all.
    InvalidText,
    /// Text with more fractional digits than the type holds, or more scale than a requested conversion can express without rounding.
    TooPrecise,
    /// A float that is NaN or infinite.
    NotFinite,
    /// Two amounts in different currencies met in an operation that needs one currency.
    CurrencyMismatch,
    /// A currency code that is not three ASCII letters.
    InvalidCurrency,
    /// The caller's output buffer is too small for the rendered text.
    BufferTooSmall,
}

impl fmt::Display for FixedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            FixedError::Overflow => "fixed-point overflow",
            FixedError::DivisionByZero => "division by zero",
            FixedError::OffTick => "value is not on the tick grid",
            FixedError::InvalidTick => "tick must be positive",
            FixedError::InvalidText => "not a decimal number",
            FixedError::TooPrecise => "more precision than the type holds",
            FixedError::NotFinite => "float is NaN or infinite",
            FixedError::CurrencyMismatch => "currencies differ",
            FixedError::InvalidCurrency => "currency code must be three ASCII letters",
            FixedError::BufferTooSmall => "output buffer too small",
        })
    }
}

impl std::error::Error for FixedError {}
