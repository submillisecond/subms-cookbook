//! Exact fixed-point arithmetic for prices, quantities and notionals.
//!
//! A [`Fixed`] is an `i64` at nine decimal places. Add, subtract and compare
//! are the integer instructions; multiply and divide widen to `i128` and round
//! once under a named [`Rounding`] mode.
//!
//! ```
//! use subms_fixed_point::{Fixed, Rounding};
//!
//! let px = Fixed::from_parts(100, 250_000_000).unwrap();   // 100.25
//! let qty = Fixed::from_int(300).unwrap();
//! let tick = Fixed::from_parts(0, 10_000_000).unwrap();    // 0.01
//!
//! let notional = px.checked_mul(qty, Rounding::HalfEven).unwrap();
//! assert_eq!(notional, Fixed::from_int(30_075).unwrap());
//! assert!(px.is_on_tick(tick));
//!
//! let off_grid = Fixed::from_parts(100, 254_000_000).unwrap();
//! assert_eq!(off_grid.round_to_tick(tick, Rounding::HalfEven).unwrap(), px);
//! assert_eq!(notional.to_string(), "30075");
//! ```
//!
//! Features: `text` (zero-allocation parse and write), `money` (currency-tagged
//! amounts), `wide` (`Fixed128`, exact products and sums at eighteen places).
//!
//! <https://www.submillisecond.com/cookbook/recipes/subms-fixed-point>

mod error;
mod fixed;
mod rounding;

pub mod features;

#[cfg(feature = "harness")]
pub mod recipe;

pub use error::FixedError;
pub use fixed::{DECIMALS, Fixed, SCALE};
pub use rounding::Rounding;

#[cfg(feature = "money")]
pub use features::money::{Currency, Money};
#[cfg(feature = "wide")]
pub use features::wide::{Fixed128, WIDE_DECIMALS, WIDE_SCALE};

#[cfg(test)]
#[path = "sample_app_tests.rs"]
mod sample_app_tests;
