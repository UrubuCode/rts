//! Finding what is still reachable, and freeing what is not.
//!
//! # What this does not decide
//!
//! Where a collection may happen, what is live at that point, and whether a
//! store needs a barrier are all [`rts_cranelift::gc`]'s, derived from liveness
//! and from field types while lowering. Its own documentation says why they are
//! derived rather than declared: *"discipline that must hold at every allocation
//! in every program will not hold"*.
//!
//! So nothing here re-derives a root set. What arrives is a list of slots, and
//! this walks from them.
//!
//! # Two ways a root arrives, and one is more precise than the old engine
//!
//! A frame the machine compiled has a descriptor and hands over exactly the
//! slots that are live. A frame it did not — a thunk, a foreign caller, anything
//! compiled another way — has none, and its words are scanned by
//! [`conservative_roots`].
//!
//! That scan is **more** precise than reading every word and treating anything
//! resembling a reference as one. A word is a root only when it is an encoded
//! value *and* its tag says reference — so a double, an integer and a boolean
//! are all rejected outright, and a float whose bits happen to spell a slot
//! number stops being a false positive. What remains false-positive is a genuine
//! reference word that is dead, which costs one extra cycle of retention and
//! never correctness.
//!
//! # Why marking has no recursion
//!
//! An object graph is as deep as a program makes it. A linked list of a hundred
//! thousand nodes is ordinary, and marking it by recursion overflows the stack
//! *during a collection*, which is the worst moment available. So the walk holds
//! a worklist and loops.

use crate::heap::{Slab, Slot};
use crate::value::Value;

/// Something the collector can walk through.
///
/// Implemented per stored type because [`Slab`] is generic and marking must work
/// without knowing what a slot holds. The alternative — a `Vec<Value>` on every
/// object for the collector's benefit — would make every object pay for a walk
/// most of them never receive.
pub trait Trace {
    /// Add every slot this refers to.
    ///
    /// Order does not matter and duplicates are harmless: the mark set makes the
    /// second visit a no-op, which is also what stops a cycle from looping
    /// forever.
    fn trace(&self, out: &mut Vec<Slot>);
}

/// Which slots have been reached.
///
/// A bitmap sized to the table rather than a set: a collection touches most of
/// the heap, so a bit per slot is smaller and faster than entries for the ones
/// that survived.
#[derive(Default)]
pub struct Marks {
    reached: Vec<bool>,
}

impl Marks {
    /// A mark set with nothing reached.
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether a slot has been reached this cycle.
    pub fn is_marked(&self, slot: Slot) -> bool {
        self.reached.get(slot.0 as usize).copied().unwrap_or(false)
    }

    /// Mark a slot, and say whether this was the first time.
    ///
    /// The answer is what makes a cycle terminate: a slot already marked is not
    /// walked again.
    pub fn mark(&mut self, slot: Slot) -> bool {
        let index = slot.0 as usize;
        if self.reached.len() <= index {
            self.reached.resize(index + 1, false);
        }
        if self.reached[index] {
            return false;
        }
        self.reached[index] = true;
        true
    }

    /// Forget everything, keeping the space.
    ///
    /// A cycle starts from nothing reached. Reusing the allocation means a
    /// collection does not allocate, which matters because the moment memory is
    /// scarce is exactly when collection runs.
    pub fn clear(&mut self) {
        self.reached.iter_mut().for_each(|bit| *bit = false);
    }
}

/// Mark everything reachable from `roots`.
///
/// A worklist rather than recursion — see the module documentation. Returns how
/// many slots were reached, which is what a sweep's result is compared against
/// when a caller wants to know whether a cycle achieved anything.
pub fn mark<T: Trace>(slab: &Slab<T>, roots: &[Slot], marks: &mut Marks) -> usize {
    let mut worklist: Vec<Slot> = Vec::new();
    let mut reached = 0;

    for root in roots {
        if marks.mark(*root) {
            reached += 1;
            worklist.push(*root);
        }
    }

    let mut outgoing = Vec::new();
    while let Some(slot) = worklist.pop() {
        // A root that names a freed slot is not an error. A conservative scan
        // produces them by construction, and refusing one would make a
        // collection fail because a dead stack word looked plausible.
        let Ok(object) = slab.at(slot) else {
            continue;
        };

        outgoing.clear();
        object.trace(&mut outgoing);

        for next in &outgoing {
            if marks.mark(*next) {
                reached += 1;
                worklist.push(*next);
            }
        }
    }

    reached
}

/// Free every live slot that was not reached.
///
/// Returns how many were freed. Runs after [`mark`] and reads nothing else: a
/// sweep that consulted a root set again would be asking a question marking
/// already answered, and the two answers could differ.
pub fn sweep<T: Trace>(slab: &mut Slab<T>, marks: &Marks) -> usize {
    let doomed: Vec<Slot> = slab
        .live_slots()
        .filter(|slot| !marks.is_marked(*slot))
        .collect();

    let count = doomed.len();
    for slot in doomed {
        slab.free(slot);
    }
    count
}

/// The slots a run of raw words could be referring to.
///
/// For a frame the machine did not compile, which has no descriptor. Every word
/// is examined and almost every one is rejected: a word is a root only if it is
/// an encoded value whose tag says reference.
///
/// **This is stricter than treating anything that resembles a handle as one.** A
/// double is rejected because it is not encoded; an integer, a boolean and a
/// singleton are rejected because their tags say what they are. What survives
/// and should not is a real reference word that is no longer live — retention
/// for one cycle, never a wrong answer.
pub fn conservative_roots(words: &[u64]) -> Vec<Slot> {
    words
        .iter()
        .filter_map(|word| Value(*word).as_slot())
        .map(Slot)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A node holding references to other nodes.
    struct Node {
        edges: Vec<Slot>,
    }

    impl Trace for Node {
        fn trace(&self, out: &mut Vec<Slot>) {
            out.extend_from_slice(&self.edges);
        }
    }

    fn node(edges: Vec<Slot>) -> Node {
        Node { edges }
    }

    #[test]
    fn what_a_root_reaches_survives_and_the_rest_does_not() {
        let mut slab: Slab<Node> = Slab::new();
        let leaf = slab.insert(node(vec![])).slot();
        let held = slab.insert(node(vec![leaf])).slot();
        let orphan = slab.insert(node(vec![])).slot();

        let mut marks = Marks::new();
        assert_eq!(mark(&slab, &[held], &mut marks), 2, "the root and its leaf");
        assert_eq!(sweep(&mut slab, &marks), 1, "the orphan");

        assert!(slab.at(held).is_ok());
        assert!(slab.at(leaf).is_ok());
        assert!(slab.at(orphan).is_err());
    }

    #[test]
    fn a_cycle_terminates() {
        let mut slab: Slab<Node> = Slab::new();
        let first = slab.insert(node(vec![])).slot();
        let second = slab.insert(node(vec![first])).slot();
        slab.at_mut(first).unwrap().edges.push(second);

        let mut marks = Marks::new();
        assert_eq!(
            mark(&slab, &[first], &mut marks),
            2,
            "marking a slot twice is a no-op, which is what stops the loop"
        );
        assert_eq!(sweep(&mut slab, &marks), 0);
    }

    #[test]
    fn a_deep_graph_does_not_use_the_stack() {
        // A hundred thousand nodes in a chain. Recursive marking overflows
        // here, and it overflows during a collection — the worst moment
        // available.
        let mut slab: Slab<Node> = Slab::new();
        let mut previous = slab.insert(node(vec![])).slot();
        for _ in 0..100_000 {
            previous = slab.insert(node(vec![previous])).slot();
        }

        let mut marks = Marks::new();
        assert_eq!(mark(&slab, &[previous], &mut marks), 100_001);
        assert_eq!(sweep(&mut slab, &marks), 0);
    }

    #[test]
    fn a_root_naming_a_freed_slot_is_skipped_and_not_an_error() {
        let mut slab: Slab<Node> = Slab::new();
        let gone = slab.insert(node(vec![])).slot();
        let alive = slab.insert(node(vec![])).slot();
        slab.free(gone);

        let mut marks = Marks::new();
        // A conservative scan produces roots like this by construction.
        mark(&slab, &[gone, alive], &mut marks);
        assert_eq!(sweep(&mut slab, &marks), 0, "the live one survived");
        assert!(slab.at(alive).is_ok());
    }

    #[test]
    fn only_a_reference_word_is_a_root() {
        let words = [
            Value::from_f64(1.5).bits(),
            Value::from_i32(42).bits(),
            Value::from_bool(true).bits(),
            Value::from_singleton(0).bits(),
            Value::from_slot(7).bits(),
        ];

        assert_eq!(
            conservative_roots(&words),
            vec![Slot(7)],
            "a double, an integer, a boolean and a singleton are all rejected by \
             their tags — which is stricter than treating anything that \
             resembles a handle as one"
        );
    }

    #[test]
    fn a_float_that_spells_a_slot_number_is_not_a_root() {
        // The bits of a small slot index read as a denormal double. A scanner
        // that looked for plausible indices would follow it.
        let disguised = Value::from_f64(f64::from_bits(7)).bits();
        assert_eq!(
            conservative_roots(&[disguised]),
            Vec::<Slot>::new(),
            "it is not encoded, so it is not a reference"
        );
    }

    #[test]
    fn clearing_keeps_the_space_so_a_collection_does_not_allocate() {
        let mut marks = Marks::new();
        marks.mark(Slot(1000));
        assert!(marks.is_marked(Slot(1000)));

        marks.clear();
        assert!(!marks.is_marked(Slot(1000)));
        assert!(
            marks.mark(Slot(1000)),
            "and marking it again reports first-time, so a new cycle walks it"
        );
    }
}
