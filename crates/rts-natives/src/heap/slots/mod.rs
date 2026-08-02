//! `Slots` — the payload of `Entry::Vec`, in two forms.
//!
//! This module exists to unblock `RTS_OPTIMIZATION.md` §5 Tier 3.2 ("emit the
//! field read as a real `load`") via its prerequisite,
//! `RTS_CLASS_IMPLEMENTATION.md` §8.4 (the object/array representation split).
//! It lands the split ONLY — no emission changes, no new ABI symbol, no knob.
//!
//! ## What was blocking 3.2
//!
//! An object's field words live behind `Entry::Vec`. Until this change that was
//! `Box<Vec<i64>>`: a separately-allocated buffer that the sweep drops and that
//! any `push` reallocates. A raw `load` against those words is unsound for both
//! reasons — the address can be freed under the reader, and it can be moved by a
//! growth the reader did not observe.
//!
//! `Slots` replaces that payload with a two-form value:
//!
//! * [`Slots::Inline`] — a fixed-stride block of `i64` words carried BY VALUE
//!   inside the `Entry`, therefore inside the `Slot`, therefore inside the slot
//!   table. This is `RTS_CLASS_IMPLEMENTATION.md` §4.2's layout: word 0 is the
//!   shape id, words `1..=N` are the direct field slots. There is no separate
//!   buffer, so there is nothing for the sweep to free independently of the slot
//!   and nothing for a `push` to reallocate. **These are the words a `load` can
//!   legally read.**
//! * [`Slots::Heap`] — today's `Box<Vec<i64>>`, unchanged, for arrays and for
//!   anything that outgrows [`INLINE_CAP`].
//!
//! ## Why this shape, and not a new `Entry` variant
//!
//! A new `Entry::Object` variant would leave every one of the ~420 existing
//! `Entry::Vec` match arms compiling untouched while silently no longer seeing
//! class instances — a missed site returns a wrong value instead of failing.
//! Changing the PAYLOAD of the existing variant inverts that: a site that
//! treated the binding as a `Vec` in a way the two forms cannot both satisfy is
//! a **compile error**, and the compiler enumerates the work.
//!
//! Deliberate exception, and the reason it is safe: `Slots` implements
//! `Deref<Target = [i64]>` / `DerefMut`. Read-only and in-place slice access
//! (`len`, `get`, `iter`, `v[i]`, `v[1..]`, `copy_from_slice`, `sort`, …) is
//! **provably identical** on both forms — both yield the same words in the same
//! order — so those sites are not a hazard and do not need to change. What
//! `Deref` cannot express is exactly the hazardous set: anything that changes
//! the LENGTH, anything that takes ownership of a `Vec`/`Box<Vec<_>>`, and
//! anything that reassigns the payload. Those remain type errors until they are
//! routed through an accessor here.
//!
//! ## Which values are INLINE — how a reader tells them apart
//!
//! The rule is a single seam, not a per-site decision: **[`crate::heap::bump::acquire`]
//! is the only producer of the inline form.** Its callers are exactly the shaped
//! allocation paths —
//! `payload_ops::vec_new_object_shaped` (the single allocation path for class
//! instances and object literals), `payload_ops::vec_new_object`, and
//! `shapes::words::alloc_shaped_object_with_id` (runtime-built shaped rows).
//! Every other construction goes through [`Slots::from_vec`] / [`Slots::heap`]
//! and is therefore HEAP — which is every array, every spread result, every
//! generator eager-buffer, every pickle-decoded array.
//!
//! Nothing else should become inline. In particular arrays must not: they grow
//! without bound and `vec_set_by_payload` grows them with HOLEs
//! (`RTS_CLASS_IMPLEMENTATION.md` §8.4), so an inline array would promote on its
//! first few writes and pay the copy for nothing.
//!
//! ## Growth — promote in place
//!
//! A `push`/`resize`/`insert` past [`INLINE_CAP`] **promotes**: the words are
//! copied into a fresh `Box<Vec<i64>>` and the value becomes [`Slots::Heap`],
//! in place, behind whatever `&mut` the caller holds. Semantically this is
//! invisible — same words, same order, same length, same subsequent behaviour.
//!
//! **What it means for a codegen fast path that cached an address** (this is the
//! consequence C2/§4.2 must design around, not an incidental note): the address
//! of the inline block is the address of the slot's `Entry`, and promotion moves
//! the words OUT of it. So a cached field address is invalidated by any
//! operation that can add a slot to that object — adding a property to a shaped
//! object past its inline capacity. That is precisely why
//! `RTS_CLASS_IMPLEMENTATION.md` §4.2 puts an overflow word in the block and why
//! C2's emission is gated on a proven receiver plus C4's ingress guards: the
//! emitted sequence must re-check the form (a one-word discriminant test) rather
//! than assume it, or the object must be proven not to grow. Nothing in this
//! module emits such a sequence; it establishes the invariant the emission will
//! rely on.
//!
//! ## GC
//!
//! `Entry`'s `Traceable::trace_children` walks `Entry::Vec(v)` with `v.iter()`,
//! which goes through `Deref` and therefore visits the inline words exactly as
//! it visited the heap buffer's words. Tracing needs no form-specific code, and
//! that is by construction: the inline words live inside the `Slot`, which the
//! collector already owns and already keeps alive for the slot's lifetime.
//!
//! ## Live-byte accounting
//!
//! `handles::entry_heap_bytes` charges an `Entry::Vec` `len * 8` bytes and is
//! left form-agnostic ON PURPOSE, even though an inline block is not a separate
//! allocation. That number feeds only the GC trigger cadence (`heap::live`), and
//! a representation change must not move the collection schedule — an inline
//! object suddenly costing 0 would delay every collection on an object-heavy
//! program and turn this change into a silent GC-tuning change. It also keeps
//! the alloc and free sides symmetric across a mid-life promotion.
//!
//! ## Serde
//!
//! `Entry` is not `Serialize`. The two persistence paths both go through a
//! slice: `heap::pickle` snapshots via `snap_vec(slots)` and
//! `string_pool::snapshot` copies with `to_vec()`. Both read through `Deref`,
//! so the on-disk pickle format is byte-for-byte unchanged and **no version bump
//! is required**. See the report for the reasoning.

/// Words carried inline, INCLUDING the shape-id header word.
///
/// So a class with `INLINE_CAP - 1` declared fields fits; wider instances take
/// the heap form. 8 covers a 7-field class, which the size classes in
/// [`crate::heap::bump`] (4/8/16/32 words) suggest is where the mass of shaped
/// objects sits.
///
/// The cost of raising it is paid by EVERY slot in the table, not only by
/// objects: `Slots` is a field of `Entry`, `Entry` is a field of `Slot`, and the
/// slot table is sized by the largest variant. At 8 words `Slots` is 72 bytes
/// (64 words + a length/discriminant word), which is the enum's new floor.
///
/// TODO(measure): 8 is reasoned from the bump size classes and from §4.2's
/// "direct property slots", not swept. The two things to measure before moving
/// it are (a) the distribution of `field_count` at `vec_new_object_shaped`, and
/// (b) RSS / cache-miss rate on an allocation-heavy fixture, since a wider
/// inline block trades promotions for a colder slot table.
pub const INLINE_CAP: usize = 8;

/// The payload of `Entry::Vec`. See the module docs.
#[derive(Debug, Clone)]
pub enum Slots {
    /// Fixed-stride block carried by value — the loadable form. Only
    /// `words[..len]` is meaningful; the tail is zeroed padding.
    Inline {
        /// Live word count, `<= INLINE_CAP`.
        len: u32,
        /// The block. Word 0 is the shape id for a shaped object.
        words: [i64; INLINE_CAP],
    },
    /// Separately-allocated buffer — today's representation, unchanged.
    Heap(Box<Vec<i64>>),
}

impl Default for Slots {
    fn default() -> Self {
        Slots::Inline {
            len: 0,
            words: [0; INLINE_CAP],
        }
    }
}

impl Slots {
    /// An EMPTY value able to hold at least `words` words without reallocating,
    /// preferring the inline form. The constructor of the shaped-allocation
    /// seam — see the module docs on which values become inline.
    #[inline]
    pub fn with_capacity_inline(words: usize) -> Self {
        if words <= INLINE_CAP {
            Slots::Inline {
                len: 0,
                words: [0; INLINE_CAP],
            }
        } else {
            Slots::Heap(Box::new(Vec::with_capacity(words)))
        }
    }

    /// Take ownership of an existing `Vec` — always the HEAP form.
    ///
    /// Deliberately does not try to inline a short `Vec`: the caller already
    /// paid for the allocation, and copying it into the block would be strictly
    /// more work for a value whose growth profile is unknown (it is an array
    /// unless it came through the shaped seam).
    #[inline]
    pub fn from_vec(v: Vec<i64>) -> Self {
        Slots::Heap(Box::new(v))
    }

    /// Take ownership of an already-boxed buffer — always the HEAP form.
    #[inline]
    pub fn heap(v: Box<Vec<i64>>) -> Self {
        Slots::Heap(v)
    }

    /// `true` when the words live inside the `Entry` (the loadable form).
    #[inline]
    pub fn is_inline(&self) -> bool {
        matches!(self, Slots::Inline { .. })
    }

    /// Words this value can hold before it must grow/promote.
    #[inline]
    pub fn capacity(&self) -> usize {
        match self {
            Slots::Inline { .. } => INLINE_CAP,
            Slots::Heap(v) => v.capacity(),
        }
    }

    /// The words, as a slice. Identical content in both forms.
    #[inline]
    pub fn as_slice(&self) -> &[i64] {
        match self {
            Slots::Inline { len, words } => &words[..*len as usize],
            Slots::Heap(v) => v.as_slice(),
        }
    }

    /// Mutable slice. Cannot change the length, so it cannot promote.
    #[inline]
    pub fn as_mut_slice(&mut self) -> &mut [i64] {
        match self {
            Slots::Inline { len, words } => &mut words[..*len as usize],
            Slots::Heap(v) => v.as_mut_slice(),
        }
    }

    /// Copy the words out into an owned `Vec`. The replacement for the old
    /// `(**v).clone()` / `v.as_ref().clone()` spellings.
    #[inline]
    pub fn to_owned_vec(&self) -> Vec<i64> {
        self.as_slice().to_vec()
    }

    /// Consume, yielding an owned `Vec`. Copies for the inline form.
    #[inline]
    pub fn into_vec(self) -> Vec<i64> {
        match self {
            Slots::Inline { len, words } => words[..len as usize].to_vec(),
            Slots::Heap(v) => *v,
        }
    }

    /// Replace the entire contents. The replacement for `**v = other`.
    #[inline]
    pub fn replace_all(&mut self, other: Vec<i64>) {
        *self = Slots::from_vec(other);
    }

    /// Move the words into a heap buffer, in place. No-op when already heap.
    ///
    /// This is the ONLY transition between the forms; there is no demotion, so
    /// a value that has been observed as `Heap` stays `Heap` for its lifetime.
    /// That one-way property is what lets a guard be a single test.
    #[inline]
    pub fn promote(&mut self) {
        if let Slots::Inline { len, words } = self {
            let mut v = Vec::with_capacity(INLINE_CAP * 2);
            v.extend_from_slice(&words[..*len as usize]);
            *self = Slots::Heap(Box::new(v));
        }
    }

    /// Promote unless `extra` more words still fit inline.
    #[inline]
    fn reserve_for(&mut self, extra: usize) {
        if let Slots::Inline { len, .. } = self {
            if *len as usize + extra > INLINE_CAP {
                self.promote();
            }
        }
    }
}

impl std::ops::Deref for Slots {
    type Target = [i64];
    #[inline]
    fn deref(&self) -> &[i64] {
        self.as_slice()
    }
}

impl std::ops::DerefMut for Slots {
    #[inline]
    fn deref_mut(&mut self) -> &mut [i64] {
        self.as_mut_slice()
    }
}

impl AsRef<[i64]> for Slots {
    #[inline]
    fn as_ref(&self) -> &[i64] {
        self.as_slice()
    }
}

impl AsMut<[i64]> for Slots {
    #[inline]
    fn as_mut(&mut self) -> &mut [i64] {
        self.as_mut_slice()
    }
}

impl From<Vec<i64>> for Slots {
    #[inline]
    fn from(v: Vec<i64>) -> Self {
        Slots::from_vec(v)
    }
}

impl From<Box<Vec<i64>>> for Slots {
    #[inline]
    fn from(v: Box<Vec<i64>>) -> Self {
        Slots::Heap(v)
    }
}

impl Extend<i64> for Slots {
    #[inline]
    fn extend<T: IntoIterator<Item = i64>>(&mut self, iter: T) {
        for v in iter {
            self.push(v);
        }
    }
}

impl FromIterator<i64> for Slots {
    #[inline]
    fn from_iter<T: IntoIterator<Item = i64>>(iter: T) -> Self {
        Slots::from_vec(iter.into_iter().collect())
    }
}

impl PartialEq for Slots {
    #[inline]
    fn eq(&self, other: &Slots) -> bool {
        self.as_slice() == other.as_slice()
    }
}


impl crate::heap::handles::Entry {
    /// An `Entry::Vec` over an owned word buffer — the HEAP form.
    ///
    /// The spelling every construction site uses, so that no call site has to
    /// name [`Slots`] or reason about which form it wants. Arrays, spread
    /// results, generator buffers and decoded payloads all belong here; the
    /// INLINE form has exactly one producer, [`crate::heap::bump::acquire`],
    /// reached only from the shaped object-allocation seam.
    ///
    /// Lives here rather than beside the enum because `handles.rs` is a
    /// draining target that may only shrink (`RTS_CLASS_IMPLEMENTATION.md` §6.1)
    /// — and because this constructor is about the payload, which is this
    /// module's subject.
    #[inline]
    pub fn vec(words: Vec<i64>) -> crate::heap::handles::Entry {
        crate::heap::handles::Entry::Vec(Slots::from_vec(words))
    }
}

/// The length-changing accessors — the set `Deref` cannot express, and so the
/// set that had to be converted at every call site.
mod ops;

#[cfg(test)]
mod tests;
