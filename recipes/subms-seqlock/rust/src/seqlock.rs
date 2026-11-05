use std::marker::PhantomData;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering, fence};

/// A value laid out as a fixed run of 64-bit words.
///
/// `store` hands each word to `put`; `load` pulls each word from `get`. The
/// lock streams them straight through its atomics, so neither path has an
/// intermediate buffer.
pub trait SeqLockImage: Copy {
    const WORDS: usize;
    fn store(&self, put: impl FnMut(usize, u64));
    fn load(get: impl FnMut(usize) -> u64) -> Self;
}

impl SeqLockImage for u64 {
    const WORDS: usize = 1;
    #[inline]
    fn store(&self, mut put: impl FnMut(usize, u64)) {
        put(0, *self);
    }
    #[inline]
    fn load(mut get: impl FnMut(usize) -> u64) -> Self {
        get(0)
    }
}

impl SeqLockImage for i64 {
    const WORDS: usize = 1;
    #[inline]
    fn store(&self, mut put: impl FnMut(usize, u64)) {
        put(0, *self as u64);
    }
    #[inline]
    fn load(mut get: impl FnMut(usize) -> u64) -> Self {
        get(0) as i64
    }
}

impl SeqLockImage for f64 {
    const WORDS: usize = 1;
    #[inline]
    fn store(&self, mut put: impl FnMut(usize, u64)) {
        put(0, self.to_bits());
    }
    #[inline]
    fn load(mut get: impl FnMut(usize) -> u64) -> Self {
        f64::from_bits(get(0))
    }
}

impl<const N: usize> SeqLockImage for [u64; N] {
    const WORDS: usize = N;
    #[inline]
    fn store(&self, mut put: impl FnMut(usize, u64)) {
        for (i, w) in self.iter().enumerate() {
            put(i, *w);
        }
    }
    #[inline]
    fn load(get: impl FnMut(usize) -> u64) -> Self {
        std::array::from_fn(get)
    }
}

impl<const N: usize> SeqLockImage for [i64; N] {
    const WORDS: usize = N;
    #[inline]
    fn store(&self, mut put: impl FnMut(usize, u64)) {
        for (i, w) in self.iter().enumerate() {
            put(i, *w as u64);
        }
    }
    #[inline]
    fn load(mut get: impl FnMut(usize) -> u64) -> Self {
        std::array::from_fn(|i| get(i) as i64)
    }
}

impl<const N: usize> SeqLockImage for [f64; N] {
    const WORDS: usize = N;
    #[inline]
    fn store(&self, mut put: impl FnMut(usize, u64)) {
        for (i, w) in self.iter().enumerate() {
            put(i, w.to_bits());
        }
    }
    #[inline]
    fn load(mut get: impl FnMut(usize) -> u64) -> Self {
        std::array::from_fn(|i| f64::from_bits(get(i)))
    }
}

// 128 bytes keeps the sequence off the image's line and off the neighbour the
// adjacent-line prefetcher drags in with it.
#[repr(align(128))]
pub(crate) struct Padded(pub(crate) AtomicU64);

pub(crate) struct Cell {
    pub(crate) seq: Padded,
    pub(crate) words: Box<[AtomicU64]>,
}

impl Cell {
    pub(crate) fn new<T: SeqLockImage>(initial: &T) -> Cell {
        assert!(T::WORDS > 0, "a seqlock image needs at least one word");
        let words: Box<[AtomicU64]> = (0..T::WORDS).map(|_| AtomicU64::new(0)).collect();
        initial.store(|i, w| words[i].store(w, Ordering::Relaxed));
        Cell {
            seq: Padded(AtomicU64::new(0)),
            words,
        }
    }

    // Odd, words, even. The fence stops a word becoming visible ahead of the
    // odd sequence; the release on the even store covers the words before it.
    #[inline]
    pub(crate) fn write<T: SeqLockImage>(&self, value: &T) {
        let s = self.seq.0.load(Ordering::Relaxed);
        self.seq.0.store(s.wrapping_add(1), Ordering::Relaxed);
        fence(Ordering::Release);
        value.store(|i, w| self.words[i].store(w, Ordering::Relaxed));
        self.seq.0.store(s.wrapping_add(2), Ordering::Release);
    }

    #[inline]
    pub(crate) fn try_read<T: SeqLockImage>(&self) -> Option<(T, u64)> {
        let s1 = self.seq.0.load(Ordering::Acquire);
        if s1 & 1 == 1 {
            return None;
        }
        let value = T::load(|i| self.words[i].load(Ordering::Relaxed));
        fence(Ordering::Acquire);
        let s2 = self.seq.0.load(Ordering::Relaxed);
        (s1 == s2).then_some((value, s1))
    }

    #[inline]
    pub(crate) fn seq(&self) -> u64 {
        self.seq.0.load(Ordering::Acquire)
    }
}

/// A (writer, reader) pair over `initial`. Clone the reader per thread; the
/// writer is unique by construction.
pub fn seqlock<T: SeqLockImage>(initial: T) -> (SeqLockWriter<T>, SeqLockReader<T>) {
    let inner = Arc::new(Cell::new(&initial));
    (
        SeqLockWriter {
            inner: inner.clone(),
            shadow: initial,
        },
        SeqLockReader {
            inner,
            _image: PhantomData,
        },
    )
}

/// The one writer. Keeps its own copy of the value so `write` edits in place.
pub struct SeqLockWriter<T: SeqLockImage> {
    pub(crate) inner: Arc<Cell>,
    shadow: T,
}

impl<T: SeqLockImage> SeqLockWriter<T> {
    /// Edit the current value and publish the result as one snapshot.
    #[inline]
    pub fn write(&mut self, f: impl FnOnce(&mut T)) {
        f(&mut self.shadow);
        self.inner.write(&self.shadow);
    }

    #[inline]
    pub fn set(&mut self, value: T) {
        self.shadow = value;
        self.inner.write(&self.shadow);
    }

    /// The current value, from the writer's copy. No atomics.
    #[inline]
    pub fn get(&self) -> T {
        self.shadow
    }

    /// Writes published so far.
    #[inline]
    pub fn version(&self) -> u64 {
        self.inner.seq() >> 1
    }

    pub fn reader(&self) -> SeqLockReader<T> {
        SeqLockReader {
            inner: self.inner.clone(),
            _image: PhantomData,
        }
    }
}

/// A reader. `Clone` per thread.
pub struct SeqLockReader<T: SeqLockImage> {
    inner: Arc<Cell>,
    _image: PhantomData<fn() -> T>,
}

impl<T: SeqLockImage> Clone for SeqLockReader<T> {
    fn clone(&self) -> Self {
        SeqLockReader {
            inner: self.inner.clone(),
            _image: PhantomData,
        }
    }
}

impl<T: SeqLockImage> SeqLockReader<T> {
    /// A consistent snapshot, retrying until one is taken.
    #[inline]
    pub fn read(&self) -> T {
        self.read_versioned().0
    }

    /// A snapshot and the write count it came from.
    #[inline]
    pub fn read_versioned(&self) -> (T, u64) {
        loop {
            if let Some(r) = self.try_read_once() {
                return r;
            }
            std::hint::spin_loop();
        }
    }

    /// At most `max_attempts` tries, then `None`.
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

    /// One attempt: the snapshot and its version, or `None` if a write was in
    /// flight or landed during the copy.
    #[inline]
    pub fn try_read_once(&self) -> Option<(T, u64)> {
        self.inner.try_read().map(|(v, s)| (v, s >> 1))
    }

    #[inline]
    pub fn version(&self) -> u64 {
        self.inner.seq() >> 1
    }

    #[inline]
    pub fn is_writing(&self) -> bool {
        self.inner.seq() & 1 == 1
    }
}

#[cfg(test)]
#[path = "seqlock_tests.rs"]
mod tests;
