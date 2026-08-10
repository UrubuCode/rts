//! The entries of a keyed collection: insertion order, and an index over it.
//!
//! # Why two vectors are the source of truth and the hash table is not
//!
//! The specification requires iteration in **insertion order** — `forEach`,
//! `keys`, `entries` and `for-of` all say so — and a bare hash table has no
//! order to give. So `keys` and `values` are what the collection IS, and the
//! index only removes the scan: `heads[bucket]` is the first slot hashing there
//! and `next[slot]` the one after it, `-1` ending the chain. That is the shape
//! the engine being replaced arrived at in `stdlib/map_set.ts`, where dropping
//! the scan was worth 86x on the last key of a thousand — and, more to the
//! point, turned every loop that consulted a map per item from quadratic into
//! linear.
//!
//! # Why `delete` shifts
//!
//! Because insertion order has to survive it, and a tombstone would make
//! iteration skip holes at every step for the sake of a path programs take
//! rarely. Shifting renumbers every slot above the removed one, so the index is
//! rebuilt — deliberately the slow path, chosen once rather than paid on every
//! read.
//!
//! # Why the equality is not `==` on the bits
//!
//! `Map` keys on **SameValueZero**, which differs from `===` in the one cell
//! that matters: `NaN` is equal to itself, so `m.set(NaN, 1); m.get(NaN)` finds
//! it. Keying on `===` grows a second entry every time and reports nothing.
//! [`crate::value::same_value_zero`] already decides it and is what this calls —
//! a second copy is a copy that will come to disagree about `-0`.

use crate::entry::Context;
use crate::value::{Value, same_value_zero, strict_equals};

/// What a `Map`, a `Set`, a `WeakMap` or a `WeakSet` holds.
///
/// One type for all four rather than two, and the reason is `delete`: a `Set`
/// that stored only keys would need its own shift, its own rehash and its own
/// bounds — so a `Set` stores each member as both key and value. That costs a
/// word per element and buys one implementation of every mutation, where the
/// alternative was a length invariant that depended on which class owned the
/// table.
///
/// The weak pair uses `keys`/`values` and never the index, because its equality
/// is identity rather than SameValueZero — see [`Table::identical`].
#[derive(Default)]
pub(in crate::entry) struct Table {
    keys: Vec<u64>,
    values: Vec<u64>,
    /// The first slot in each bucket, or `-1`.
    heads: Vec<i32>,
    /// The next slot with the same bucket, or `-1`.
    next: Vec<i32>,
    /// One less than the bucket count, so the modulo is an `and`. Zero means
    /// there is no index yet.
    mask: usize,
}

impl Table {
    /// How many entries there are.
    pub(super) fn len(&self) -> usize {
        self.keys.len()
    }

    /// The keys, in insertion order.
    pub(super) fn keys(&self) -> &[u64] {
        &self.keys
    }

    /// Both, paired, in insertion order.
    pub(super) fn entries(&self) -> Vec<(u64, u64)> {
        self.keys.iter().copied().zip(self.values.iter().copied()).collect()
    }

    /// Every value this table holds, keys and values both, for the tracer.
    ///
    /// `WeakMap`/`WeakSet` hold their keys STRONGLY today — [`super::weak`]
    /// says so in its own documentation — so tracing every entry the same way
    /// `Map`/`Set` are traced is correct for what this table currently *is*,
    /// not an oversight of what its name promises. Tracing a key weakly needs
    /// the `(slot, generation)` mechanism `PLAN.md` phase C1 describes; doing
    /// it here without that would free a key still reachable through the
    /// collection.
    pub(in crate::entry) fn trace(&self, out: &mut Vec<u64>) {
        out.extend_from_slice(&self.keys);
        out.extend_from_slice(&self.values);
    }

    /// The value a key holds, if the key is present.
    pub(super) fn get(&self, context: &Context, key: u64) -> Option<u64> {
        Some(self.values[self.slot(context, key)?])
    }

    /// Whether a key is present.
    pub(super) fn has(&self, context: &Context, key: u64) -> bool {
        self.slot(context, key).is_some()
    }

    /// Writes a key, keeping the position it already had if it had one.
    ///
    /// Reassigning in place rather than removing and appending: `m.set(k, 1)`
    /// followed by `m.set(k, 2)` must not move `k` to the end of the iteration
    /// order, which is what the specification says and what a program printing
    /// a map after updating it would otherwise show.
    pub(in crate::entry) fn set(&mut self, context: &Context, key: u64, value: u64) {
        if let Some(at) = self.slot(context, key) {
            self.values[at] = value;
            return;
        }
        let index = self.keys.len();
        self.keys.push(key);
        self.values.push(value);
        self.link(context, key, index);
    }

    /// Removes a key, preserving the order of what is left.
    pub(super) fn remove(&mut self, context: &Context, key: u64) -> bool {
        let Some(at) = self.slot(context, key) else {
            return false;
        };
        self.keys.remove(at);
        self.values.remove(at);
        self.rehash(context, self.keys.len());
        true
    }

    /// Empties it, index and all.
    pub(super) fn clear(&mut self) {
        self.keys.clear();
        self.values.clear();
        self.heads.clear();
        self.next.clear();
        self.mask = 0;
    }

    /// Where a key is, by hash when there is an index and by scan when there is
    /// not.
    ///
    /// The scan is not a fallback for a broken index — it is what an empty
    /// table and an unindexed one both correctly answer, and writing it that
    /// way removes an invariant ("`mask == 0` implies empty") that every caller
    /// would otherwise have to keep.
    fn slot(&self, context: &Context, key: u64) -> Option<usize> {
        if self.mask == 0 {
            return self.keys.iter().position(|held| same_key(context, *held, key));
        }
        let mut at = self.heads[hash_of(context, key) as usize & self.mask];
        while at >= 0 {
            let index = at as usize;
            if same_key(context, self.keys[index], key) {
                return Some(index);
            }
            at = self.next[index];
        }
        None
    }

    /// Chains a freshly appended slot, growing the table when it is half full.
    ///
    /// Half rather than full, because open chaining degrades by chain length
    /// rather than by probe count: at a load factor of one the average chain is
    /// already long enough that the index stops being one.
    fn link(&mut self, context: &Context, key: u64, index: usize) {
        if self.mask == 0 || (index + 1) * 2 > self.mask + 1 {
            self.rehash(context, index + 1);
            return;
        }
        let bucket = hash_of(context, key) as usize & self.mask;
        self.next.push(self.heads[bucket]);
        self.heads[bucket] = index as i32;
    }

    /// Rebuilds the index for `want` entries.
    fn rehash(&mut self, context: &Context, want: usize) {
        let mut capacity = 8;
        while capacity < (want + 1) * 2 {
            capacity *= 2;
        }
        self.mask = capacity - 1;
        self.heads = vec![-1; capacity];
        self.next = vec![-1; self.keys.len()];
        for index in 0..self.keys.len() {
            let bucket = hash_of(context, self.keys[index]) as usize & self.mask;
            self.next[index] = self.heads[bucket];
            self.heads[bucket] = index as i32;
        }
    }

    /// Where a key is, by identity, for the weak pair.
    ///
    /// `===` rather than SameValueZero, because a weak key is required to be a
    /// reference and the two agree on every reference. Kept separate rather
    /// than sharing [`Self::slot`] so that the weak collections never build an
    /// index — an index is what makes iteration order observable, and the weak
    /// pair deliberately has no iteration at all.
    pub(super) fn identical(&self, context: &Context, key: u64) -> Option<usize> {
        self.keys.iter().position(|held| {
            strict_equals(Value(*held), Value(key), |left, right| {
                context.same_text(left, right)
            })
        })
    }

    /// The value at a position [`Self::identical`] found.
    pub(super) fn value_at(&self, at: usize) -> u64 {
        self.values[at]
    }

    /// Writes the value at such a position.
    pub(super) fn set_value_at(&mut self, at: usize, value: u64) {
        self.values[at] = value;
    }

    /// Appends without touching the index.
    ///
    /// Only the weak pair may call this: it has no index to keep, where a
    /// hashed table appended to this way would hold an entry no lookup could
    /// reach.
    pub(super) fn push_unindexed(&mut self, key: u64, value: u64) {
        self.keys.push(key);
        self.values.push(value);
    }

    /// Removes at a position, without rebuilding an index there is none of.
    pub(super) fn remove_at(&mut self, at: usize) {
        self.keys.remove(at);
        self.values.remove(at);
    }
}

/// SameValueZero over two tagged values.
fn same_key(context: &Context, left: u64, right: u64) -> bool {
    same_value_zero(Value(left), Value(right), |a, b| context.same_text(a, b))
}

/// A bucket number for a key.
///
/// The only guarantee this owes is the one direction: if two keys are
/// SameValueZero then their hashes are equal. A collision the other way costs a
/// longer chain and nothing else, which is what lets the reference case be a
/// single bucket.
///
/// # Why an object hashes to zero
///
/// A real engine hashes object **identity**, and this one cannot: a reference
/// is a region index, and the collector this heap is heading for moves cells —
/// so a hash taken from the index would change under a value that did not, and
/// the entry would become unreachable in its own table. Rehashing the world on
/// every collection is the alternative, and it is a decision for the collector
/// rather than for this. Until then every reference lands in bucket zero, which
/// degrades that chain to the linear scan it replaced and stays correct.
fn hash_of(context: &Context, value: u64) -> u32 {
    let value = Value(value);
    if let Some(number) = value.numeric() {
        // One fixed bucket for every NaN, because SameValueZero unites them
        // while the bits do not.
        if number.is_nan() {
            return 0x7fff_ffff;
        }
        // `+0` and `-0` are the same key, so they must be the same bucket; the
        // addition canonicalises the negative one without naming it.
        let number = number + 0.0;
        // The integer path is the common case — an id, an index, a small count
        // — and it reaches the mixer without a multiply. A non-integer enters
        // through a scaled truncation, which collides more and stays correct.
        //
        // `as i32` saturates in Rust where JavaScript's `| 0` wraps. That is a
        // different set of collisions, not a wrong answer, and it is
        // deterministic.
        let mut mixed = number as i32 as u32;
        if f64::from(mixed as i32) != number {
            mixed = (number * 2_654_435_761.0) as i32 as u32;
        }
        // Without the mix, keys that are multiples of a power of two all land
        // in one bucket once the mask is applied — which is exactly what a map
        // keyed by an address or a stride would be.
        mixed ^= mixed >> 16;
        mixed = mixed.wrapping_mul(2_246_822_519);
        mixed ^= mixed >> 13;
        return mixed & 0x7fff_ffff;
    }
    if let Some(cell) = value.as_slot()
        && let Some(text) = context.text_at(cell)
    {
        // FNV-1a over the code UNITS, because that is what a string is here and
        // what `same_units` compares. Hashing the UTF-8 of it would be a second
        // spelling of the same string's identity.
        let mut hash: u32 = 2_166_136_261;
        for unit in text.units() {
            hash ^= u32::from(unit);
            hash = hash.wrapping_mul(16_777_619);
        }
        return hash & 0x7fff_ffff;
    }
    0
}
