//! Typed error surface for the wheel family.

use std::fmt;

/// Every way a schedule can be refused. `schedule` clamps instead of
/// returning these; `try_schedule` surfaces them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerError {
    /// The delay is past what the wheel can represent. On the base wheel the
    /// ceiling is `num_slots * u32::MAX` (the rounds counter is a `u32`); on
    /// the hierarchical wheel it is the coarsest level's span.
    DelayTooLong { delay: u64, max: u64 },
}

impl fmt::Display for TimerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TimerError::DelayTooLong { delay, max } => {
                write!(
                    f,
                    "delay {delay} ticks exceeds the wheel capacity of {max} ticks"
                )
            }
        }
    }
}

impl std::error::Error for TimerError {}

#[cfg(test)]
#[path = "error_tests.rs"]
mod tests;
