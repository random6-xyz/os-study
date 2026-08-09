extern crate alloc;

use alloc::boxed::Box;
use core::mem::MaybeUninit;

/// Error returned when a push operation fails.
#[derive(Debug, PartialEq, Eq)]
pub enum QueueError {
    /// The queue is full.
    Full,
}

/// Fixed-capacity FIFO circular queue (ring buffer).
///
/// The storage is a fixed-size array of `CAP` slots allocated on the
/// heap, so `new()` never fails. The queue is not thread-safe by itself;
/// the caller must guarantee exclusive access.
pub struct CircularQueue<T, const CAP: usize> {
    data: Box<[MaybeUninit<T>; CAP]>,
    head: usize,
    tail: usize,
    len: usize,
}

impl<T, const CAP: usize> CircularQueue<T, CAP> {
    /// Maximum number of items the queue can hold.
    pub const fn capacity() -> usize {
        CAP
    }

    /// Creates an empty queue.
    pub fn new() -> Self {
        // SAFETY: `MaybeUninit` slots do not need to be initialized.
        let data = Box::new(core::array::from_fn(|_| MaybeUninit::uninit()));
        Self {
            data,
            head: 0,
            tail: 0,
            len: 0,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn is_full(&self) -> bool {
        self.len == CAP
    }

    pub fn len(&self) -> usize {
        self.len
    }

    /// Pushes `item` to the back of the queue.
    pub fn push_back(&mut self, item: T) -> Result<(), QueueError> {
        if self.is_full() {
            return Err(QueueError::Full);
        }
        self.data[self.tail].write(item);
        self.tail = (self.tail + 1) % CAP;
        self.len += 1;
        Ok(())
    }

    /// Returns a mutable reference to the item at the back of the queue.
    pub fn back_mut(&mut self) -> Option<&mut T> {
        if self.is_empty() {
            return None;
        }
        let idx = (self.tail + CAP - 1) % CAP;
        // SAFETY: the slot at `idx` was written by `push_back` and has
        // not been popped yet, so it holds a valid value.
        Some(unsafe { self.data[idx].assume_init_mut() })
    }

    /// Removes and returns the first item that matches `pred`, preserving
    /// the relative order of the remaining items. Returns `None` if no
    /// item matches.
    pub fn remove_if(&mut self, mut pred: impl FnMut(&T) -> bool) -> Option<T> {
        let n = self.len;
        let mut found = None;
        for _ in 0..n {
            let item = self.pop_front()?;
            if found.is_none() && pred(&item) {
                found = Some(item);
            } else {
                // The queue cannot become full here: we only re-insert
                // items that were just popped.
                self.push_back(item).unwrap();
            }
        }
        found
    }

    /// Pops and returns the front item, or `None` if the queue is empty.
    pub fn pop_front(&mut self) -> Option<T> {
        if self.is_empty() {
            return None;
        }
        // SAFETY: the slot at `head` was written by `push_back` and has
        // not been popped yet, so it holds a valid value.
        let item = unsafe { self.data[self.head].assume_init_read() };
        self.head = (self.head + 1) % CAP;
        self.len -= 1;
        Some(item)
    }
}

impl<T, const CAP: usize> Drop for CircularQueue<T, CAP> {
    fn drop(&mut self) {
        while self.pop_front().is_some() {}
    }
}
