//! Sequence lock: one writer publishes a snapshot, any number of readers take
//! it, no lock on either side.
//!
//! The image is a fixed run of 64-bit words read and written through relaxed
//! atomics, so a torn read is a value the sequence check discards rather than
//! a data race. Any `Copy` type that can copy itself in and out of words is an
//! image: see [`SeqLockImage`].
//!
//! ```
//! use subms_seqlock::seqlock;
//!
//! let (mut writer, reader) = seqlock([10_025u64, 300, 10_026, 500]);
//! writer.write(|top| {
//!     top[0] = 10_026;        // bid moves up a tick
//!     top[1] = 200;
//! });
//! let snap = reader.read();   // every field from the same write
//! assert_eq!(snap, [10_026, 200, 10_026, 500]);
//! assert_eq!(reader.version(), 1);
//! ```
//!
//! Features: `array` (N slots under one writer), `latch` (the double-buffered
//! seqcount latch, readers never wait on a write), `stats` (retry counters).
//!
//! <https://www.submillisecond.com/cookbook/recipes/subms-seqlock>

mod seqlock;

pub mod features;

#[cfg(feature = "harness")]
pub mod recipe;

pub use seqlock::{SeqLockImage, SeqLockReader, SeqLockWriter, seqlock};

#[cfg(feature = "array")]
pub use features::array::{SeqLockArrayReader, SeqLockArrayWriter, seqlock_array};
#[cfg(feature = "latch")]
pub use features::latch::{SeqLatchReader, SeqLatchWriter, seqlatch};
#[cfg(feature = "stats")]
pub use features::stats::{InstrumentedReader, ReadStats, ReadStatsSnapshot};

#[cfg(test)]
#[path = "sample_app_tests.rs"]
mod sample_app_tests;
