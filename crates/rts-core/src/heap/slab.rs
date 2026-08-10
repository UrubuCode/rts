//! A table of slots, addressed by index.
//!
//! Not an allocator handing out addresses. The machine's 48-bit payload is a
//! **slot index**, and that is what makes a NaN-boxed value safe to scan
//! conservatively: a payload cannot be mistaken for a pointer, because it is not
//! one. A collector reading a stack word that happens to spell a valid index
//! keeps a live object alive for one extra cycle; a collector reading a word
//! that happens to spell a valid *address* follows it.
//!
//! # Why one table per kind rather than one union
//!
//! The tag space already spends one tag on "reference". Splitting the 48-bit
//! payload to re-encode which kind of reference would spend address bits to save
//! a branch — and the branch is one a shape check performs anyway. So a slot's
//! kind is a property of the table it is in, and this type is generic over what
//! a slot holds.
//!
//! The union that unifies them arrives when there is more than one kind to
//! unify, which is not yet.
//!
//! # Generations, and where they are not
//!
//! Sixteen bits of generation do not fit beside 48 bits of slot inside a 48-bit
//! payload. So a live [`Value`] carries the slot alone, and staleness is caught
//! here rather than there.
//!
//! [`Value`]: crate::Value
//!
//! That is sound only because of an invariant worth stating plainly: **a live
//! value keeps its slot reachable**, so a value in a program's hands never names
//! a slot that has been reused. The case where it could — a reference that
//! deliberately outlives its object — is exactly `WeakRef` and
//! `FinalizationRegistry`, and those take a [`Handle`], which carries the
//! generation and can therefore be asked whether it is still valid.

use core::marker::PhantomData;

/// Where something is.
///
/// A `u32` rather than the full 48 bits the payload could hold: 2³² slots is
/// four billion live objects, at which point the table is not the constraint.
/// Keeping it narrow means a [`Handle`] is one 64-bit word rather than two.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
pub struct Slot(pub u32);

/// A reference that can outlive what it names.
///
/// Slot **and** generation, which is what lets a holder ask whether the thing is
/// still there. Every ordinary value carries only the slot and relies on being
/// reachable; this exists for the two places the language deliberately does not
/// keep something alive.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct Handle<T> {
    slot: Slot,
    generation: u32,
    kind: PhantomData<fn() -> T>,
}

impl<T> Handle<T> {
    /// Which slot.
    pub fn slot(self) -> Slot {
        self.slot
    }
}

/// Why a slot could not be read.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Stale {
    /// The slot has been freed and possibly reused.
    ///
    /// Not an error to panic on: a `WeakRef` whose target was collected is
    /// *expected* to answer this, and it is how it answers.
    Freed,
    /// The slot was never in this table.
    ///
    /// Distinguished from [`Stale::Freed`] because they mean different things
    /// about who is wrong. A freed slot is the ordinary end of an object's life;
    /// an out-of-range one means a handle came from somewhere it should not
    /// have.
    OutOfRange,
}

/// A slot's state.
enum Entry<T> {
    Live {
        generation: u32,
        value: T,
    },
    /// A freed slot, holding the next free slot rather than a value.
    ///
    /// The free list lives *in* the freed slots, so it costs no memory of its
    /// own — the space is already there and holds nothing.
    Free {
        generation: u32,
        next: Option<Slot>,
    },
    /// A slot whose generation ran out.
    ///
    /// See [`Slab::free`]: reusing it after the counter wrapped would let a
    /// stale handle validate against a different object, so it is never handed
    /// out again.
    Retired,
}

/// A table of slots.
pub struct Slab<T> {
    entries: Vec<Entry<T>>,
    free: Option<Slot>,
    live: usize,
}

impl<T> Default for Slab<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Slab<T> {
    /// An empty table.
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            free: None,
            live: 0,
        }
    }

    /// How many slots hold something.
    pub fn len(&self) -> usize {
        self.live
    }

    /// Whether none do.
    pub fn is_empty(&self) -> bool {
        self.live == 0
    }

    /// Put something in, reusing a freed slot when there is one.
    ///
    /// Growth never invalidates anything, because an index is an index. That is
    /// the property which later makes a *moving* collector possible without
    /// rewriting every reference in the program — the thing being moved is what
    /// a slot points at, not the slot.
    pub fn insert(&mut self, value: T) -> Handle<T> {
        self.live += 1;

        if let Some(slot) = self.free {
            let (generation, next) = match &self.entries[slot.0 as usize] {
                Entry::Free { generation, next } => (*generation, *next),
                _ => unreachable!("the free list only ever links freed slots"),
            };
            self.free = next;
            self.entries[slot.0 as usize] = Entry::Live { generation, value };
            return Handle {
                slot,
                generation,
                kind: PhantomData,
            };
        }

        let slot = Slot(self.entries.len() as u32);
        self.entries.push(Entry::Live {
            generation: 0,
            value,
        });
        Handle {
            slot,
            generation: 0,
            kind: PhantomData,
        }
    }

    /// Read through a handle, checking that it still names what it named.
    pub fn get(&self, handle: Handle<T>) -> Result<&T, Stale> {
        match self.entries.get(handle.slot.0 as usize) {
            None => Err(Stale::OutOfRange),
            Some(Entry::Live { generation, value }) if *generation == handle.generation => {
                Ok(value)
            }
            Some(_) => Err(Stale::Freed),
        }
    }

    /// Read through a slot, without a generation to check.
    ///
    /// What an ordinary value does, and it is sound for the reason in the module
    /// documentation: a live value keeps its slot reachable, so a slot reached
    /// this way has not been reused. Anything that can outlive its object holds
    /// a [`Handle`] and goes through [`Slab::get`] instead.
    pub fn at(&self, slot: Slot) -> Result<&T, Stale> {
        match self.entries.get(slot.0 as usize) {
            None => Err(Stale::OutOfRange),
            Some(Entry::Live { value, .. }) => Ok(value),
            Some(_) => Err(Stale::Freed),
        }
    }

    /// Read through a slot, mutably.
    pub fn at_mut(&mut self, slot: Slot) -> Result<&mut T, Stale> {
        match self.entries.get_mut(slot.0 as usize) {
            None => Err(Stale::OutOfRange),
            Some(Entry::Live { value, .. }) => Ok(value),
            Some(_) => Err(Stale::Freed),
        }
    }

    /// Take something out, so its slot can be reused.
    ///
    /// The generation increments, which is what makes every existing handle to
    /// this slot answer [`Stale::Freed`] afterwards.
    ///
    /// **When the counter would wrap, the slot is retired instead of reused.**
    /// A wrapped generation makes a very old handle validate against whatever
    /// occupies the slot now — the ABA problem, and here it hands a program a
    /// reference to an object it never had. Retiring costs one slot out of four
    /// billion allocations through it; the alternative costs correctness, once,
    /// silently, after a long uptime.
    pub fn free(&mut self, slot: Slot) -> Option<T> {
        let entry = self.entries.get_mut(slot.0 as usize)?;

        let (generation, value) = match core::mem::replace(entry, Entry::Retired) {
            Entry::Live { generation, value } => (generation, value),
            // Freeing something already free, or retired, changes nothing —
            // and putting the original back is what makes that true.
            other => {
                *entry = other;
                return None;
            }
        };

        self.live -= 1;

        match generation.checked_add(1) {
            Some(next_generation) => {
                *entry = Entry::Free {
                    generation: next_generation,
                    next: self.free,
                };
                self.free = Some(slot);
            }
            // Left as `Retired`, and deliberately not linked into the free list.
            None => {}
        }

        Some(value)
    }

    /// Whether a handle still names what it named.
    pub fn is_live(&self, handle: Handle<T>) -> bool {
        self.get(handle).is_ok()
    }

    /// Every live slot, ascending.
    ///
    /// What a collector sweeps over, which is why it must work without knowing
    /// what put anything in the table.
    pub fn live_slots(&self) -> impl Iterator<Item = Slot> + '_ {
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| matches!(entry, Entry::Live { .. }))
            .map(|(index, _)| Slot(index as u32))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_freed_slot_is_reused_and_the_old_handle_knows() {
        let mut slab: Slab<&str> = Slab::new();

        let first = slab.insert("first");
        assert_eq!(slab.get(first), Ok(&"first"));

        assert_eq!(slab.free(first.slot()), Some("first"));
        assert_eq!(
            slab.get(first),
            Err(Stale::Freed),
            "the generation moved, so the old handle no longer validates"
        );

        let second = slab.insert("second");
        assert_eq!(
            second.slot(),
            first.slot(),
            "the slot is reused — that is the point of freeing it"
        );
        assert_eq!(slab.get(second), Ok(&"second"));
        assert_eq!(
            slab.get(first),
            Err(Stale::Freed),
            "and the old handle still does not reach the new occupant"
        );
    }

    #[test]
    fn a_slot_read_without_a_generation_reaches_whatever_is_there() {
        let mut slab: Slab<i32> = Slab::new();
        let handle = slab.insert(1);
        let slot = handle.slot();

        slab.free(slot);
        assert_eq!(slab.at(slot), Err(Stale::Freed));

        slab.insert(2);
        assert_eq!(
            slab.at(slot),
            Ok(&2),
            "which is exactly why an ordinary value may only do this while it \
             keeps the slot reachable"
        );
    }

    #[test]
    fn an_out_of_range_slot_is_told_apart_from_a_freed_one() {
        let slab: Slab<i32> = Slab::new();
        assert_eq!(slab.at(Slot(0)), Err(Stale::OutOfRange));
        assert_eq!(slab.at(Slot(999)), Err(Stale::OutOfRange));
    }

    #[test]
    fn freeing_twice_changes_nothing() {
        let mut slab: Slab<i32> = Slab::new();
        let handle = slab.insert(1);

        assert_eq!(slab.free(handle.slot()), Some(1));
        assert_eq!(slab.free(handle.slot()), None, "already gone");
        assert_eq!(slab.len(), 0, "and the count did not go negative");
    }

    #[test]
    fn the_free_list_hands_slots_back_in_reverse_order_of_freeing() {
        let mut slab: Slab<i32> = Slab::new();
        let a = slab.insert(1).slot();
        let b = slab.insert(2).slot();

        slab.free(a);
        slab.free(b);

        // Most-recently-freed first, because the list is pushed onto. Recorded
        // as a fact rather than a promise — nothing depends on the order, and
        // asserting it is how a change to it stops being invisible.
        assert_eq!(slab.insert(3).slot(), b);
        assert_eq!(slab.insert(4).slot(), a);
    }

    #[test]
    fn a_slot_whose_generation_ran_out_is_never_handed_out_again() {
        let mut slab: Slab<i32> = Slab::new();
        let handle = slab.insert(1);
        let slot = handle.slot();

        // Reach the last generation the counter can hold.
        for _ in 0..3 {
            slab.free(slot);
            slab.insert(0);
        }
        // Force the wrap by hand — cycling four billion times is not a test.
        if let Some(Entry::Live { generation, .. }) = slab.entries.get_mut(slot.0 as usize) {
            *generation = u32::MAX;
        }

        slab.free(slot);
        let next = slab.insert(2);
        assert_ne!(
            next.slot(),
            slot,
            "reusing it would let a very old handle validate against a new \
             object — the ABA problem, paid in a wrong reference"
        );
    }

    #[test]
    fn a_collector_can_walk_the_table_without_knowing_what_filled_it() {
        let mut slab: Slab<i32> = Slab::new();
        let a = slab.insert(1).slot();
        let _b = slab.insert(2).slot();
        let c = slab.insert(3).slot();
        slab.free(a);

        let live: Vec<Slot> = slab.live_slots().collect();
        assert_eq!(live.len(), 2);
        assert!(!live.contains(&a), "the freed one is not swept over");
        assert!(live.contains(&c));
        assert_eq!(slab.len(), 2);
    }

    #[test]
    fn growth_does_not_move_anything() {
        let mut slab: Slab<usize> = Slab::new();
        let first = slab.insert(0);

        for value in 1..1000 {
            slab.insert(value);
        }

        assert_eq!(
            slab.get(first),
            Ok(&0),
            "an index is an index — which is what later lets a moving collector \
             move objects without rewriting references"
        );
    }
}
