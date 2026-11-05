/// How a quotient that does not land on the grid is resolved.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Rounding {
    /// Round to the nearest; on a tie, to the even neighbour.
    #[default]
    HalfEven,
    /// Round to the nearest; on a tie, away from zero.
    HalfUp,
    /// Round to the nearest; on a tie, toward zero.
    HalfDown,
    /// Truncate toward zero.
    Down,
    /// Away from zero.
    Up,
    /// Toward negative infinity.
    Floor,
    /// Toward positive infinity.
    Ceiling,
}

impl Rounding {
    /// `n / d` rounded under this mode.
    pub(crate) fn div(self, n: i128, d: i128) -> i128 {
        debug_assert!(d != 0, "division by zero reaches Rounding::div");
        let q = n / d;
        let r = n % d;
        if r == 0 {
            return q;
        }
        let negative = (n < 0) != (d < 0);
        let twice = (r.unsigned_abs()).checked_mul(2).expect("|2r| fits u128");
        let dabs = d.unsigned_abs();
        let away = match self {
            Rounding::Down => false,
            Rounding::Up => true,
            Rounding::Floor => negative,
            Rounding::Ceiling => !negative,
            Rounding::HalfUp => twice >= dabs,
            Rounding::HalfDown => twice > dabs,
            Rounding::HalfEven => twice > dabs || (twice == dabs && q % 2 != 0),
        };
        if away {
            if negative { q - 1 } else { q + 1 }
        } else {
            q
        }
    }

    /// The mode's `java.math.RoundingMode` name, for the Java port's cross-language fixtures and for logs.
    pub const fn as_str(self) -> &'static str {
        match self {
            Rounding::HalfEven => "HALF_EVEN",
            Rounding::HalfUp => "HALF_UP",
            Rounding::HalfDown => "HALF_DOWN",
            Rounding::Down => "DOWN",
            Rounding::Up => "UP",
            Rounding::Floor => "FLOOR",
            Rounding::Ceiling => "CEILING",
        }
    }
}

#[cfg(test)]
#[path = "rounding_tests.rs"]
mod tests;
