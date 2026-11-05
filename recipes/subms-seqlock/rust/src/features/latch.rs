//! The double-buffered latch, `seqcount_latch` from the Linux kernel. Two
//! copies; the sequence's low bit says which one readers use. There is no
//! write-in-progress state, so a reader never waits on the writer and retries
//! only if the writer flipped while it was copying.

use std::marker::PhantomData;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering, fence};

use crate::seqlock::{Padded, SeqLockImage};

pub(crate) struct LatchCell {
    pub(crate) seq: Padded,
    pub(crate) copies: [Box<[AtomicU64]>; 2],
}

impl LatchCell {
    fn new<T: SeqLockImage>(initial: &T) -> LatchCell {
        assert!(T::WORDS > 0, "a seqlock image needs at least one word");
        let make = || -> Box<[AtomicU64]> {
            let words: Box<[AtomicU64]> = (0..T::WORDS).map(|_| AtomicU64::new(0)).collect();
            initial.store(|i, w| words[i].store(w, Ordering::Relaxed));
            words
        };
        LatchCell {
            seq: Padded(AtomicU64::new(0)),
            copies: [make(), make()],
        }
    }

    // Flip readers to the other copy, rewrite the one they left, flip them
    // onto it, then bring the stale copy up to date for the next write.
    #[inline]
    fn write<T: SeqLockImage>(&self, value: &T) {
        let s = self.seq.0.load(Ordering::Relaxed);
        self.seq.0.store(s.wrapping_add(1), Ordering::Relaxed);
        fence(Ordering::Release);
        let a = &self.copies[(s & 1) as usize];
        value.store(|i, w| a[i].store(w, Ordering::Relaxed));
        self.seq.0.store(s.wrapping_add(2), Ordering::Release);
        fence(Ordering::Release);
        let b = &self.copies[((s + 1) & 1) as usize];
        value.store(|i, w| b[i].store(w, Ordering::Relaxed));
    }

    #[inline]
    fn try_read<T: SeqLockImage>(&self) -> Option<(T, u64)> {
        let s1 = self.seq.0.load(Ordering::Acquire);
        let copy = &self.copies[(s1 & 1) as usize];
        let value = T::load(|i| copy[i].load(Ordering::Relaxed));
        fence(Ordering::Acquire);
        let s2 = self.seq.0.load(Ordering::Relaxed);
        (s1 == s2).then_some((value, s1))
    }

    #[inline]
    fn seq(&self) -> u64 {
        self.seq.0.load(Ordering::Acquire)
    }
}

/// A (writer, reader) pair over `initial`, double-buffered.
pub fn seqlatch<T: SeqLockImage>(initial: T) -> (SeqLatchWriter<T>, SeqLatchReader<T>) {
    let inner = Arc::new(LatchCell::new(&initial));
    (
        SeqLatchWriter {
            inner: inner.clone(),
            shadow: initial,
        },
        SeqLatchReader {
            inner,
            _image: PhantomData,
        },
    )
}

pub struct SeqLatchWriter<T: SeqLockImage> {
    pub(crate) inner: Arc<LatchCell>,
    shadow: T,
}

impl<T: SeqLockImage> SeqLatchWriter<T> {
    #[inline]
    pub fn write(&mut self, f: impl FnOnce(&mut T)) {
        f(&mut self.shadow);
        self.inner.write(&self.shadow);
    }

    #[inline]
    pub fn set(&mut self, value: T) {
        self.shadow = value;
        self.inner.write(&value);
    }

    #[inline]
    pub fn get(&self) -> T {
        self.shadow
    }

    /// Writes published. Two sequence steps per write.
    pub fn version(&self) -> u64 {
        self.inner.seq() >> 1
    }

    pub fn reader(&self) -> SeqLatchReader<T> {
        SeqLatchReader {
            inner: self.inner.clone(),
            _image: PhantomData,
        }
    }
}

pub struct SeqLatchReader<T: SeqLockImage> {
    inner: Arc<LatchCell>,
    _image: PhantomData<fn() -> T>,
}

impl<T: SeqLockImage> Clone for SeqLatchReader<T> {
    fn clone(&self) -> Self {
        SeqLatchReader {
            inner: self.inner.clone(),
            _image: PhantomData,
        }
    }
}

impl<T: SeqLockImage> SeqLatchReader<T> {
    #[inline]
    pub fn read(&self) -> T {
        self.read_versioned().0
    }

    /// A reader that lands between a write's two flips gets the previous value
    /// with the previous count, which is the consistent pair.
    #[inline]
    pub fn read_versioned(&self) -> (T, u64) {
        loop {
            if let Some(r) = self.try_read_once() {
                return r;
            }
            std::hint::spin_loop();
        }
    }

    #[inline]
    pub fn try_read(&self, max_attempts: usize) -> Option<T> {
        for _ in 0..max_attempts {
            if let Some((v, _)) = self.try_read_once() {
                return Some(v);
            }
            std::hint::spin_loop();
        }
        None
    }

    #[inline]
    pub fn try_read_once(&self) -> Option<(T, u64)> {
        self.inner.try_read().map(|(v, s)| (v, s >> 1))
    }

    pub fn version(&self) -> u64 {
        self.inner.seq() >> 1
    }
}

#[cfg(test)]
#[path = "latch_tests.rs"]
mod tests;
