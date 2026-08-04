//! What the language asks the runtime to do.
//!
//! # Why this list is here and not in the runtime
//!
//! Because membership is a **language** judgement. `add` is an entry point
//! because joining two strings allocates; `to_boolean` is one because the empty
//! string is falsy and finding that out reads the heap. Neither sentence is
//! about a machine, and the machine agrees: it refuses to lower `Inst::Generic`
//! at all rather than deciding which symbol a generic addition should dial.
//!
//! ```text
//! Inst::Generic(..) => Err(LowerError::NotYetLowered { needs: Capability::Calls })
//! ```
//!
//! That refusal is the boundary working. The machine knows a generic operation
//! is a call and declines to say *which* call, because which one is a fact about
//! JavaScript.
//!
//! # Why linkage here is by name, when the engine's rule is by index
//!
//! Index linkage is right for a set one side numbers and the other reads —
//! `TABLE_HASH` exists because an index skew fails quietly where a name fails
//! loudly. This is not that set. Two crates that never see each other's source
//! must agree, and the only thing that makes them agree is that each states the
//! same symbol and the same signature.
//!
//! With names, a disagreement is an unresolved symbol at link time. With
//! indices, it is a call to the wrong function with plausible arguments. The
//! rule was never "indices everywhere" — it was that a *closed set assembled in
//! one place* should not be addressed by string. This set is assembled in two
//! places by construction.
//!
//! # What is not here
//!
//! The machine's own entry points — allocation, the write barrier, the promise
//! operations, the inline-cache miss. Those are `rts_cranelift::symbols::RtEntry`
//! and the machine emits them itself. A language layer naming them would be
//! reaching past the boundary to do work the machine already does.

use rts_cranelift::ir::{FuncId, FuncRegistry, Signature};
use rts_cranelift::repr::Repr;

use crate::emit::UNPROVEN;

/// An operation the language performs by calling the runtime.
///
/// Membership is decided by the machine's rule, quoted rather than paraphrased
/// because the first version of this comment shortened it to "it touches the
/// heap" — and a *narrower* rule in the crate that decides membership would emit
/// an operation as instructions when it should have been a call:
///
/// > An entry point exists if and only if the operation touches the heap, the
/// > operating system, or global mutable state. Pure computation is
/// > instructions.
///
/// — [`rts_cranelift::symbols`]. Every operation below happens to touch the
/// heap; the other two clauses are what will decide the next ones.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum RuntimeOp {
    /// `a + b`.
    ///
    /// Not addition. It converts both operands to primitives and then either
    /// concatenates or adds depending on what came back, and concatenating
    /// allocates.
    Add,

    /// `a === b`.
    ///
    /// Two strings are identical when their *text* is, which reads the heap.
    /// Everything else about strict equality is a comparison, which is why this
    /// is the only one of the three equalities with an entry point so far.
    StrictEquals,

    /// `ToBoolean`.
    ///
    /// Seven falsy values, six of which a comparison settles. The seventh is the
    /// empty string, and finding out whether a string is empty reads its length
    /// from the heap.
    ///
    /// This is what a condition needs, and it is why control flow cannot be
    /// emitted before calls: the machine's `branch` requires a proven `Bool`,
    /// and the only route from a tagged JavaScript value to one runs through
    /// here.
    ToBoolean,

    /// `String(n)`.
    ///
    /// The result is allocated.
    NumberToString,

    /// `a - b`.
    ///
    /// An entry point because `ToNumber` of a string reads the heap. The
    /// operation once both operands are numbers is one instruction, and
    /// proving that is what a type pass buys.
    Subtract,

    /// `a * b`.
    ///
    /// An entry point because `ToNumber` of a string reads the heap. The
    /// operation once both operands are numbers is one instruction, and
    /// proving that is what a type pass buys.
    Multiply,

    /// `a / b`.
    ///
    /// An entry point because `ToNumber` of a string reads the heap. The
    /// operation once both operands are numbers is one instruction, and
    /// proving that is what a type pass buys.
    Divide,

    /// `a % b`.
    ///
    /// An entry point because `ToNumber` of a string reads the heap. The
    /// operation once both operands are numbers is one instruction, and
    /// proving that is what a type pass buys.
    Remainder,

    /// `a < b`.
    ///
    /// An entry point because `ToNumber` of a string reads the heap. The
    /// operation once both operands are numbers is one instruction, and
    /// proving that is what a type pass buys.
    Less,

    /// `a <= b`.
    ///
    /// An entry point because `ToNumber` of a string reads the heap. The
    /// operation once both operands are numbers is one instruction, and
    /// proving that is what a type pass buys.
    LessEqual,

    /// `a > b`.
    ///
    /// An entry point because `ToNumber` of a string reads the heap. The
    /// operation once both operands are numbers is one instruction, and
    /// proving that is what a type pass buys.
    Greater,

    /// `a >= b`.
    ///
    /// An entry point because `ToNumber` of a string reads the heap. The
    /// operation once both operands are numbers is one instruction, and
    /// proving that is what a type pass buys.
    GreaterEqual,
}

impl RuntimeOp {
    /// Every operation, in declaration order.
    pub const ALL: &'static [RuntimeOp] = &[
        RuntimeOp::Add,
        RuntimeOp::StrictEquals,
        RuntimeOp::ToBoolean,
        RuntimeOp::NumberToString,
        RuntimeOp::Subtract,
        RuntimeOp::Multiply,
        RuntimeOp::Divide,
        RuntimeOp::Remainder,
        RuntimeOp::Less,
        RuntimeOp::LessEqual,
        RuntimeOp::Greater,
        RuntimeOp::GreaterEqual,
    ];

    /// The linker name the runtime must define.
    ///
    /// The contract between this crate and whichever runtime is linked. It is
    /// not decorated with a scope prefix: the old engine's `__rtsm_`/`__rtsn_`
    /// convention exists to organise a table of thousands, and a set a reader
    /// sees in one screen is not the failure mode that motivated it.
    pub fn symbol(self) -> &'static str {
        match self {
            RuntimeOp::Add => "__rts_add",
            RuntimeOp::StrictEquals => "__rts_strict_equals",
            RuntimeOp::ToBoolean => "__rts_to_boolean",
            RuntimeOp::NumberToString => "__rts_number_to_string",
            RuntimeOp::Subtract => "__rts_subtract",
            RuntimeOp::Multiply => "__rts_multiply",
            RuntimeOp::Divide => "__rts_divide",
            RuntimeOp::Remainder => "__rts_remainder",
            RuntimeOp::Less => "__rts_less",
            RuntimeOp::LessEqual => "__rts_less_equal",
            RuntimeOp::Greater => "__rts_greater",
            RuntimeOp::GreaterEqual => "__rts_greater_equal",
        }
    }

    /// What it takes and what it gives back.
    ///
    /// Written in the machine's representations rather than in JavaScript's
    /// types, because that is what a call site has to agree about. `UNPROVEN` in
    /// a parameter means "a JavaScript value, nothing established"; `Repr::Bool`
    /// in a return means the runtime has established one, which is exactly what
    /// makes `to_boolean` useful to a branch.
    pub fn signature(self) -> Signature {
        let (params, returns) = match self {
            RuntimeOp::Add => (vec![UNPROVEN, UNPROVEN], vec![UNPROVEN]),
            RuntimeOp::StrictEquals => (vec![UNPROVEN, UNPROVEN], vec![Repr::Bool]),
            RuntimeOp::ToBoolean => (vec![UNPROVEN], vec![Repr::Bool]),
            RuntimeOp::NumberToString => (vec![Repr::F64], vec![UNPROVEN]),
            RuntimeOp::Subtract => (vec![UNPROVEN, UNPROVEN], vec![UNPROVEN]),
            RuntimeOp::Multiply => (vec![UNPROVEN, UNPROVEN], vec![UNPROVEN]),
            RuntimeOp::Divide => (vec![UNPROVEN, UNPROVEN], vec![UNPROVEN]),
            RuntimeOp::Remainder => (vec![UNPROVEN, UNPROVEN], vec![UNPROVEN]),
            RuntimeOp::Less => (vec![UNPROVEN, UNPROVEN], vec![Repr::Bool]),
            RuntimeOp::LessEqual => (vec![UNPROVEN, UNPROVEN], vec![Repr::Bool]),
            RuntimeOp::Greater => (vec![UNPROVEN, UNPROVEN], vec![Repr::Bool]),
            RuntimeOp::GreaterEqual => (vec![UNPROVEN, UNPROVEN], vec![Repr::Bool]),
        };
        Signature {
            params,
            returns,
            ..Signature::default()
        }
    }
}

/// The runtime operations one compilation has declared, declared at most once
/// each.
///
/// A `FuncId` is an index into a registry and carries no name, so the same
/// operation asked for twice must yield the same id or the two call sites are
/// calling two different functions as far as the machine is concerned.
///
/// A fixed-size array rather than a map: the set is closed and small enough that
/// hashing a key would cost more than the lookup it replaces, and an array
/// cannot acquire an entry that `ALL` does not list.
pub struct RuntimeCalls {
    declared: [Option<FuncId>; RuntimeOp::ALL.len()],
}

impl Default for RuntimeCalls {
    fn default() -> Self {
        Self::new()
    }
}

impl RuntimeCalls {
    /// Nothing declared yet.
    pub fn new() -> Self {
        RuntimeCalls {
            declared: [None; RuntimeOp::ALL.len()],
        }
    }

    /// The id for an operation, declaring it the first time it is asked for.
    ///
    /// Lazy rather than declaring all of them up front, so a compilation that
    /// never concatenates does not carry a relocation to the string path. What
    /// a program does not do should not appear in what it links.
    pub fn declare(&mut self, funcs: &mut FuncRegistry, op: RuntimeOp) -> FuncId {
        let slot = op as usize;
        match self.declared[slot] {
            Some(id) => id,
            None => {
                let sig = funcs.declare_signature(op.signature());
                let id = funcs.declare_function(sig);
                self.declared[slot] = Some(id);
                id
            }
        }
    }

    /// Which operations were declared, in declaration order.
    ///
    /// What a linker has to resolve for this compilation. The consumer is
    /// whatever binds a `FuncId` to a symbol — today a test, and `rts-host`
    /// when it exists.
    pub fn declared(&self) -> impl Iterator<Item = (RuntimeOp, FuncId)> + '_ {
        RuntimeOp::ALL
            .iter()
            .enumerate()
            .filter_map(|(slot, op)| self.declared[slot].map(|id| (*op, id)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asking_twice_yields_one_function_not_two() {
        let mut funcs = FuncRegistry::new();
        let mut calls = RuntimeCalls::new();
        let first = calls.declare(&mut funcs, RuntimeOp::Add);
        let second = calls.declare(&mut funcs, RuntimeOp::Add);
        assert_eq!(
            first, second,
            "a FuncId carries no name, so two ids for one operation are two \
             different functions as far as the machine can tell"
        );
        assert_eq!(funcs.len(), 1);
    }

    #[test]
    fn nothing_is_declared_until_something_asks() {
        let mut funcs = FuncRegistry::new();
        let mut calls = RuntimeCalls::new();
        calls.declare(&mut funcs, RuntimeOp::Add);
        // A program that never concatenates must not carry a relocation to the
        // string path, and a compilation that declared everything up front
        // would link every operation into every program.
        assert_eq!(calls.declared().count(), 1);
        assert_eq!(funcs.len(), 1);
    }

    #[test]
    fn the_slot_is_the_discriminant_and_the_list_agrees_with_it() {
        // `declare` indexes by `op as usize`, so a variant added to the enum
        // without being added to ALL — or added to ALL out of order — would
        // write one operation's id into another's slot. Silent, and this is the
        // check that makes it loud.
        for (position, op) in RuntimeOp::ALL.iter().enumerate() {
            assert_eq!(*op as usize, position, "{op:?} is out of place in ALL");
        }
    }

    #[test]
    fn to_boolean_returns_a_proven_boolean_because_a_branch_needs_one() {
        // The reason control flow could not be emitted before calls, stated as
        // a test rather than only as prose: the machine's `branch` refuses
        // anything but `Repr::Bool`, and this is the only route to one from a
        // tagged JavaScript value.
        assert_eq!(RuntimeOp::ToBoolean.signature().returns, vec![Repr::Bool]);
        assert_eq!(RuntimeOp::ToBoolean.signature().params, vec![UNPROVEN]);
    }

    #[test]
    fn every_symbol_is_distinct() {
        let mut seen: Vec<&str> = RuntimeOp::ALL.iter().map(|op| op.symbol()).collect();
        seen.sort_unstable();
        let count = seen.len();
        seen.dedup();
        assert_eq!(seen.len(), count, "two operations claiming one symbol link to one function");
    }
}
