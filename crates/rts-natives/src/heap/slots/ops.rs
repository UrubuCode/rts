//! Length-changing accessors for [`Slots`].
//!
//! Split out of `slots/mod.rs` for the 500-line ceiling. This is exactly the
//! set of operations `Deref<Target = [i64]>` cannot express and therefore the
//! set that had to be converted at every call site: anything that can change
//! the length, and so anything that can PROMOTE an inline block to the heap
//! form (see the module docs on what promotion means for a cached address).

use super::Slots;

impl Slots {

    /// Append one word, promoting if it does not fit.
    #[inline]
    pub fn push(&mut self, value: i64) {
        self.reserve_for(1);
        match self {
            Slots::Inline { len, words } => {
                words[*len as usize] = value;
                *len += 1;
            }
            Slots::Heap(v) => v.push(value),
        }
    }

    /// Remove and return the last word.
    #[inline]
    pub fn pop(&mut self) -> Option<i64> {
        match self {
            Slots::Inline { len, words } => {
                if *len == 0 {
                    None
                } else {
                    *len -= 1;
                    Some(words[*len as usize])
                }
            }
            Slots::Heap(v) => v.pop(),
        }
    }

    /// Grow or shrink to `new_len`, filling new positions with `value` —
    /// `Vec::resize` semantics exactly (this is what carries JS sparse-array
    /// HOLE growth in `payload_ops::vec_set_by_payload`).
    #[inline]
    pub fn resize(&mut self, new_len: usize, value: i64) {
        if new_len > self.len() {
            self.reserve_for(new_len - self.len());
        }
        match self {
            Slots::Inline { len, words } => {
                let n = new_len as u32;
                for w in words.iter_mut().take(new_len).skip(*len as usize) {
                    *w = value;
                }
                *len = n;
            }
            Slots::Heap(v) => v.resize(new_len, value),
        }
    }

    /// Append a slice.
    #[inline]
    pub fn extend_from_slice(&mut self, other: &[i64]) {
        self.reserve_for(other.len());
        match self {
            Slots::Inline { len, words } => {
                let base = *len as usize;
                words[base..base + other.len()].copy_from_slice(other);
                *len += other.len() as u32;
            }
            Slots::Heap(v) => v.extend_from_slice(other),
        }
    }

    /// Insert at `index`, shifting the tail right.
    #[inline]
    pub fn insert(&mut self, index: usize, value: i64) {
        self.reserve_for(1);
        match self {
            Slots::Inline { len, words } => {
                let n = *len as usize;
                words.copy_within(index..n, index + 1);
                words[index] = value;
                *len += 1;
            }
            Slots::Heap(v) => v.insert(index, value),
        }
    }

    /// Remove at `index`, shifting the tail left.
    #[inline]
    pub fn remove(&mut self, index: usize) -> i64 {
        match self {
            Slots::Inline { len, words } => {
                let n = *len as usize;
                let out = words[index];
                words.copy_within(index + 1..n, index);
                *len -= 1;
                out
            }
            Slots::Heap(v) => v.remove(index),
        }
    }

    /// Remove at `index` by swapping in the last word.
    #[inline]
    pub fn swap_remove(&mut self, index: usize) -> i64 {
        match self {
            Slots::Inline { len, words } => {
                let last = *len as usize - 1;
                words.swap(index, last);
                *len -= 1;
                words[last]
            }
            Slots::Heap(v) => v.swap_remove(index),
        }
    }

    /// Shorten to at most `new_len`.
    #[inline]
    pub fn truncate(&mut self, new_len: usize) {
        match self {
            Slots::Inline { len, .. } => {
                if (new_len as u32) < *len {
                    *len = new_len as u32;
                }
            }
            Slots::Heap(v) => v.truncate(new_len),
        }
    }

    /// Drop every word.
    #[inline]
    pub fn clear(&mut self) {
        match self {
            Slots::Inline { len, .. } => *len = 0,
            Slots::Heap(v) => v.clear(),
        }
    }

    /// Keep only the words `f` accepts.
    #[inline]
    pub fn retain(&mut self, mut f: impl FnMut(&i64) -> bool) {
        match self {
            Slots::Inline { len, words } => {
                let mut out = 0usize;
                for i in 0..*len as usize {
                    if f(&words[i]) {
                        words[out] = words[i];
                        out += 1;
                    }
                }
                *len = out as u32;
            }
            Slots::Heap(v) => v.retain(|x| f(x)),
        }
    }

    /// Move every word of `other` onto the end, leaving `other` empty.
    #[inline]
    pub fn append(&mut self, other: &mut Slots) {
        let tail = std::mem::take(other).into_vec();
        self.extend_from_slice(&tail);
    }

    /// Split off the words from `at` onwards into a new value.
    #[inline]
    pub fn split_off(&mut self, at: usize) -> Slots {
        let tail = self.as_slice()[at..].to_vec();
        self.truncate(at);
        Slots::from_vec(tail)
    }

    /// Remove `range` and return the removed words, shifting the tail left.
    ///
    /// Spelled as a method returning an owned `Vec` rather than as `Vec::drain`
    /// returning an iterator: a borrowing drain guard over the inline form would
    /// have to borrow the block through the enum discriminant, and every call
    /// site in the tree immediately `collect()`ed the iterator anyway.
    #[inline]
    pub fn drain_range(&mut self, range: std::ops::Range<usize>) -> Vec<i64> {
        match self {
            Slots::Inline { len, words } => {
                let n = *len as usize;
                let out = words[range.start..range.end].to_vec();
                words.copy_within(range.end..n, range.start);
                *len -= (range.end - range.start) as u32;
                out
            }
            Slots::Heap(v) => v.drain(range).collect(),
        }
    }

    /// Ensure room for `additional` more words. Promotes when they cannot fit
    /// inline — the reservation must be a real promise, not a best effort.
    #[inline]
    pub fn reserve(&mut self, additional: usize) {
        self.reserve_for(additional);
        if let Slots::Heap(v) = self {
            v.reserve(additional);
        }
    }
}

