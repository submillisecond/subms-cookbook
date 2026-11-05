//! N independent slots under one writer: a per-symbol table where a tick in
//! one symbol never makes a reader of another retry.

use std::marker::PhantomData;
use std::sync::Arc;

use crate::seqlock::{Cell, SeqLockImage};

/// One slot per element of `initial`.
pub fn seqlock_array<T: SeqLockImage>(
    initial: &[T],
) -> (SeqLockArrayWriter<T>, SeqLockArrayReader<T>) {
    assert!(
        !initial.is_empty(),
        "a seqlock array needs at least one slot"
    );
    let cells: Arc<[Cell]> = initial.iter().map(Cell::new).collect();
    (
        SeqLockArrayWriter {
            cells: cells.clone(),
            shadows: initial.to_vec(),
        },
        SeqLockArrayReader {
            cells,
            _image: PhantomData,
        },
    )
}

pub struct SeqLockArrayWriter<T: SeqLockImage> {
    cells: Arc<[Cell]>,
    shadows: Vec<T>,
}

impl<T: SeqLockImage> SeqLockArrayWriter<T> {
    /// Edit slot `idx` and publish it; other slots are untouched.
    #[inline]
    pub fn write(&mut self, idx: usize, f: impl FnOnce(&mut T)) {
        f(&mut self.shadows[idx]);
        self.cells[idx].write(&self.shadows[idx]);
    }

    #[inline]
    pub fn set(&mut self, idx: usize, value: T) {
        self.shadows[idx] = value;
        self.cells[idx].write(&value);
    }

    #[inline]
    pub fn get(&self, idx: usize) -> T {
        self.shadows[idx]
    }

    pub fn len(&self) -> usize {
        self.cells.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    pub fn version(&self, idx: usize) -> u64 {
        self.cells[idx].seq() >> 1
    }

    pub fn reader(&self) -> SeqLockArrayReader<T> {
        SeqLockArrayReader {
            cells: self.cells.clone(),
            _image: PhantomData,
        }
    }
}

pub struct SeqLockArrayReader<T: SeqLockImage> {
    cells: Arc<[Cell]>,
    _image: PhantomData<fn() -> T>,
}

impl<T: SeqLockImage> Clone for SeqLockArrayReader<T> {
    fn clone(&self) -> Self {
        SeqLockArrayReader {
            cells: self.cells.clone(),
            _image: PhantomData,
        }
    }
}

impl<T: SeqLockImage> SeqLockArrayReader<T> {
    #[inline]
    pub fn read(&self, idx: usize) -> T {
        self.read_versioned(idx).0
    }

    #[inline]
    pub fn read_versioned(&self, idx: usize) -> (T, u64) {
        loop {
            if let Some(r) = self.try_read_once(idx) {
                return r;
            }
            std::hint::spin_loop();
        }
    }

    #[inline]
    pub fn try_read(&self, idx: usize, max_attempts: usize) -> Option<T> {
        for _ in 0..max_attempts {
            if let Some((v, _)) = self.try_read_once(idx) {
                return Some(v);
            }
            std::hint::spin_loop();
        }
        None
    }

    #[inline]
    pub fn try_read_once(&self, idx: usize) -> Option<(T, u64)> {
        self.cells[idx].try_read().map(|(v, s)| (v, s >> 1))
    }

    /// Every slot, each individually consistent. Not a snapshot across slots:
    /// a cross-slot snapshot is one image, which is what the base lock holds.
    pub fn read_all(&self, out: &mut Vec<T>) {
        out.clear();
        out.extend((0..self.cells.len()).map(|i| self.read(i)));
    }

    pub fn version(&self, idx: usize) -> u64 {
        self.cells[idx].seq() >> 1
    }

    pub fn len(&self) -> usize {
        self.cells.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }
}

#[cfg(test)]
#[path = "array_tests.rs"]
mod tests;
